use std::cell::Cell;

use moxy_token::{Delim, DelimSpan, Ident, Span, ToTokens, TokenStream, TokenTree};

use crate::{Cursor, Parse, ParseError};

/// An AST representation of Rust parser syntax.
#[derive(Clone)]
pub struct Parser<'a> {
    cursor: Cell<Cursor<'a>>,
    // The delimiter that closes the group the parser reads, which an error at
    // the end of its tokens names
    close: Option<Delim>,
}

impl<'a> Parser<'a> {
    pub fn from_tokens(tokens: &'a TokenStream) -> Self {
        Self {
            cursor: Cell::new(Cursor::from_tokens(tokens)),
            close: None,
        }
    }

    pub fn from_cursor(cursor: Cursor<'a>) -> Self {
        Self {
            cursor: Cell::new(cursor),
            close: None,
        }
    }

    pub fn cursor(&self) -> Cursor<'a> {
        self.cursor.get()
    }

    pub fn is_empty(&self) -> bool {
        self.cursor.get().is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.cursor.get().remaining()
    }

    pub fn span(&self) -> Span {
        self.cursor.get().span()
    }

    pub fn error(&self, message: impl std::fmt::Display) -> ParseError {
        ParseError::new(self.span(), message)
    }

    /// An error at the current token, which is not the `expected` input, that
    /// names the token or the end of the input, as "expected `;`, found `}`".
    pub fn expected(&self, expected: impl std::fmt::Display) -> ParseError {
        self.error(format!("expected {expected}, found {}", self.found()))
    }

    /// An error at the current token, which no input expects, as "unexpected
    /// `@`".
    pub fn unexpected(&self) -> ParseError {
        self.error(format!("unexpected {}", self.found()))
    }

    // The current token as a message quotes it: at the end of a group's
    // tokens, the delimiter that closes it, or else the end of the input
    fn found(&self) -> String {
        match (self.curr(), self.close) {
            (Some(token), _) => quoted(&received(token)),
            (None, Some(close)) => quoted(&close.close().to_string()),
            (None, None) => "end of input".to_owned(),
        }
    }

    // A parser over the tokens of `group`, which knows the delimiter that
    // closes them
    fn in_group(group: &'a moxy_token::Group) -> Self {
        Self {
            cursor: Cell::new(Cursor::from_tokens(&group.tokens)),
            close: (group.delim != Delim::None).then_some(group.delim),
        }
    }

    pub fn fork(&self) -> Self {
        Self {
            cursor: self.cursor.clone(),
            close: self.close,
        }
    }

    pub fn seek(&self, other: &Self) {
        self.cursor.set(other.cursor.get());
    }

    pub fn curr(&self) -> Option<&'a TokenTree> {
        self.cursor.get().curr()
    }

    pub fn next(&self) -> Option<&'a TokenTree> {
        self.cursor.get().next()
    }

    pub fn nth(&self, n: usize) -> Option<&'a TokenTree> {
        self.cursor.get().nth(n)
    }

    pub fn prev(&self) -> Option<&'a TokenTree> {
        self.cursor.get().prev()
    }

    pub fn offset(&self, n: usize) -> &Self {
        let next = self.cursor.get().offset(n);
        self.cursor.set(next);
        self
    }

    pub fn is_delimited(&self, delim: Delim) -> bool {
        self.cursor.get().is_delimited(delim)
    }

    pub fn descend<P: FnOnce(&Parser) -> bool + 'static>(&self, delim: Delim, p: P) -> Result<bool, ParseError> {
        let parser = self.parse_group(delim)?;
        Ok(p(&parser))
    }
}

impl<'a> Parser<'a> {
    pub fn advance(&self) -> Option<&'a TokenTree> {
        let (cursor, token) = self.cursor.get().advance();
        self.cursor.set(cursor);
        token
    }

    pub fn advance_by(&self, n: usize) -> Option<&'a [TokenTree]> {
        let original = self.cursor.get();
        let (cursor, tokens) = original.advance_by(n);

        if tokens.is_some() {
            self.cursor.set(cursor);
        }

        tokens
    }

    pub fn skip_until<F: Fn(Option<&TokenTree>) -> bool>(&self, pred: F) -> &Self {
        self.cursor.set(self.cursor.get().skip_until(pred));
        self
    }
}

impl Parser<'_> {
    pub fn parse_while<T: Parse>(&self) -> Vec<T> {
        let mut items = Vec::new();

        while T::peek(self.cursor()) {
            let fork = self.fork();

            match T::parse(&fork) {
                Ok(item) => {
                    self.seek(&fork);
                    items.push(item);
                }
                Err(_) => break,
            }
        }

        items
    }

    /// Returns an error at the first remaining token unless the input is fully
    /// consumed, so a list that stops early inside a delimited group fails
    /// instead of dropping the rest of the group.
    pub fn ensure_empty(&self) -> Result<(), ParseError> {
        if self.is_empty() { Ok(()) } else { Err(self.unexpected()) }
    }

    pub fn parse_until_empty<T: Parse>(&self) -> Result<Vec<T>, ParseError> {
        let mut items = Vec::new();

        while !self.is_empty() {
            items.push(T::parse(self)?);
        }

        Ok(items)
    }
}

impl<'a> Parser<'a> {
    pub fn parse_group(&self, delim: Delim) -> Result<Parser<'a>, ParseError> {
        match self.cursor().curr() {
            Some(TokenTree::Group(group)) if group.delim == delim => {
                self.advance();
                Ok(Self::in_group(group))
            }
            _ => Err(self.expected(format_args!("`{}`", delim.open()))),
        }
    }

    pub fn parse_group_spanned(&self, delim: Delim) -> Result<(DelimSpan, Parser<'a>), ParseError> {
        match self.cursor().curr() {
            Some(TokenTree::Group(group)) if group.delim == delim => {
                self.advance();
                Ok((group.span, Self::in_group(group)))
            }
            _ => Err(self.expected(format_args!("`{}`", delim.open()))),
        }
    }

    pub fn parse_ident_any(&self) -> Result<Ident, ParseError> {
        let ident = match self.curr() {
            Some(TokenTree::Ident(ident)) => ident.clone(),
            Some(TokenTree::Keyword(keyword)) => Ident::new(keyword.as_str()).with_span(keyword.span()),
            _ => return Err(self.expected("identifier")),
        };

        self.advance();
        Ok(ident)
    }
}

// A token as an error message quotes it: a group as its delimiters alone, as
// `{ ... }`, so a message does not print a whole macro body
pub(crate) fn received(token: &TokenTree) -> String {
    match token {
        // A group without delimiters, which only a stream from the compiler
        // holds, by its first token
        TokenTree::Group(group) if group.delim == Delim::None => match group.tokens.first() {
            Some(first) if group.tokens.len() > 1 => format!("{} ...", received(first)),
            Some(first) => received(first),
            None => String::new(),
        },
        TokenTree::Group(group) if group.tokens.is_empty() => format!("{}{}", group.delim.open(), group.delim.close()),
        TokenTree::Group(group) => format!("{} ... {}", group.delim.open(), group.delim.close()),
        other => other.to_string(),
    }
}

// Code in a message in backticks, or in double ones with spaces when it holds a
// backtick, as a string literal can
pub(crate) fn quoted(code: &str) -> String {
    if code.contains('`') {
        format!("`` {code} ``")
    } else {
        format!("`{code}`")
    }
}

impl<'a> ToTokens for Parser<'a> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.cursor.get().to_tokens(tokens);
    }
}
