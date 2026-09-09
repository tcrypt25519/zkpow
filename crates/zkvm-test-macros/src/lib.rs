use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{ItemFn, parse::Parse, parse_macro_input};

const TEST_CASE_STATIC_PREFIX: &str = "__ZKVM_TEST_CASE_";
const ATTR_EXPECTED: &str = "expected";
const ATTR_EXIT_CODE: &str = "exit_code";

/// An option accepted both inside `#[zkvm_test(...)]` and as a sibling attribute
/// on the test function. One table so the two parse paths cannot drift.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opt {
    Ignore,
    ShouldPanic,
    ExitCode,
    MaxCycles,
    Fixture,
    Snapshot,
}

impl Opt {
    const ALL: [Self; 6] = [
        Self::Ignore,
        Self::ShouldPanic,
        Self::ExitCode,
        Self::MaxCycles,
        Self::Fixture,
        Self::Snapshot,
    ];

    fn from_path(path: &syn::Path) -> Option<Self> {
        Self::ALL.into_iter().find(|option| path.is_ident(option.name()))
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Ignore => "ignore",
            Self::ShouldPanic => "should_panic",
            Self::ExitCode => ATTR_EXIT_CODE,
            Self::MaxCycles => "max_cycles",
            Self::Fixture => "fixture",
            Self::Snapshot => "snapshot",
        }
    }

    fn unsupported() -> String {
        let names = Self::ALL
            .into_iter()
            .map(|option| format!("`{}`", option.name()))
            .collect::<Vec<_>>()
            .join(", ");
        format!("unsupported zkvm_test option (expected one of: {names})")
    }
}

/// Read `name = <lit>` or `#[name(<lit>)]` as the literal type `L`.
fn lit_from_meta<L: Parse>(meta: &syn::Meta, name: &str, expected: &str) -> syn::Result<L> {
    match meta {
        syn::Meta::NameValue(nv) => syn::parse2::<L>(nv.value.to_token_stream())
            .map_err(|_| syn::Error::new_spanned(&nv.value, format!("expected {expected} for {name}"))),
        syn::Meta::List(list) => list.parse_args::<L>(),
        _ => Err(syn::Error::new_spanned(
            meta,
            format!("expected `{name} = {expected}` or `#[{name}({expected})]`"),
        )),
    }
}

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

impl TestConfig {
    /// An option written inside `#[zkvm_test(...)]`.
    fn parse_nested(&mut self, meta: &syn::meta::ParseNestedMeta) -> syn::Result<()> {
        let Some(opt) = Opt::from_path(&meta.path) else {
            return Err(meta.error(Opt::unsupported()));
        };
        match opt {
            Opt::Ignore => self.ignore = true,
            Opt::ShouldPanic => {
                self.should_panic = true;
                if meta.input.peek(syn::token::Paren) {
                    meta.parse_nested_meta(|nested| self.parse_should_panic_nested(&nested))?;
                }
            },
            Opt::ExitCode => {
                self.should_panic = true;
                self.expected_exit_code = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
            },
            Opt::MaxCycles => self.max_cycles = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?),
            Opt::Fixture => self.fixture = Some(meta.value()?.parse::<syn::LitStr>()?.value()),
            Opt::Snapshot => self.snapshot = Some(meta.value()?.parse::<syn::LitStr>()?.value()),
        }
        Ok(())
    }

    fn parse_should_panic_nested(&mut self, nested: &syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if nested.path.is_ident(ATTR_EXPECTED) {
            self.expected_message = Some(nested.value()?.parse::<syn::LitStr>()?.value());
            return Ok(());
        }
        if nested.path.is_ident(ATTR_EXIT_CODE) {
            self.expected_exit_code = Some(nested.value()?.parse::<syn::LitInt>()?.base10_parse()?);
            return Ok(());
        }
        Err(nested.error("unsupported option inside should_panic (expected `expected = \"...\"` or `exit_code = ...`)"))
    }

    /// Tries to consume a sibling attribute attached to the function.
    /// Returns `Ok(true)` if recognized and consumed, `Ok(false)` if unrelated,
    /// or `Err` if the syntax is invalid.
    fn consume_fn_attr(&mut self, attr: &syn::Attribute) -> syn::Result<bool> {
        let Some(opt) = Opt::from_path(attr.path()) else {
            return Ok(false);
        };
        let name = opt.name();
        match opt {
            Opt::Ignore => self.ignore = true,
            Opt::ShouldPanic => {
                self.should_panic = true;
                if let syn::Meta::List(list) = &attr.meta {
                    list.parse_nested_meta(|nested| self.parse_should_panic_nested(&nested))?;
                }
            },
            Opt::ExitCode => {
                self.should_panic = true;
                self.expected_exit_code =
                    Some(lit_from_meta::<syn::LitInt>(&attr.meta, name, "<int>")?.base10_parse()?);
            },
            Opt::MaxCycles => {
                self.max_cycles = Some(lit_from_meta::<syn::LitInt>(&attr.meta, name, "<int>")?.base10_parse()?);
            },
            Opt::Fixture => {
                self.fixture = Some(lit_from_meta::<syn::LitStr>(&attr.meta, name, "\"...\"")?.value());
            },
            Opt::Snapshot => {
                self.snapshot = Some(lit_from_meta::<syn::LitStr>(&attr.meta, name, "\"...\"")?.value());
            },
        }
        Ok(true)
    }
}

/// A test is registered as a `fn()` pointer, so anything that cannot coerce to one
/// has to be rejected here rather than as a type error inside the expansion.
fn validate_signature(sig: &syn::Signature) -> syn::Result<()> {
    let reject = |msg: &str| Err(syn::Error::new_spanned(sig, format!("zkvm_test: test functions {msg}")));

    if sig.asyncness.is_some() {
        return reject("cannot be `async`");
    }
    if sig.unsafety.is_some() {
        return reject("cannot be `unsafe`");
    }
    if !sig.generics.params.is_empty() {
        return reject("cannot be generic");
    }
    if !sig.inputs.is_empty() {
        return reject("cannot take arguments; read inputs with `zkvm_test::fixture()`");
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        let returns_unit = match ty.as_ref() {
            syn::Type::Tuple(t) => t.elems.is_empty(),
            syn::Type::Never(_) => true,
            _ => false,
        };
        if !returns_unit {
            return reject("must return `()`; assert inside the test instead of returning a value");
        }
    }
    Ok(())
}

/// `Option<T>` as the token stream for a `::core::option::Option` expression.
macro_rules! option_expr {
    ($value:expr) => {
        match $value {
            Some(v) => quote! { ::core::option::Option::Some(#v) },
            None => quote! { ::core::option::Option::None },
        }
    };
}

/// Attribute macro that registers a test function for both host (`cargo test`)
/// and SP1 zkVM (`zkvm-test`) execution.
///
/// Can be invoked as:
/// - `#[zkvm_test]` or `#[test]` (via `use zkvm_test::test;`)
/// - With inline options: `#[zkvm_test(ignore)]`, `#[zkvm_test(should_panic)]`, `#[zkvm_test(max_cycles = 100_000)]`,
///   `#[zkvm_test(fixture = "path.bin")]`, `#[zkvm_test(snapshot = "path.hex")]`
/// - With standard Rust attributes: ```rust,ignore #[test] #[ignore] #[should_panic(expected = "...")] #[max_cycles =
///   100_000] #[fixture = "path.bin"] #[snapshot = "path.hex"] fn my_test() { ... } ```
#[proc_macro_attribute]
pub fn zkvm_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut config = TestConfig::default();

    if !attr.is_empty() {
        let attr_parser = syn::meta::parser(|meta| config.parse_nested(&meta));
        parse_macro_input!(attr with attr_parser);
    }

    let mut input = parse_macro_input!(item as ItemFn);

    if let Err(err) = validate_signature(&input.sig) {
        return err.to_compile_error().into();
    }

    // Sibling attributes on the function (e.g. #[ignore], #[should_panic]) fold into
    // the same config; everything else stays on the emitted function.
    let mut remaining_attrs = Vec::new();
    for attr_item in input.attrs {
        match config.consume_fn_attr(&attr_item) {
            Ok(true) => {},
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

    let host_should_panic_attr = match (&config.expected_message, config.should_panic) {
        (Some(msg), _) => quote! { #[cfg_attr(not(target_os = "zkvm"), should_panic(expected = #msg))] },
        (None, true) => quote! { #[cfg_attr(not(target_os = "zkvm"), should_panic)] },
        (None, false) => quote! {},
    };

    let expected_message_expr = option_expr!(config.expected_message.as_ref());
    let exit_code_expr = option_expr!(config.expected_exit_code);
    let max_cycles_expr = option_expr!(config.max_cycles);
    let fixture_expr = option_expr!(config.fixture.as_ref());
    let snapshot_expr = option_expr!(config.snapshot.as_ref());

    // A fixture or a snapshot needs the host body wrapped, which forces the host and
    // zkVM copies apart. Without either, one function serves both.
    let fn_tokens = if config.fixture.is_some() || config.snapshot.is_some() {
        let (fn_vis, fn_sig, fn_block) = (&input.vis, &input.sig, &input.block);
        quote! {
            #[cfg(not(target_os = "zkvm"))]
            #[::core::prelude::v1::test]
            #host_ignore_attr
            #host_should_panic_attr
            #fn_vis #fn_sig {
                ::zkvm_test::__internal::with_host_context(#fixture_expr, #snapshot_expr, || #fn_block)
            }

            #[cfg(target_os = "zkvm")]
            #input
        }
    } else {
        quote! {
            #[cfg_attr(not(target_os = "zkvm"), ::core::prelude::v1::test)]
            #host_ignore_attr
            #host_should_panic_attr
            #input
        }
    };

    let ignore = config.ignore;
    let should_panic = config.should_panic;

    quote! {
        #fn_tokens

        #[cfg(target_os = "zkvm")]
        #[test_case]
        static #static_ident: ::zkvm_test::TestCase = ::zkvm_test::TestCase {
            name: concat!(module_path!(), "::", stringify!(#fn_name)),
            test_fn: #fn_name,
            ignore: #ignore,
            should_panic: #should_panic,
            expected_message: #expected_message_expr,
            expected_exit_code: #exit_code_expr,
            max_cycles: #max_cycles_expr,
            fixture: #fixture_expr,
            snapshot: #snapshot_expr,
        };
    }
    .into()
}

/// Convenience alias for `#[zkvm_test]`, allowing `use zkvm_test::test;`
/// to drop in place of standard Rust `#[test]`.
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    zkvm_test(attr, item)
}
