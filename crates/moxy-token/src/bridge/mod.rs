/// Conversions for the compiler's `proc_macro` token types.
pub mod proc_macro;

/// Conversions for `proc_macro2` token types; requires the `proc-macro2` feature.
#[cfg(feature = "proc-macro2")]
pub mod proc_macro2;

// The stream an infallible conversion returns, which panics on a stream whose
// groups nest past `MAX_DEPTH`, as `try_into_token_stream` returns an error
fn expect_converted(converted: Result<crate::TokenStream, crate::LexError>) -> crate::TokenStream {
    converted.unwrap_or_else(|error| panic!("{error}; use `try_into_token_stream` to get this as an error"))
}
