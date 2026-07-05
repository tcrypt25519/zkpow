extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Attribute macro that wraps a function body in a cycle-tracked span.
///
/// The generated label is `module_path/function_name`, which is derived from
/// the call site via `module_path!()` and the function's identifier. The
/// wrapper uses the `cycle-tracker-report-*` format so the span is accumulated
/// into `ExecutionReport.cycle_tracker` and `invocation_tracker` on the host.
#[proc_macro_attribute]
pub fn tracked(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let name = &sig.ident;

    let result = quote! {
        #(#attrs)*
        #vis #sig {
            ::zkpow_core::cycle_track!(
                concat!(module_path!(), "/", stringify!(#name)),
                #block
            )
        }
    };

    result.into()
}
