extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// Attribute macro that registers a test function for both host (`cargo test`)
/// and SP1 zkVM (`zkvm-test`) execution.
///
/// Options:
/// - `#[zkvm_test]`
/// - `#[zkvm_test(ignore)]`
/// - `#[zkvm_test(should_panic)]`
/// - `#[zkvm_test(should_panic(expected = "..."))]`
/// - `#[zkvm_test(should_panic, exit_code = 3)]`
/// - `#[zkvm_test(should_panic(expected = "..."), exit_code = 3)]`
/// - `#[zkvm_test(exit_code = 3)]`
#[proc_macro_attribute]
pub fn zkvm_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ignore = false;
    let mut should_panic = false;
    let mut expected_message: Option<String> = None;
    let mut expected_exit_code: Option<u32> = None;

    let attr_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("ignore") {
            ignore = true;
            Ok(())
        } else if meta.path.is_ident("should_panic") {
            should_panic = true;
            if meta.input.peek(syn::token::Paren) {
                meta.parse_nested_meta(|nested| {
                    if nested.path.is_ident("expected") {
                        let value: syn::LitStr = nested.value()?.parse()?;
                        expected_message = Some(value.value());
                        Ok(())
                    } else if nested.path.is_ident("exit_code") {
                        let value: syn::LitInt = nested.value()?.parse()?;
                        expected_exit_code = Some(value.base10_parse()?);
                        Ok(())
                    } else {
                        Err(nested.error("unsupported option inside should_panic (expected `expected = \"...\"` or `exit_code = ...`)"))
                    }
                })?;
            }
            Ok(())
        } else if meta.path.is_ident("exit_code") {
            should_panic = true;
            let value: syn::LitInt = meta.value()?.parse()?;
            expected_exit_code = Some(value.base10_parse()?);
            Ok(())
        } else {
            Err(meta.error("unsupported zkvm_test option (expected `ignore`, `should_panic`, or `exit_code`)"))
        }
    });

    parse_macro_input!(attr with attr_parser);
    let input = parse_macro_input!(item as ItemFn);

    let fn_name = &input.sig.ident;
    let static_ident = syn::Ident::new(&format!("__ZKVM_TEST_CASE_{}", fn_name), fn_name.span());

    let host_ignore_attr = if ignore {
        quote! { #[ignore] }
    } else {
        quote! {}
    };

    let host_should_panic_attr = if let Some(ref msg) = expected_message {
        quote! { #[should_panic(expected = #msg)] }
    } else if should_panic {
        quote! { #[should_panic] }
    } else {
        quote! {}
    };

    let zkvm_exit_code_expr = if let Some(code) = expected_exit_code {
        quote! { ::core::option::Option::Some(#code) }
    } else {
        quote! { ::core::option::Option::None }
    };

    let result = quote! {
        #[cfg_attr(not(target_os = "zkvm"), test)]
        #host_ignore_attr
        #host_should_panic_attr
        #input

        #[cfg(target_os = "zkvm")]
        #[test_case]
        static #static_ident: ::zkvm_test::TestCase = ::zkvm_test::TestCase {
            name: concat!(module_path!(), "::", stringify!(#fn_name)),
            test_fn: #fn_name,
            ignore: #ignore,
            should_panic: #should_panic,
            expected_exit_code: #zkvm_exit_code_expr,
        };
    };

    result.into()
}
