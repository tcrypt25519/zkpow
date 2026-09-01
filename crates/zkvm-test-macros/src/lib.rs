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
/// - `#[zkvm_test(ignore, should_panic)]`
#[proc_macro_attribute]
pub fn zkvm_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ignore = false;
    let mut should_panic = false;

    let attr_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("ignore") {
            ignore = true;
            Ok(())
        } else if meta.path.is_ident("should_panic") {
            should_panic = true;
            Ok(())
        } else {
            Err(meta.error("unsupported zkvm_test option (expected `ignore` or `should_panic`)"))
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

    let host_should_panic_attr = if should_panic {
        quote! { #[should_panic] }
    } else {
        quote! {}
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
        };
    };

    result.into()
}
