use super::*;

pub(crate) fn single(parser: &Parser) -> Result<Pattern, ParseError> {
    if PatWild::peek(parser.cursor()) {
        return Ok(Pattern::Wild(<_ as Parse>::parse(parser)?));
    }

    if PatRange::peek(parser.cursor()) {
        return Ok(Pattern::Range(<_ as Parse>::parse(parser)?));
    }

    if PatRest::peek(parser.cursor()) {
        return Ok(Pattern::Rest(<_ as Parse>::parse(parser)?));
    }

    if PatBox::peek(parser.cursor()) {
        return Ok(Pattern::Box(<_ as Parse>::parse(parser)?));
    }

    if PatConst::peek(parser.cursor()) {
        return Ok(Pattern::Const(<_ as Parse>::parse(parser)?));
    }

    if PatReference::peek(parser.cursor()) {
        return Ok(Pattern::Reference(<_ as Parse>::parse(parser)?));
    }

    if PatGroup::peek(parser.cursor()) {
        return Ok(Pattern::Group(<_ as Parse>::parse(parser)?));
    }

    if PatSlice::peek(parser.cursor()) {
        return Ok(Pattern::Slice(<_ as Parse>::parse(parser)?));
    }

    if Attributes::skip(parser.cursor()).is_some_and(|cursor| cursor.is_delimited(moxy_token::Delim::Paren)) {
        return paren_or_tuple(parser);
    }

    if PatMacro::peek(parser.cursor()) {
        return Ok(Pattern::Macro(<_ as Parse>::parse(parser)?));
    }

    if PatTupleStruct::peek(parser.cursor()) {
        return Ok(Pattern::TupleStruct(<_ as Parse>::parse(parser)?));
    }

    if PatStruct::peek(parser.cursor()) {
        return Ok(Pattern::Struct(<_ as Parse>::parse(parser)?));
    }

    if PatIdent::peek(parser.cursor()) {
        return Ok(Pattern::Ident(<_ as Parse>::parse(parser)?));
    }

    if PatPath::peek(parser.cursor()) {
        return Ok(Pattern::Path(<_ as Parse>::parse(parser)?));
    }

    if PatLit::peek(parser.cursor()) {
        return Ok(Pattern::Lit(<_ as Parse>::parse(parser)?));
    }

    parser.error("expected pattern").into()
}

// Parses the parenthesized patterns once and then tells a tuple from a
// parenthesized pattern, so nested parentheses parse in linear time. One
// pattern without a trailing comma, other than `..`, is parenthesized.
fn paren_or_tuple(parser: &Parser) -> Result<Pattern, ParseError> {
    let attrs = <_ as Parse>::parse(parser)?;
    let (span, inner) = parser.parse_group_spanned(moxy_token::Delim::Paren)?;
    let mut elems: List<Pattern, Token![,]> = List::parse_all(&inner)?;

    if elems.len() == 1 && !elems.is_trailing() && !elems.first().is_some_and(Pattern::is_rest) {
        let content = elems.pop().map(|pair| pair.into_value()).expect("one pattern");

        return Ok(Pattern::Paren(PatParen {
            attrs,
            content: Delimited::paren(span, Box::new(content)),
        }));
    }

    Ok(Pattern::Tuple(PatTuple {
        attrs,
        elems: Delimited::paren(span, elems),
    }))
}
