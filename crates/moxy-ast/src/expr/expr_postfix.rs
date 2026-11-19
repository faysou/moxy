use moxy_token::{Span, Spanner, ToTokenStream, ToTokens, TokenStream};

use crate::*;

/// An expression and the postfix operations on it, in order: calls, method
/// calls, field accesses, indexes, `?`, and `.await`, as in
/// `a.b()[0]?.c.await`.
///
/// Each operation applies to the base and the operations before it, so
/// `a.b().c()` calls `c` on the result of `a.b()`. Holding the chain in one
/// node keeps every traversal of a long chain, such as a builder chain, from
/// recursing once per operation.
#[derive(Clone)]
#[cfg_attr(feature = "derives", derive(Debug, PartialEq, Eq))]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExprPostfix {
    pub attrs: Attributes,
    pub base: Box<Expr>,
    /// One or more operations.
    pub ops: Vec<Postfix>,
}

impl Attributed for ExprPostfix {
    fn attrs(&self) -> &[Attribute] {
        &self.attrs
    }
}

impl From<ExprPostfix> for Expr {
    fn from(value: ExprPostfix) -> Self {
        Self::Postfix(value)
    }
}

impl Spanner for ExprPostfix {
    fn span(&self) -> Span {
        self.to_token_stream().span()
    }
}

impl ToTokens for ExprPostfix {
    fn to_tokens(&self, t: &mut TokenStream) {
        self.attrs.to_tokens(t);
        self.base.to_tokens(t);

        for op in &self.ops {
            op.to_tokens(t);
        }
    }
}

/// One postfix operation of an [`ExprPostfix`].
#[derive(Clone)]
#[cfg_attr(feature = "derives", derive(Debug, PartialEq, Eq))]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Postfix {
    Call(PostfixCall),
    MethodCall(PostfixMethodCall),
    Field(PostfixField),
    Index(PostfixIndex),
    Try(PostfixTry),
    Await(PostfixAwait),
}

impl Postfix {
    pub fn is_call(&self) -> bool {
        matches!(self, Self::Call(_))
    }

    pub fn is_method_call(&self) -> bool {
        matches!(self, Self::MethodCall(_))
    }

    pub fn is_field(&self) -> bool {
        matches!(self, Self::Field(_))
    }

    pub fn is_index(&self) -> bool {
        matches!(self, Self::Index(_))
    }

    pub fn is_try(&self) -> bool {
        matches!(self, Self::Try(_))
    }

    pub fn is_await(&self) -> bool {
        matches!(self, Self::Await(_))
    }

    pub fn as_call(&self) -> Option<&PostfixCall> {
        if let Self::Call(v) = self { Some(v) } else { None }
    }

    pub fn as_method_call(&self) -> Option<&PostfixMethodCall> {
        if let Self::MethodCall(v) = self { Some(v) } else { None }
    }

    pub fn as_field(&self) -> Option<&PostfixField> {
        if let Self::Field(v) = self { Some(v) } else { None }
    }

    pub fn as_index(&self) -> Option<&PostfixIndex> {
        if let Self::Index(v) = self { Some(v) } else { None }
    }

    pub fn as_try(&self) -> Option<&PostfixTry> {
        if let Self::Try(v) = self { Some(v) } else { None }
    }

    pub fn as_await(&self) -> Option<&PostfixAwait> {
        if let Self::Await(v) = self { Some(v) } else { None }
    }
}

impl Spanner for Postfix {
    fn span(&self) -> Span {
        self.to_token_stream().span()
    }
}

impl ToTokens for Postfix {
    fn to_tokens(&self, t: &mut TokenStream) {
        match self {
            Self::Call(v) => v.to_tokens(t),
            Self::MethodCall(v) => v.to_tokens(t),
            Self::Field(v) => v.to_tokens(t),
            Self::Index(v) => v.to_tokens(t),
            Self::Try(v) => v.to_tokens(t),
            Self::Await(v) => v.to_tokens(t),
        }
    }
}
