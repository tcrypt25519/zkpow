use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

#[derive(Default)]
struct TestConfig {
    ignore:             bool,
    should_panic:       bool,
    expected_message:   Option<String>,
    expected_exit_code: Option<u32>,
}

impl TestConfig {
    fn parse_nested(&mut self, meta: &syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if meta.path.is_ident("ignore") {
            self.ignore = true;
            Ok(())
        } else if meta.path.is_ident("should_panic") {
            self.should_panic = true;
            if meta.input.peek(syn::token::Paren) {
                meta.parse_nested_meta(|nested| self.parse_should_panic_nested(&nested))?;
            }
            Ok(())
        } else if meta.path.is_ident("exit_code") {
            self.should_panic = true;
            let value: syn::LitInt = meta.value()?.parse()?;
            self.expected_exit_code = Some(value.base10_parse()?);
            Ok(())
        } else {
            Err(meta.error("unsupported zkvm_test option (expected `ignore`, `should_panic`, or `exit_code`)"))
        }
    }

    fn parse_should_panic_nested(&mut self, nested: &syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if nested.path.is_ident("expected") {
            let value: syn::LitStr = nested.value()?.parse()?;
            self.expected_message = Some(value.value());
            Ok(())
        } else if nested.path.is_ident("exit_code") {
            let value: syn::LitInt = nested.value()?.parse()?;
            self.expected_exit_code = Some(value.base10_parse()?);
            Ok(())
        } else {
            Err(nested.error("unsupported option inside should_panic (expected `expected = \"...\"` or `exit_code = ...`)"))
        }
    }

    /// Tries to consume a sibling attribute attached to the function.
    /// Returns `Ok(true)` if recognized and consumed, `Ok(false)` if unrelated,
    /// or `Err` if the syntax is invalid.
    fn consume_fn_attr(&mut self, attr: &syn::Attribute) -> syn::Result<bool> {
        if attr.path().is_ident("ignore") {
            self.ignore = true;
            Ok(true)
        } else if attr.path().is_ident("should_panic") {
            self.should_panic = true;
            match &attr.meta {
                syn::Meta::Path(_) => Ok(true),
                syn::Meta::List(list) => {
                    list.parse_nested_meta(|nested| self.parse_should_panic_nested(&nested))?;
                    Ok(true)
                },
                _ => Ok(true),
            }
        } else if attr.path().is_ident("exit_code") {
            self.should_panic = true;
            match &attr.meta {
                syn::Meta::NameValue(nv) => {
                    if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(lit_int), .. }) = &nv.value {
                        self.expected_exit_code = Some(lit_int.base10_parse()?);
                        Ok(true)
                    } else {
                        Err(syn::Error::new_spanned(&nv.value, "expected integer for exit_code"))
                    }
                },
                syn::Meta::List(list) => {
                    let lit_int: syn::LitInt = list.parse_args()?;
                    self.expected_exit_code = Some(lit_int.base10_parse()?);
                    Ok(true)
                },
                _ => Ok(true),
            }
        } else {
            Ok(false)
        }
    }
}

/// Attribute macro that registers a test function for both host (`cargo test`)
/// and SP1 zkVM (`zkvm-test`) execution.
///
/// Can be invoked as:
/// - `#[zkvm_test]` or `#[test]` (via `use zkvm_test::test;`)
/// - With inline options: `#[zkvm_test(ignore)]`, `#[zkvm_test(should_panic)]`, `#[zkvm_test(exit_code = 3)]`
/// - With standard Rust attributes:
///   ```rust,ignore
///   #[test]
///   #[ignore]
///   #[should_panic(expected = "...")]
///   fn my_test() { ... }
///   ```
#[proc_macro_attribute]
pub fn zkvm_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut config = TestConfig::default();

    if !attr.is_empty() {
        let attr_parser = syn::meta::parser(|meta| config.parse_nested(&meta));
        parse_macro_input!(attr with attr_parser);
    }

    let mut input = parse_macro_input!(item as ItemFn);

    // Filter and consume sibling attributes on the function (e.g. #[ignore], #[should_panic])
    let mut remaining_attrs = Vec::new();
    for attr_item in input.attrs {
        match config.consume_fn_attr(&attr_item) {
            Ok(true) => {}, // consumed and recorded in config
            Ok(false) => remaining_attrs.push(attr_item),
            Err(err) => return err.to_compile_error().into(),
        }
    }
    input.attrs = remaining_attrs;

    let fn_name = &input.sig.ident;
    let static_ident = syn::Ident::new(&format!("__ZKVM_TEST_CASE_{}", fn_name), fn_name.span());

    let host_ignore_attr = if config.ignore {
        quote! { #[cfg_attr(not(target_os = "zkvm"), ignore)] }
    } else {
        quote! {}
    };

    let host_should_panic_attr = if let Some(ref msg) = config.expected_message {
        quote! { #[cfg_attr(not(target_os = "zkvm"), should_panic(expected = #msg))] }
    } else if config.should_panic {
        quote! { #[cfg_attr(not(target_os = "zkvm"), should_panic)] }
    } else {
        quote! {}
    };

    let zkvm_exit_code_expr = if let Some(code) = config.expected_exit_code {
        quote! { ::core::option::Option::Some(#code) }
    } else {
        quote! { ::core::option::Option::None }
    };

    let ignore = config.ignore;
    let should_panic = config.should_panic;

    let result = quote! {
        #[cfg_attr(not(target_os = "zkvm"), ::core::prelude::v1::test)]
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

/// Convenience alias for `#[zkvm_test]`, allowing `use zkvm_test::test;`
/// to drop in place of standard Rust `#[test]`.
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    zkvm_test(attr, item)
}
