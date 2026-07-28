//! Shared test harness attributes for workspace tests.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, ReturnType, parse_macro_input, parse_quote};

/// Runs a synchronous or asynchronous test with `color_eyre` installed.
///
/// Synchronous functions use the standard test harness; asynchronous functions
/// use Tokio. Functions without an explicit return type are wrapped in
/// `color_eyre::eyre::Result<()>`, allowing their bodies to use `?` without an
/// `Ok(())` tail.
#[proc_macro_attribute]
pub fn test(args: TokenStream, input: TokenStream) -> TokenStream {
    parse_macro_input!(args as syn::parse::Nothing);
    let mut function = parse_macro_input!(input as ItemFn);

    function.attrs.push(if function.sig.asyncness.is_some() {
        parse_quote!(#[tokio::test])
    } else {
        parse_quote!(#[test])
    });

    // The hook is process-global, so another concurrently starting test may
    // install it first.
    let setup_position = function
        .block
        .stmts
        .iter()
        .take_while(|statement| matches!(statement, syn::Stmt::Item(_)))
        .count();
    function.block.stmts.insert(
        setup_position,
        parse_quote!(let _installed = ::color_eyre::install().is_ok();),
    );

    if matches!(function.sig.output, ReturnType::Default) {
        function.sig.output = parse_quote!(-> ::color_eyre::eyre::Result<()>);
        let ok = parse_quote!(::core::result::Result::Ok(()));
        function.block.stmts.push(syn::Stmt::Expr(ok, None));
    }

    quote!(#function).into()
}
