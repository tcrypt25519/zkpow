use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

#[derive(Default)]
struct TestConfig {
    ignore:             bool,
    should_panic:       bool,
    expected_message:   Option<String>,
    expected_exit_code: Option<u32>,
    max_cycles:         Option<u64>,
    fixture:            Option<String>,
    snapshot:           Option<String>,
}

const TEST_CASE_STATIC_PREFIX: &str = "__ZKVM_TEST_CASE_";
const ATTR_IGNORE: &str = "ignore";
const ATTR_SHOULD_PANIC: &str = "should_panic";
const ATTR_EXIT_CODE: &str = "exit_code";
const ATTR_MAX_CYCLES: &str = "max_cycles";
const ATTR_FIXTURE: &str = "fixture";
const ATTR_SNAPSHOT: &str = "snapshot";
const ATTR_EXPECTED: &str = "expected";

fn parse_int_from_meta<T: std::str::FromStr>(meta: &syn::Meta, name: &str) -> syn::Result<T>
where
    T::Err: std::fmt::Display,
{
    match meta {
        syn::Meta::NameValue(nv) => {
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(lit_int),
                ..
            }) = &nv.value
            else {
                return Err(syn::Error::new_spanned(&nv.value, format!("expected integer for {name}")));
            };
            lit_int.base10_parse()
        },
        syn::Meta::List(list) => {
            let lit_int: syn::LitInt = list.parse_args()?;
            lit_int.base10_parse()
        },
        _ => Err(syn::Error::new_spanned(
            meta,
            format!("expected `{name} = <int>` or `#[{name}(<int>)]`"),
        )),
    }
}

fn parse_str_from_meta(meta: &syn::Meta, name: &str) -> syn::Result<String> {
    match meta {
        syn::Meta::NameValue(nv) => {
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(lit_str),
                ..
            }) = &nv.value
            else {
                return Err(syn::Error::new_spanned(&nv.value, format!("expected string literal for {name}")));
            };
            Ok(lit_str.value())
        },
        syn::Meta::List(list) => {
            let lit_str: syn::LitStr = list.parse_args()?;
            Ok(lit_str.value())
        },
        _ => Err(syn::Error::new_spanned(
            meta,
            format!("expected `{name} = \"...\"` or `#[{name}(\"...\")]`"),
        )),
    }
}

impl TestConfig {
    fn parse_nested(&mut self, meta: &syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if meta.path.is_ident(ATTR_IGNORE) {
            self.ignore = true;
            return Ok(());
        }
        if meta.path.is_ident(ATTR_SHOULD_PANIC) {
            self.should_panic = true;
            if meta.input.peek(syn::token::Paren) {
                meta.parse_nested_meta(|nested| self.parse_should_panic_nested(&nested))?;
            }
            return Ok(());
        }
        if meta.path.is_ident(ATTR_EXIT_CODE) {
            self.should_panic = true;
            let value: syn::LitInt = meta.value()?.parse()?;
            self.expected_exit_code = Some(value.base10_parse()?);
            return Ok(());
        }
        if meta.path.is_ident(ATTR_MAX_CYCLES) {
            let value: syn::LitInt = meta.value()?.parse()?;
            self.max_cycles = Some(value.base10_parse()?);
            return Ok(());
        }
        if meta.path.is_ident(ATTR_FIXTURE) {
            let value: syn::LitStr = meta.value()?.parse()?;
            self.fixture = Some(value.value());
            return Ok(());
        }
        if meta.path.is_ident(ATTR_SNAPSHOT) {
            let value: syn::LitStr = meta.value()?.parse()?;
            self.snapshot = Some(value.value());
            return Ok(());
        }

        Err(meta.error(
            "unsupported zkvm_test option (expected `ignore`, `should_panic`, `exit_code`, `max_cycles`, `fixture`, or `snapshot`)",
        ))
    }

    fn parse_should_panic_nested(&mut self, nested: &syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if nested.path.is_ident(ATTR_EXPECTED) {
            let value: syn::LitStr = nested.value()?.parse()?;
            self.expected_message = Some(value.value());
            return Ok(());
        }
        if nested.path.is_ident(ATTR_EXIT_CODE) {
            let value: syn::LitInt = nested.value()?.parse()?;
            self.expected_exit_code = Some(value.base10_parse()?);
            return Ok(());
        }

        Err(nested.error(
            "unsupported option inside should_panic (expected `expected = \"...\"` or `exit_code = ...`)",
        ))
    }

    /// Tries to consume a sibling attribute attached to the function.
    /// Returns `Ok(true)` if recognized and consumed, `Ok(false)` if unrelated,
    /// or `Err` if the syntax is invalid.
    fn consume_fn_attr(&mut self, attr: &syn::Attribute) -> syn::Result<bool> {
        if attr.path().is_ident(ATTR_IGNORE) {
            self.ignore = true;
            return Ok(true);
        }

        if attr.path().is_ident(ATTR_SHOULD_PANIC) {
            self.should_panic = true;
            if let syn::Meta::List(list) = &attr.meta {
                list.parse_nested_meta(|nested| self.parse_should_panic_nested(&nested))?;
            }
            return Ok(true);
        }

        if attr.path().is_ident(ATTR_EXIT_CODE) {
            self.should_panic = true;
            self.expected_exit_code = Some(parse_int_from_meta(&attr.meta, ATTR_EXIT_CODE)?);
            return Ok(true);
        }

        if attr.path().is_ident(ATTR_MAX_CYCLES) {
            self.max_cycles = Some(parse_int_from_meta(&attr.meta, ATTR_MAX_CYCLES)?);
            return Ok(true);
        }

        if attr.path().is_ident(ATTR_FIXTURE) {
            self.fixture = Some(parse_str_from_meta(&attr.meta, ATTR_FIXTURE)?);
            return Ok(true);
        }

        if attr.path().is_ident(ATTR_SNAPSHOT) {
            self.snapshot = Some(parse_str_from_meta(&attr.meta, ATTR_SNAPSHOT)?);
            return Ok(true);
        }

        Ok(false)
    }
}

/// Attribute macro that registers a test function for both host (`cargo test`)
/// and SP1 zkVM (`zkvm-test`) execution.
///
/// Can be invoked as:
/// - `#[zkvm_test]` or `#[test]` (via `use zkvm_test::test;`)
/// - With inline options: `#[zkvm_test(ignore)]`, `#[zkvm_test(should_panic)]`, `#[zkvm_test(max_cycles = 100_000)]`, `#[zkvm_test(fixture = "path.bin")]`, `#[zkvm_test(snapshot = "path.hex")]`
/// - With standard Rust attributes:
///   ```rust,ignore
///   #[test]
///   #[ignore]
///   #[should_panic(expected = "...")]
///   #[max_cycles = 100_000]
///   #[fixture = "path.bin"]
///   #[snapshot = "path.hex"]
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
    let static_ident = syn::Ident::new(&format!("{TEST_CASE_STATIC_PREFIX}{fn_name}"), fn_name.span());

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

    let zkvm_max_cycles_expr = if let Some(max) = config.max_cycles {
        quote! { ::core::option::Option::Some(#max) }
    } else {
        quote! { ::core::option::Option::None }
    };

    let zkvm_fixture_expr = if let Some(ref fix) = config.fixture {
        quote! { ::core::option::Option::Some(#fix) }
    } else {
        quote! { ::core::option::Option::None }
    };

    let zkvm_snapshot_expr = if let Some(ref snap) = config.snapshot {
        quote! { ::core::option::Option::Some(#snap) }
    } else {
        quote! { ::core::option::Option::None }
    };

    let ignore = config.ignore;
    let should_panic = config.should_panic;

    let (host_fn_tokens, zkvm_fn_tokens) = match (&config.fixture, &config.snapshot) {
        (Some(fix), Some(snap)) => {
            let fn_vis = &input.vis;
            let fn_sig = &input.sig;
            let fn_block = &input.block;
            let host = quote! {
                #[cfg(not(target_os = "zkvm"))]
                #[::core::prelude::v1::test]
                #host_ignore_attr
                #host_should_panic_attr
                #fn_vis #fn_sig {
                    ::zkvm_test::__internal::with_host_fixture_and_snapshot(#fix, #snap, || #fn_block)
                }
            };
            let zkvm = quote! {
                #[cfg(target_os = "zkvm")]
                #input
            };
            (host, zkvm)
        },
        (Some(fix), None) => {
            let fn_vis = &input.vis;
            let fn_sig = &input.sig;
            let fn_block = &input.block;
            let host = quote! {
                #[cfg(not(target_os = "zkvm"))]
                #[::core::prelude::v1::test]
                #host_ignore_attr
                #host_should_panic_attr
                #fn_vis #fn_sig {
                    ::zkvm_test::__internal::with_host_fixture(#fix, || #fn_block)
                }
            };
            let zkvm = quote! {
                #[cfg(target_os = "zkvm")]
                #input
            };
            (host, zkvm)
        },
        (None, Some(snap)) => {
            let fn_vis = &input.vis;
            let fn_sig = &input.sig;
            let fn_block = &input.block;
            let host = quote! {
                #[cfg(not(target_os = "zkvm"))]
                #[::core::prelude::v1::test]
                #host_ignore_attr
                #host_should_panic_attr
                #fn_vis #fn_sig {
                    ::zkvm_test::__internal::with_host_snapshot(#snap, || #fn_block)
                }
            };
            let zkvm = quote! {
                #[cfg(target_os = "zkvm")]
                #input
            };
            (host, zkvm)
        },
        (None, None) => {
            let all = quote! {
                #[cfg_attr(not(target_os = "zkvm"), ::core::prelude::v1::test)]
                #host_ignore_attr
                #host_should_panic_attr
                #input
            };
            (all, quote! {})
        },
    };

    let result = quote! {
        #host_fn_tokens
        #zkvm_fn_tokens

        #[cfg(target_os = "zkvm")]
        #[test_case]
        static #static_ident: ::zkvm_test::TestCase = ::zkvm_test::TestCase {
            name: concat!(module_path!(), "::", stringify!(#fn_name)),
            test_fn: #fn_name,
            ignore: #ignore,
            should_panic: #should_panic,
            expected_exit_code: #zkvm_exit_code_expr,
            max_cycles: #zkvm_max_cycles_expr,
            fixture: #zkvm_fixture_expr,
            snapshot: #zkvm_snapshot_expr,
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
