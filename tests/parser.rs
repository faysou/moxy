use std::str::FromStr;

use moxy::Token;
use moxy::ast::{Parse, Parser};
use moxy::token::{Delim, TokenStream, TokenTree};

#[test]
fn peek_and_optional_parses_do_not_advance() {
    let tokens = TokenStream::from_str("fn value").unwrap();
    let parser = Parser::from_tokens(&tokens);
    let remaining = parser.remaining();

    assert!(<Token![fn]>::peek(parser.cursor()));
    assert_eq!(parser.remaining(), remaining);

    let optional = Option::<Token![struct]>::parse(&parser).unwrap();
    assert!(optional.is_none());
    assert_eq!(parser.remaining(), remaining);

    <Token![fn]>::parse(&parser).unwrap();
    assert_eq!(parser.remaining(), remaining - 1);

    let tokens = TokenStream::from_str("fn").unwrap();
    let parser = Parser::from_tokens(&tokens);
    assert!(<Token![struct]>::parse(&parser).is_err());
    assert!(!parser.is_empty());
}

#[test]
fn terminating_repetition_leaves_the_first_nonmatching_token() {
    let tokens = TokenStream::from_str("fn fn struct").unwrap();
    let parser = Parser::from_tokens(&tokens);
    let parsed = parser.parse_while::<Token![fn]>();

    assert_eq!(parsed.len(), 2);
    assert!(<Token![struct]>::peek(parser.cursor()));

    <Token![struct]>::parse(&parser).unwrap();
    assert!(parser.is_empty());
}

#[test]
fn cursor_access_is_bounds_safe_and_tracks_advancement() {
    let empty = TokenStream::new();
    let parser = Parser::from_tokens(&empty);
    assert!(parser.curr().is_none());
    assert!(parser.next().is_none());
    assert!(parser.prev().is_none());
    assert!(parser.nth(usize::MAX).is_none());
    assert!(parser.advance().is_none());

    let tokens = TokenStream::from_str("fn value").unwrap();
    let parser = Parser::from_tokens(&tokens);
    assert!(matches!(parser.curr(), Some(TokenTree::Keyword(_))));
    assert!(matches!(parser.next(), Some(TokenTree::Ident(_))));
    parser.advance();
    assert!(matches!(parser.prev(), Some(TokenTree::Keyword(_))));
    assert!(matches!(parser.curr(), Some(TokenTree::Ident(_))));
}

#[test]
fn group_parsing_creates_an_independent_nested_parser() {
    let tokens = TokenStream::from_str("(fn)").unwrap();
    let parser = Parser::from_tokens(&tokens);
    let inner = parser.parse_group(Delim::Paren).unwrap();
    assert!(parser.is_empty());

    <Token![fn]>::parse(&inner).unwrap();
    assert!(inner.is_empty());
}

#[test]
fn public_parse_rejects_trailing_tokens() {
    assert!(moxy::parse!("fn value" as Token![fn]).is_err());
}

#[test]
fn delimited_lists_that_stop_early_are_errors() {
    for (source, message) in [
        ("fn f(x: u8 y: u8) {}", "unexpected trailing input"),
        ("fn f(x: Vec<u8 u8>) {}", "unexpected trailing input"),
        ("fn f() { g(a b) }", "expected `,` punctuation"),
        ("fn f() { match x { 1 => a b } }", "expected `,` after a match arm"),
        ("fn f() { let s = S { a: 1 b: 2 }; }", "unexpected trailing input"),
        ("fn f() { [1 2] }", "unexpected trailing input"),
        ("fn f() { (1 2) }", "unexpected trailing input"),
        ("fn f() { x.y::<u8 u8>() }", "expected `>` punctuation"),
        ("impl A { fn f() {} @ }", "unexpected trailing input"),
        ("mod m { fn f() {} @ }", "unexpected trailing input"),
    ] {
        let error = moxy::parse!(source as moxy::ast::Item)
            .err()
            .unwrap_or_else(|| panic!("`{source}` parsed"));
        assert_eq!(error.message(), message, "`{source}`");
    }
}

#[test]
fn combined_errors_emit_one_compile_error_each() {
    let tokens = TokenStream::from_str("first second third").unwrap();
    let spans: Vec<moxy::token::Span> = tokens.iter().map(moxy::token::Spanner::span).collect();
    let error = moxy::ast::ParseError::new(spans[0], "first failed")
        .combine(moxy::ast::ParseError::new(spans[1], "second failed"))
        .combine(moxy::ast::ParseError::new(spans[2], "third failed"));

    assert_eq!(
        error.to_compile_error().to_string(),
        "compile_error ! (\"first failed\") ; compile_error ! (\"second failed\") ; compile_error ! (\"third failed\") ;"
    );
}

#[test]
fn spanned_errors_run_from_the_first_token_to_the_last() {
    let tokens = TokenStream::from_str("call(a, b) + Vec<u8>").unwrap();
    let error = moxy::ast::ParseError::new_spanned(&tokens, "unsupported");
    let emitted: Vec<TokenTree> = error.to_compile_error().iter().cloned().collect();
    let TokenTree::Ident(first) = tokens.iter().next().unwrap() else {
        panic!("expected an identifier");
    };
    let last = tokens.iter().last().unwrap().span();

    assert_eq!(error.span(), first.span());
    assert_eq!(emitted[0].span(), first.span());
    assert_eq!(emitted[1].span(), first.span());
    assert_eq!(emitted[3].span(), last);
    assert_eq!(error.to_compile_error().to_string(), "compile_error ! (\"unsupported\") ;");

    let tokens = TokenStream::from_str("(a, b)").unwrap();
    let TokenTree::Group(group) = tokens.iter().next().unwrap() else {
        panic!("expected a group");
    };
    let emitted: Vec<TokenTree> = moxy::ast::ParseError::new_spanned(&tokens, "unsupported")
        .to_compile_error()
        .iter()
        .cloned()
        .collect();
    assert_eq!(emitted[0].span(), group.span.open());
    assert_eq!(emitted[3].span(), group.span.close());
}

#[cfg(feature = "diagnostic")]
#[test]
fn diagnostics_from_spanned_errors_keep_both_ends() {
    let tokens = TokenStream::from_str("call(a, b) + Vec<u8>").unwrap();
    let error = moxy::ast::ParseError::new_spanned(&tokens, "unsupported");
    let diagnostic = moxy::diagnostic::Diagnostic::from(&error);

    assert_eq!(diagnostic.spans(), [error.span(), error.end_span()]);
    let emitted: Vec<TokenTree> = diagnostic.to_compile_error().iter().cloned().collect();
    assert_eq!(emitted[0].span(), error.span());
    assert_eq!(emitted[3].span(), error.end_span());
}

// Runs `parse` on a thread with the 2 MiB stack that spawned threads and
// tests get by default, which input at the depth limit must fit in a debug build
fn on_small_stack<T: Send + 'static>(parse: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(parse)
        .unwrap()
        .join()
        .unwrap()
}

// Every recursive construct of the grammar, as `(name, kind, before, open,
// center, close, after, unbounded)`: `open` and `close` nest around `center`
// inside `before` and `after`. An unbounded construct parses at any depth the
// lexer accepts: macro bodies and attribute arguments, which are tokens, and
// a chain, which holds its steps flat
const NESTED: &[(&str, char, &str, &str, &str, &str, &str, bool)] = &[
    ("neg", 'e', "", "-", "x", "", "", false),
    ("not", 'e', "", "!", "x", "", "", false),
    ("deref", 'e', "", "*", "x", "", "", false),
    ("ref", 'e', "", "& ", "x", "", "", false),
    ("ref mut", 'e', "", "&mut ", "x", "", "", false),
    ("raw ref", 'e', "", "&raw const ", "x", "", "", false),
    ("return", 'e', "", "return ", "x", "", "", false),
    ("break", 'e', "", "break ", "x", "", "", false),
    ("break label", 'e', "", "break 'a ", "x", "", "", false),
    ("yield", 'e', "", "yield ", "x", "", "", false),
    ("closure", 'e', "", "|| ", "x", "", "", false),
    ("closure param", 'e', "", "|x| ", "x", "", "", false),
    ("move closure", 'e', "", "move || ", "x", "", "", false),
    ("async closure", 'e', "", "async || ", "x", "", "", false),
    ("static closure", 'e', "", "static || ", "x", "", "", false),
    ("assign", 'e', "", "a = ", "x", "", "", false),
    ("compound assign", 'e', "", "a += ", "x", "", "", false),
    ("range prefix", 'e', "", ".. ", "x", "", "", false),
    ("range inclusive prefix", 'e', "", "..= ", "x", "", "", false),
    ("assign range", 'e', "a", " = b..", "", "", "", false),
    ("binary unary", 'e', "", "a + -", "x", "", "", true),
    ("cast unary", 'e', "", "-", "x as T", "", "", false),
    ("turbofish", 'e', "a::<", "Vec<", "u8", ">", ">()", false),
    ("paren", 'e', "", "(", "x", ")", "", false),
    ("tuple", 'e', "", "(", "x", ",)", "", false),
    ("array", 'e', "", "[", "x", "]", "", false),
    ("repeat value", 'e', "", "[", "x", "; 1]", "", false),
    ("repeat length", 'e', "", "[x; ", "1", "]", "", false),
    ("block", 'e', "", "{ ", "x", " }", "", false),
    ("call", 'e', "", "f(", "x", ")", "", false),
    ("method", 'e', "", "a.b(", "x", ")", "", false),
    ("index", 'e', "", "a[", "x", "]", "", false),
    ("struct literal", 'e', "", "S { a: ", "x", " }", "", false),
    ("struct base", 'e', "", "S { ..", "x", " }", "", false),
    ("if body", 'e', "", "if a { ", "x", " }", "", false),
    ("else body", 'e', "", "if a {} else { ", "x", " }", "", false),
    ("if condition", 'e', "", "if ", "x", " {}", "", false),
    ("if let", 'e', "", "if let a = ", "x", " {}", "", false),
    ("while body", 'e', "", "while a { ", "x", " }", "", false),
    ("while condition", 'e', "", "while ", "x", " {}", "", false),
    ("loop", 'e', "", "loop { ", "x", " }", "", false),
    ("labeled loop", 'e', "", "'a: loop { ", "x", " }", "", false),
    ("for body", 'e', "", "for x in y { ", "x", " }", "", false),
    ("for iterator", 'e', "", "for x in ", "x", " {}", "", false),
    ("unsafe", 'e', "", "unsafe { ", "x", " }", "", false),
    ("async", 'e', "", "async { ", "x", " }", "", false),
    ("async move", 'e', "", "async move { ", "x", " }", "", false),
    ("const block", 'e', "", "const { ", "x", " }", "", false),
    ("try block", 'e', "", "try { ", "x", " }", "", false),
    ("labeled block", 'e', "", "'a: { ", "x", " }", "", false),
    ("match arm", 'e', "", "match a { _ => ", "x", " }", "", false),
    ("match arm block", 'e', "", "match a { _ => { ", "x", " } }", "", false),
    ("match scrutinee", 'e', "", "match ", "x", " {}", "", false),
    ("match guard", 'e', "", "match a { _ if ", "x", " => {} }", "", false),
    ("closure block", 'e', "", "|| { ", "x", " }", "", false),
    ("closure return type", 'e', "", "|| -> T { ", "x", " }", "", false),
    ("macro call", 'e', "", "m!(", "x", ")", "", true),
    ("macro braces", 'e', "", "m! { ", "x", " }", "", true),
    ("let in block", 'e', "", "{ let a = ", "x", "; a }", "", false),
    ("let else", 'e', "", "{ let a = b else { ", "x", " }; }", "", false),
    ("attribute arguments", 'e', "#[a", "(", "x", ")", "] y", true),
    ("const generic block", 'e', "", "a::<{ ", "1", " }>()", "", false),
    ("closure pattern", 'e', "|", "(", "x", ")", "| x", false),
    ("ref type", 't', "", "&", "u8", "", "", false),
    ("ref mut type", 't', "", "&mut ", "u8", "", "", false),
    ("lifetime ref type", 't', "", "&'a ", "u8", "", "", false),
    ("const pointer", 't', "", "*const ", "u8", "", "", false),
    ("mut pointer", 't', "", "*mut ", "u8", "", "", false),
    ("generics", 't', "", "Vec<", "u8", ">", "", false),
    ("fn return", 't', "", "fn() -> ", "u8", "", "", false),
    (
        "extern fn return",
        't',
        "",
        "unsafe extern \"C\" fn() -> ",
        "u8",
        "",
        "",
        false,
    ),
    ("for fn return", 't', "", "for<'a> fn() -> ", "u8", "", "", false),
    ("impl Fn", 't', "", "impl Fn() -> ", "u8", "", "", false),
    ("dyn Fn", 't', "", "dyn Fn() -> ", "u8", "", "", false),
    ("Fn sugar", 't', "", "Fn(u8) -> ", "u8", "", "", false),
    ("box dyn generics", 't', "", "Box<dyn A<", "u8", ">>", "", false),
    ("assoc type", 't', "", "A<B = ", "u8", ">", "", false),
    ("assoc bound", 't', "", "A<B: ", "C", ">", "", false),
    ("qself", 't', "", "<", "T", " as A>::B", "", false),
    ("type turbofish", 't', "", "A::<", "u8", ">", "", false),
    ("impl generics", 't', "", "impl A<", "u8", ">", "", false),
    ("lifetime generics", 't', "", "A<'a, ", "u8", ">", "", false),
    ("paren type", 't', "", "(", "u8", ")", "", false),
    ("tuple type", 't', "", "(", "u8", ",)", "", false),
    ("slice", 't', "", "[", "u8", "]", "", false),
    ("array type", 't', "", "[", "u8", "; 1]", "", false),
    ("fn params", 't', "", "fn(", "u8", ")", "", false),
    ("Fn args", 't', "", "Fn(", "u8", ")", "", false),
    ("type macro", 't', "", "m!(", "u8", ")", "", true),
    ("array length", 't', "", "[u8; { let x: ", "u8", "; 1 }]", "", false),
    ("const argument", 't', "", "A<{ let x: ", "u8", "; 1 }>", "", false),
    ("ref pattern", 'p', "", "&", "x", "", "", false),
    ("ref mut pattern", 'p', "", "&mut ", "x", "", "", false),
    ("box pattern", 'p', "", "box ", "x", "", "", false),
    ("binding", 'p', "", "a @ ", "x", "", "", false),
    ("mut binding", 'p', "", "ref mut a @ ", "x", "", "", false),
    ("paren pattern", 'p', "", "(", "x", ")", "", false),
    ("tuple pattern", 'p', "", "(", "x", ",)", "", false),
    ("slice pattern", 'p', "", "[", "x", "]", "", false),
    ("tuple struct pattern", 'p', "", "S(", "x", ")", "", false),
    ("struct pattern", 'p', "", "S { a: ", "x", " }", "", false),
    ("const pattern", 'p', "", "const { ", "1", " }", "", false),
    ("pattern turbofish", 'p', "A::<", "Vec<", "u8", ">", ">", false),
    ("pattern macro", 'p', "", "m!(", "x", ")", "", true),
    ("range pattern", 'p', "", "(", "1..=2", ")", "", false),
    ("modules", 'f', "", "mod m { ", "", "}", "", false),
    ("functions", 'f', "", "fn f() { ", "", "}", "", false),
    ("impls", 'f', "", "impl A { fn f() { ", "", "} }", "", false),
    ("traits", 'f', "", "trait A { fn f() { ", "", "} }", "", false),
    ("const items", 'f', "", "const _: () = { ", "", " };", "", false),
    ("static items", 'f', "", "static S: () = { ", "", " };", "", false),
    ("enum discriminants", 'f', "", "enum E { A = { ", "", " 1 } }", "", false),
    ("struct array fields", 'f', "", "struct S([u8; { ", "", " 1 }]);", "", false),
    ("type aliases", 'f', "", "type A = [u8; { ", "", " 1 }];", "", false),
    ("impl consts", 'f', "", "impl A { const X: () = { ", "", " }; }", "", false),
    ("trait consts", 'f', "", "trait A { const X: () = { ", "", " }; }", "", false),
    (
        "extern statics",
        'f',
        "",
        "extern \"C\" { static X: [u8; { ",
        "",
        " 1 }]; }",
        "",
        false,
    ),
    ("let initializers", 'f', "", "fn f() { let a = { ", "", " }; }", "", false),
    ("use groups", 'f', "use ", "a::{", "b", "}", ";", false),
    ("item attributes", 'f', "#[a", "(", "x", ")", "] fn f() {}", true),
    ("where clauses", 'f', "fn f() where T: ", "A<", "B", ">", " {}", false),
    ("generic bounds", 'f', "fn f<T: ", "A<", "B", ">", ">() {}", false),
    ("generic defaults", 'f', "struct S<T = ", "A<", "B", ">", ">;", false),
    ("macro_rules", 'f', "macro_rules! m { ", "(", "", ")", " }", true),
    ("macro 2.0", 'f', "macro m { ", "(", "", ")", " }", true),
    ("item macro", 'f', "", "m! { ", "", " }", "", true),
    ("fn return impl", 'f', "fn f() -> ", "impl Fn() -> ", "u8", "", " {}", false),
    (
        "closure return impl",
        'e',
        "|| -> ",
        "impl Fn() -> ",
        "u8",
        "",
        " { x }",
        false,
    ),
    ("nested blocks", 'b', "", "{ ", "x", " }", "", false),
    ("ifs in a block", 'b', "{ ", "if a { ", "x", " }", " }", false),
];

// A nested construct `n` levels deep
fn nested(case: usize, n: usize) -> String {
    let (_, _, before, open, center, close, after, _) = NESTED[case];
    format!("{before}{}{center}{}{after}", open.repeat(n), close.repeat(n))
}

// The deepest nesting of a construct that the lexer accepts, at most 50,000
// levels: as deep as a source, or a stream from the compiler or proc-macro2,
// can nest it
fn deepest_lexed(case: usize) -> usize {
    let lexes = |n: usize| nested(case, n).parse::<TokenStream>().is_ok();

    if lexes(50_000) {
        return 50_000;
    }

    let (mut low, mut high) = (1, 50_000);

    while high - low > 1 {
        let middle = (low + high) / 2;

        if lexes(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }

    low
}

// Parses, skips, and peeks a source as `T` on a 2 MiB thread, and emits,
// clones, and drops a tree that parses; an overflow aborts the test
fn parse_skip_and_peek<T>(source: String) -> (Result<(), String>, bool, bool)
where
    T: Parse + moxy::token::ToTokenStream + Clone + 'static,
{
    on_small_stack(move || {
        let tokens: TokenStream = source.parse().unwrap();
        let parser = Parser::from_tokens(&tokens);
        let peeked = T::peek(parser.cursor());
        let skipped = T::skip(parser.cursor()).is_some_and(|cursor| cursor.is_empty());
        let parsed = match moxy::parse!(tokens as T) {
            Ok(node) => {
                let _ = node.to_token_stream();
                drop(node.clone());
                Ok(())
            }
            Err(error) => Err(error.message().to_string()),
        };
        (parsed, skipped, peeked)
    })
}

#[test]
fn every_construct_nested_to_the_lexer_limit_reports_the_depth() {
    use moxy::ast::{Expr, File, Pattern, StmtBlock, Type};

    let message = format!("nesting is deeper than {} levels", moxy::token::MAX_DEPTH);

    for (case, &(name, kind, .., unbounded)) in NESTED.iter().enumerate() {
        let run = match kind {
            'e' => parse_skip_and_peek::<Expr>,
            't' => parse_skip_and_peek::<Type>,
            'p' => parse_skip_and_peek::<Pattern>,
            'b' => parse_skip_and_peek::<StmtBlock>,
            _ => parse_skip_and_peek::<File>,
        };

        // Two levels are well formed
        assert_eq!(run(nested(case, 2)), (Ok(()), true, true), "{name} at 2 levels");

        // As deep as the lexer accepts, the parser reports its own depth error
        // and skipping fails, unless the construct is unbounded
        let n = deepest_lexed(case);
        let expected = if unbounded {
            (Ok(()), true, true)
        } else {
            (Err(message.clone()), false, true)
        };
        assert_eq!(run(nested(case, n)), expected, "{name} at {n} levels");
    }
}

#[test]
fn the_lexer_and_the_parser_report_the_depth_apart() {
    let depth = moxy::token::MAX_DEPTH;
    let lexer = on_small_stack(move || {
        format!("{}{}", "[".repeat(depth + 1), "]".repeat(depth + 1))
            .parse::<TokenStream>()
            .unwrap_err()
            .to_string()
    });
    assert_eq!(lexer, format!("delimiters nest deeper than {depth} levels"));

    let parser = moxy::parse!((format!("{}a", "!".repeat(depth + 1))) as moxy::ast::Expr)
        .err()
        .unwrap();
    assert_eq!(parser.message(), format!("nesting is deeper than {depth} levels"));
}

#[test]
fn input_nested_to_the_depth_limit_parses_on_a_small_stack() {
    let depth = moxy::token::MAX_DEPTH - 1;

    let parsed = on_small_stack(move || {
        let parens = format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
        // The arms of a match take a level of their own
        let arms = format!("{}a{}", "match a { _ => ".repeat(depth - 2), " }".repeat(depth - 2));
        // A closure parameter is a pattern, which keeps one level of room
        let closures = format!("{}a", "|x| ".repeat(depth - 1));
        let generics = format!("{}u8{}", "Vec<".repeat(depth), ">".repeat(depth));
        // A pattern keeps one level of room for the peeks that choose its
        // form
        let patterns = format!("{}a{}", "(".repeat(depth - 1), ")".repeat(depth - 1));
        // A module's item and its body each take a level
        let modules = format!("{}{}", "mod m { ".repeat(depth / 2), "}".repeat(depth / 2));
        [
            moxy::parse!(parens as moxy::ast::Expr).is_ok(),
            moxy::parse!(arms as moxy::ast::Expr).is_ok(),
            moxy::parse!(closures as moxy::ast::Expr).is_ok(),
            moxy::parse!(generics as moxy::ast::Type).is_ok(),
            moxy::parse!(patterns as moxy::ast::Pattern).is_ok(),
            moxy::parse!(modules as moxy::ast::File).is_ok(),
        ]
    });

    assert_eq!(parsed, [true; 6]);
}

#[test]
fn an_error_inside_a_list_of_items_is_reported_where_it_is() {
    let source = "mod m {\n    fn f() {}\n    fn g() { let = 1; }\n}";
    let error = moxy::parse!(source as moxy::ast::File).err().unwrap();
    assert_eq!(error.span().start().line(), 2);
    assert_ne!(error.message(), "unexpected trailing input");

    let source = "fn f() {}\nstruct S { a: }";
    let error = moxy::parse!(source as moxy::ast::File).err().unwrap();
    assert_eq!(error.span().start().line(), 1);
    assert_ne!(error.message(), "unexpected trailing input");
}

// Runs `parse` on a worker thread and fails if it takes longer than a limit
// no linear parse comes near, so an exponential one fails instead of hanging
fn within_limit(label: &str, parse: impl FnOnce() -> bool + Send + 'static) {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || sender.send(parse()).unwrap())
        .unwrap();
    let parsed = receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("{label} took longer than 10 seconds"));
    assert!(parsed, "{label} did not parse");
}

#[test]
fn nested_parentheses_in_types_and_block_chains_parse_in_linear_time() {
    let depth = 64;
    let parens = |inner: &str| format!("{}{inner}{}", "(".repeat(depth), ")".repeat(depth));
    let tuples = format!("{}T{}", "(".repeat(depth), ",)".repeat(depth));
    let qualified = format!("A::<{}>::B", parens("T"));
    let chain = format!("{}x{}", "{ ".repeat(depth), " }.a()".repeat(depth));

    for (label, source) in [
        ("parameter type", format!("fn f(a: {}) {{}}", parens("T"))),
        ("tuple parameter type", format!("fn f(a: {tuples}) {{}}")),
        ("receiver type", format!("impl A {{ fn f(self: {}) {{}} }}", parens("T"))),
        ("trait function", format!("trait A {{ fn f(a: {}); }}", parens("T"))),
        ("foreign function", format!("extern \"C\" {{ fn f(a: {}); }}", parens("T"))),
        ("pattern path", format!("fn f() {{ let {qualified} = x; }}")),
        ("block chain", format!("fn f() {{ {chain}; }}")),
    ] {
        within_limit(label, move || moxy::parse!(source as moxy::ast::File).is_ok());
    }
}

#[test]
fn block_like_statements_at_the_depth_limit_skip_in_linear_time() {
    use moxy::ast::{Expr, StmtBlock};

    // A block-like statement that failed to skip at the limit was skipped
    // again as an operand at every enclosing level
    let depth = moxy::token::MAX_DEPTH;

    for open in ["if a { ", "if a {} else { ", "while a { ", "loop { ", "unsafe { ", "'a: { "] {
        let source = format!("{}x{}", open.repeat(depth), " }".repeat(depth));
        within_limit(open, move || {
            let tokens: TokenStream = source.parse().unwrap();
            Expr::skip(Parser::from_tokens(&tokens).cursor()).is_none()
        });
    }

    let source = format!("{{ {}x{} }}", "if a { ".repeat(depth - 1), " }".repeat(depth - 1));
    within_limit("ifs in a block", move || {
        let tokens: TokenStream = source.parse().unwrap();
        StmtBlock::skip(Parser::from_tokens(&tokens).cursor()).is_none()
    });
}

#[test]
fn members_skip_to_the_depth_that_they_parse_to() {
    use moxy::ast::File;

    let message = format!("nesting is deeper than {} levels", moxy::token::MAX_DEPTH);

    // Each as deep as the lexer accepts, past the depth that members parse to
    for (open, close, groups) in [
        ("impl A { fn f() { ", "} }", 2),
        ("trait A { fn f() { ", "} }", 2),
        ("extern \"C\" { static X: [u8; { ", " 1 }]; }", 3),
    ] {
        let depth = moxy::token::MAX_DEPTH / groups;
        let source = format!("{}{}", open.repeat(depth), close.repeat(depth));
        let message = message.clone();
        let (error, skipped) = on_small_stack(move || {
            let tokens: TokenStream = source.parse().unwrap();
            let parser = Parser::from_tokens(&tokens);
            let skipped = File::skip(parser.cursor()).is_some();
            (File::parse(&parser).err().map(|error| error.message().to_string()), skipped)
        });
        assert_eq!((error.as_deref(), skipped), (Some(message.as_str()), false), "{open}");
    }
}

#[test]
fn many_let_statements_parse_in_linear_time() {
    // Peeking for an or-pattern read to the end of the block at every `let`
    let source = format!("fn f() {{ {} }}", "let a = 1; ".repeat(20_000));
    within_limit("let statements", move || moxy::parse!(source as moxy::ast::File).is_ok());

    let pattern: moxy::ast::Pattern = moxy::parse!("a | b").unwrap();
    assert!(matches!(pattern, moxy::ast::Pattern::Or(_)));
}

// Parses a source on a 2 MiB thread and, when it parses, emits, spans, clones,
// and drops the tree; deep input must give the depth error instead of
// overflowing the stack, which would abort the test
fn traverse_on_small_stack<T>(source: String) -> Option<String>
where
    T: moxy::ast::Parse + moxy::token::ToTokenStream + moxy::token::Spanner + Clone,
{
    on_small_stack(move || match moxy::parse!(source as T) {
        Ok(node) => {
            let _ = node.to_token_stream();
            let _ = node.span();
            drop(node.clone());
            drop(node);
            None
        }
        Err(error) => Some(error.message().to_string()),
    })
}

// Every traversal a user can run on a tree: emitting, spanning, cloning,
// comparing, debug printing, visiting, formatting, serializing, and dropping.
// AST nodes do not implement Hash.
fn traverse<T>(node: T, visit: fn(&T))
where
    T: moxy::token::ToTokenStream + moxy::token::Spanner + Clone + PartialEq + std::fmt::Debug,
    T: moxy::fmt::Format + serde::Serialize,
{
    let _ = node.to_token_stream();
    let _ = node.span();
    let copy = node.clone();
    assert!(copy == node);
    let _ = format!("{node:?}");
    visit(&node);
    let _ = moxy::fmt!(&node);
    let _ = serde_json::to_string(&node).unwrap();
    drop(copy);
    drop(node);
}

struct Walk;

impl moxy::ast::Visit<'_> for Walk {}

// The deepest nesting of a kind that parses on a 2 MiB thread, found by
// bisection, with every traversal run on the tree at that depth; past it
// the parse reports the depth error
fn deepest<T>(make: fn(usize) -> String, visit: fn(&T)) -> usize
where
    T: moxy::ast::Parse + moxy::token::ToTokenStream + moxy::token::Spanner + Clone + PartialEq + std::fmt::Debug,
    T: moxy::fmt::Format + serde::Serialize + 'static,
{
    let message = format!("nesting is deeper than {} levels", moxy::token::MAX_DEPTH);
    let parses = move |n: usize| {
        let source = make(n);
        let message = message.clone();

        // Deeper than the lexer accepts: the parser never sees it
        if source.parse::<TokenStream>().is_err() {
            return false;
        }

        on_small_stack(move || match moxy::parse!(source as T) {
            Ok(node) => {
                traverse(node, visit);
                true
            }
            Err(error) => {
                assert_eq!(error.message(), message, "at {n} levels");
                false
            }
        })
    };

    let (mut low, mut high) = (1, 400);
    assert!(!parses(high), "400 levels parsed");

    while high - low > 1 {
        let middle = (low + high) / 2;

        if parses(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }

    low
}

#[test]
fn deep_trees_of_every_kind_parse_and_traverse_or_report_the_depth() {
    use moxy::ast::{Expr, File, Pattern, Type, Visit};

    let expr: fn(&Expr) = |node| Walk.visit_expr(node);
    let ty: fn(&Type) = |node| Walk.visit_type(node);
    let pattern: fn(&Pattern) = |node| Walk.visit_pattern(node);
    let file: fn(&File) = |node| node.items.iter().for_each(|item| Walk.visit_item(item));

    use moxy::token::MAX_DEPTH;

    let block: fn(&moxy::ast::StmtBlock) = |node| Walk.visit_stmt_block(node);

    // Each kind with the deepest nesting that the depth count accepts, which
    // does not depend on the machine
    let kinds = [
        (
            "nested blocks",
            MAX_DEPTH - 2,
            deepest::<Expr>(|n| format!("{}x{}", "{ ".repeat(n), " }".repeat(n)), expr),
        ),
        (
            "modules",
            MAX_DEPTH / 2,
            deepest::<File>(|n| format!("{}{}", "mod m { ".repeat(n), "}".repeat(n)), file),
        ),
        (
            "let initializers in blocks",
            (MAX_DEPTH - 1) / 3,
            deepest::<Expr>(|n| format!("{}1{}", "{ let a = ".repeat(n), "; a }".repeat(n)), expr),
        ),
        (
            "const blocks in let patterns",
            (MAX_DEPTH - 2) / 3,
            deepest::<File>(
                |n| format!("fn f() {{ {}1{} }}", "let const { ".repeat(n), " } = 1;".repeat(n)),
                file,
            ),
        ),
        (
            "method call arguments",
            MAX_DEPTH - 1,
            deepest::<Expr>(|n| format!("{}x{}", "a.b(".repeat(n), ")".repeat(n)), expr),
        ),
        (
            "nested ifs in a block",
            MAX_DEPTH - 2,
            deepest::<moxy::ast::StmtBlock>(|n| format!("{{ {}x{} }}", "if a { ".repeat(n), " }".repeat(n)), block),
        ),
        (
            "operator ladders in parentheses",
            (MAX_DEPTH - 1) / 9,
            deepest::<Expr>(|n| format!("{}0{}", "(0||0&&0==0|0^0&0<<0+0*".repeat(n), ")".repeat(n)), expr),
        ),
        (
            "operator ladders in closures",
            (MAX_DEPTH - 1) / 9,
            deepest::<Expr>(|n| format!("{}0", "|| 0||0&&0==0|0^0&0<<0+0*".repeat(n)), expr),
        ),
        (
            "match arms",
            MAX_DEPTH - 3,
            deepest::<Expr>(|n| format!("{}x{}", "match a { _ => ".repeat(n), " }".repeat(n)), expr),
        ),
        (
            "match guards",
            (MAX_DEPTH - 2) / 2,
            deepest::<Expr>(|n| format!("{}x{}", "match a { _ if ".repeat(n), " => {} }".repeat(n)), expr),
        ),
        (
            "parentheses",
            MAX_DEPTH - 1,
            deepest::<Expr>(|n| format!("{}a{}", "(".repeat(n), ")".repeat(n)), expr),
        ),
        (
            "struct literals",
            (MAX_DEPTH - 1) / 2,
            deepest::<Expr>(|n| format!("{}a{}", "S { a: ".repeat(n), " }".repeat(n)), expr),
        ),
        (
            "closures with a return type",
            (MAX_DEPTH - 1) / 3,
            deepest::<Expr>(|n| format!("{}1{}", "|| -> T { ".repeat(n), " }".repeat(n)), expr),
        ),
        (
            "blocks with a method call",
            MAX_DEPTH - 2,
            deepest::<Expr>(|n| format!("{}x{}", "{ ".repeat(n), " }.a()".repeat(n)), expr),
        ),
        (
            "function sugar",
            (MAX_DEPTH - 1) / 2,
            deepest::<Type>(|n| format!("{}T", "Fn() -> ".repeat(n)), ty),
        ),
        (
            "impl function sugar",
            (MAX_DEPTH - 1) / 2,
            deepest::<Type>(|n| format!("{}T", "impl Fn() -> ".repeat(n)), ty),
        ),
        (
            "associated types",
            (MAX_DEPTH - 1) / 3,
            deepest::<Type>(|n| format!("{}u8{}", "A<B = ".repeat(n), ">".repeat(n)), ty),
        ),
        (
            "associated bounds",
            (MAX_DEPTH - 1) / 3,
            deepest::<Type>(|n| format!("{}C{}", "A<B: ".repeat(n), ">".repeat(n)), ty),
        ),
        (
            "function pointers",
            (MAX_DEPTH - 1) / 2,
            deepest::<Type>(|n| format!("{}u8{}", "fn(".repeat(n), ")".repeat(n)), ty),
        ),
        (
            "array lengths",
            (MAX_DEPTH - 1) / 4,
            deepest::<Type>(|n| format!("{}u8{}", "[u8; { let x: ".repeat(n), "; 1 }]".repeat(n)), ty),
        ),
        (
            "generic arguments",
            MAX_DEPTH - 1,
            deepest::<Type>(|n| format!("{}u8{}", "Vec<".repeat(n), ">".repeat(n)), ty),
        ),
        (
            "struct patterns",
            (MAX_DEPTH - 1) / 2,
            deepest::<Pattern>(|n| format!("{}x{}", "S { a: ".repeat(n), " }".repeat(n)), pattern),
        ),
        (
            "impls",
            (MAX_DEPTH + 1) / 5,
            deepest::<File>(|n| format!("{}{}", "impl A { fn f() { ".repeat(n), "} }".repeat(n)), file),
        ),
        (
            "traits",
            (MAX_DEPTH + 1) / 5,
            deepest::<File>(|n| format!("{}{}", "trait A { fn f() { ".repeat(n), "} }".repeat(n)), file),
        ),
        (
            "functions",
            MAX_DEPTH / 3,
            deepest::<File>(|n| format!("{}{}", "fn f() { ".repeat(n), "}".repeat(n)), file),
        ),
    ];

    for (kind, expected, reached) in kinds {
        assert_eq!(reached, expected, "{kind}");
    }
}

#[test]
fn a_pattern_at_the_depth_limit_reports_the_depth() {
    let message = format!("nesting is deeper than {} levels", moxy::token::MAX_DEPTH);

    for depth in [140, 142, 143, 144] {
        let source = format!("{}1..=2{}", "(".repeat(depth), ")".repeat(depth));
        let result = on_small_stack(move || {
            moxy::parse!(source as moxy::ast::Pattern)
                .map(|_| ())
                .map_err(|error| error.message().to_string())
        });

        if let Err(error) = result {
            assert_eq!(error, message, "at {depth} levels");
        }
    }
}

#[test]
fn nested_ranges_without_a_start_count_against_the_depth() {
    let message = format!("nesting is deeper than {} levels", moxy::token::MAX_DEPTH);
    let source = format!("{}a", ".. ".repeat(200_000));
    assert_eq!(traverse_on_small_stack::<moxy::ast::Expr>(source), Some(message));
    assert_eq!(
        traverse_on_small_stack::<moxy::ast::Expr>(format!("{}a", ".. ".repeat(100))),
        None
    );
}

#[test]
fn assignments_after_ranges_count_against_the_depth() {
    let message = format!("nesting is deeper than {} levels", moxy::token::MAX_DEPTH);
    let source = format!("a{}", " = b..".repeat(100_000));
    assert_eq!(traverse_on_small_stack::<moxy::ast::Expr>(source.clone()), Some(message));

    // Skipping holds each assignment's level as parsing does
    let skipped = on_small_stack(move || {
        let tokens: TokenStream = source.parse().unwrap();
        <moxy::ast::Expr as Parse>::skip(Parser::from_tokens(&tokens).cursor()).is_some()
    });
    assert!(!skipped);
    assert_eq!(
        traverse_on_small_stack::<moxy::ast::Expr>(format!("a{}", " = b..".repeat(50))),
        None
    );
}

#[test]
fn blocks_count_against_the_same_depth_as_other_nesting() {
    // Blocks counted only against the lexer's own limit, so blocks and
    // closures together nested past the stack
    let message = format!("nesting is deeper than {} levels", moxy::token::MAX_DEPTH);
    let source = format!("{}{}a{}", "if a { ".repeat(140), "|x| ".repeat(141), " }".repeat(140));
    assert_eq!(traverse_on_small_stack::<moxy::ast::Expr>(source), Some(message));

    let source = format!("{}{}a{}", "if a { ".repeat(60), "|x| ".repeat(60), " }".repeat(60));
    assert_eq!(traverse_on_small_stack::<moxy::ast::Expr>(source), None);
}

#[test]
fn patterns_with_blocks_in_their_bounds_and_paths_parse_in_linear_time() {
    // A range peek skipped a bound's block and each path form skipped the
    // path again, which doubled the work per level
    let depth = 32;
    let consts = format!("fn f() {{ {}1{} }}", "let const { ".repeat(depth), " } = 1;".repeat(depth));
    let turbofish = format!(
        "fn f() {{ let A::<{}0{}>::B = 1; }}",
        "{ let A::<".repeat(depth),
        ">::B = 1; 0 }".repeat(depth)
    );
    let ranges = format!(
        "fn f() {{ match x {{ {}0{} => 1 }} }}",
        "A::<{ match x { 1..=2 => ".repeat(8),
        " } }>::B..=C".repeat(8)
    );

    for (label, source) in [
        ("const patterns", consts),
        ("turbofish patterns", turbofish),
        ("ranges", ranges),
    ] {
        within_limit(label, move || moxy::parse!(source as moxy::ast::File).is_ok());
    }

    for source in [
        "1..=2",
        "-1..",
        "A::B..=C",
        "<T>::A..",
        "const { 1 }..=2",
        "a..=b",
        "..=b",
        "..b",
    ] {
        let pattern: moxy::ast::Pattern = moxy::parse!(source).unwrap_or_else(|error| panic!("`{source}`: {error}"));
        assert!(matches!(pattern, moxy::ast::Pattern::Range(_)), "`{source}`");
    }

    for source in ["ref a..=b", "S(x)..=b", "m!().."] {
        assert!(moxy::parse!(source as moxy::ast::Pattern).is_err(), "`{source}`");
    }
}

#[test]
fn receivers_and_trait_aliases_with_blocks_in_their_types_parse_in_linear_time() {
    // The receiver peek skipped the receiver's type and the alias peek the
    // trait's generics before each parsed them again
    let depth = 18;
    let receivers = format!(
        "{}u8{}",
        "impl A { fn f(self: [u8; { ".repeat(depth),
        "; 1 }]) {} }".repeat(depth)
    );
    let aliases = format!("{}u8{}", "trait A<T = [u8; { ".repeat(depth), "; 1 }]> = B;".repeat(depth));

    for (label, source) in [("receivers", receivers), ("trait aliases", aliases)] {
        within_limit(label, move || moxy::parse!(source as moxy::ast::File).is_ok());
    }

    let item: moxy::ast::Item = moxy::parse!("trait A<T> = B<T> + C;").unwrap();
    assert!(item.is_trait_alias());
    let item: moxy::ast::Item = moxy::parse!("unsafe trait A<T>: B {}").unwrap();
    assert!(item.is_trait());
}

#[test]
fn chains_of_any_length_parse_and_traverse_on_a_small_stack() {
    use moxy::ast::{Expr, File, Visit, VisitMut};
    use moxy::token::{ToTokenStream, TokenStream};

    struct WalkMut;

    impl VisitMut for WalkMut {}

    // The steps of the one chain at the top of each tree
    fn steps(expression: &Expr) -> usize {
        match expression {
            Expr::BinaryChain(chain) => chain.operands.len() - 1,
            Expr::Postfix(chain) => chain.ops.len(),
            Expr::CastChain(chain) => chain.casts.len(),
            Expr::IfChain(chain) => chain.else_ifs.len(),
            _ => 0,
        }
    }

    let steps_wanted = 100_000;

    for (source, wanted) in [
        (format!("a{}", " + a".repeat(steps_wanted)), steps_wanted),
        (format!("a{}", ".b()".repeat(steps_wanted)), steps_wanted),
        (format!("a{}", "?".repeat(steps_wanted)), steps_wanted),
        (format!("a{}", " as T".repeat(steps_wanted)), steps_wanted),
        (
            format!("if a {{}}{} else {{}}", " else if a {}".repeat(steps_wanted)),
            steps_wanted,
        ),
        // A sum of method chains that end in casts
        (format!("a{}", " + a.b()? as T".repeat(steps_wanted / 4)), steps_wanted / 4),
    ] {
        let checked = on_small_stack(move || {
            let lexed: TokenStream = source.parse().unwrap();
            let mut node: Expr = moxy::parse!(source).unwrap();
            assert_eq!(steps(&node), wanted);
            assert_eq!(node.to_token_stream().to_string(), lexed.to_string());
            WalkMut.visit_expr_mut(&mut node);
            let formatted = moxy::fmt!(&node).unwrap();
            let again: Expr = moxy::parse!(formatted).unwrap();
            assert!(again == node);
            traverse(node, |node| Walk.visit_expr(node));
            true
        });
        assert!(checked);
    }

    let source = format!("use {}a;", "a::".repeat(steps_wanted));
    let checked = on_small_stack(move || {
        let lexed: TokenStream = source.parse().unwrap();
        let node: File = moxy::parse!(source).unwrap();
        let path = node.items[0].as_use().unwrap().tree.as_path_chain().unwrap();
        assert_eq!(path.segments.len(), steps_wanted);
        assert_eq!(node.to_token_stream().to_string(), lexed.to_string());
        traverse(node, |node| node.items.iter().for_each(|item| Walk.visit_item(item)));
        true
    });
    assert!(checked);
}

#[test]
fn a_chain_of_atoms_costs_no_level_of_its_own() {
    // `(1 + 1)` costs what its parentheses cost; a chain nests a level only
    // inside an operand of a weaker chain
    let parens = |n: usize| format!("{}1{}", "(1 + ".repeat(n), ")".repeat(n));
    let depth = moxy::token::MAX_DEPTH - 1;
    assert!(moxy::parse!((parens(depth)) as moxy::ast::Expr).is_ok());

    let message = format!("nesting is deeper than {} levels", moxy::token::MAX_DEPTH);
    let error = moxy::parse!((parens(depth + 1)) as moxy::ast::Expr).err().unwrap();
    assert_eq!(error.message(), message);
}

// A proc-macro2 stream of `n` nested groups
#[cfg(feature = "proc-macro2")]
fn nested_proc_macro2_groups(n: usize) -> proc_macro2::TokenStream {
    use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};

    (0..n).fold(TokenStream::new(), |stream, _| {
        TokenTree::Group(Group::new(Delimiter::Brace, stream)).into()
    })
}

#[cfg(feature = "proc-macro2")]
#[test]
fn a_token_stream_nested_past_the_depth_limit_is_refused_when_converted() {
    use moxy::token::TryIntoTokenStream;

    // Streams from the compiler or proc-macro2 are not lexed, so converting
    // one counts its groups as the lexer does, and no deeper stream exists for
    // the traversals that recurse per group
    let depth = moxy::token::MAX_DEPTH;
    let (refused, parsed, converted) = on_small_stack(move || {
        let refused = nested_proc_macro2_groups(50_000)
            .try_into_token_stream()
            .unwrap_err()
            .to_string();
        let parsed = moxy::parse!((nested_proc_macro2_groups(depth + 1)) as moxy::ast::StmtBlock)
            .err()
            .map(|error| error.message().to_string());
        let converted = nested_proc_macro2_groups(depth).try_into_token_stream().is_ok();
        (refused, parsed, converted)
    });
    let message = format!("delimiters nest deeper than {depth} levels");
    assert_eq!(
        (refused.as_str(), parsed.as_deref(), converted),
        (message.as_str(), Some(message.as_str()), true)
    );
}

#[cfg(feature = "proc-macro2")]
#[test]
fn converting_a_token_stream_past_the_depth_limit_without_an_error_panics() {
    let depth = moxy::token::MAX_DEPTH;
    let panicked = std::panic::catch_unwind(|| moxy::token::TokenStream::from(nested_proc_macro2_groups(depth + 1)));
    let message = panicked
        .err()
        .and_then(|payload| payload.downcast::<String>().ok())
        .map(|message| *message);
    let expected = format!("delimiters nest deeper than {depth} levels; use `try_into_token_stream` to get this as an error");
    assert_eq!(message, Some(expected));
}

#[test]
fn an_error_quotes_a_group_by_its_delimiters() {
    // A message that printed the whole group printed every macro body it met
    let error = moxy::parse!("{ a b c }" as moxy::token::Ident).err().unwrap();
    assert_eq!(error.message(), "expected ident, received \"{ ... }\"");

    let error = moxy::parse!("()" as moxy::token::Ident).err().unwrap();
    assert_eq!(error.message(), "expected ident, received \"()\"");
}

#[test]
fn parsing_a_token_stream_borrows_it() {
    use moxy::token::TryIntoTokenStream;

    // A borrowed stream reaches the parser as itself, not as a copy
    let tokens: moxy::token::TokenStream = "a + b".parse().unwrap();
    let address = (&tokens).with_token_stream(std::ptr::from_ref).unwrap();
    assert_eq!(address, std::ptr::from_ref(&tokens));

    let expression: moxy::ast::Expr = moxy::parse!(tokens).unwrap();
    assert!(expression.is_binary_chain());
}
