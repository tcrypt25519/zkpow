use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

/// Derive `bytemuck::{Pod, Zeroable}` and implement `zkpow_types::WireBytes`.
#[proc_macro_attribute]
pub fn wire_type(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new_spanned(
            proc_macro2::TokenStream::from(attr),
            "wire_type takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let ast = parse_macro_input!(item as DeriveInput);
    let name = &ast.ident;
    quote! {
        #[derive(::bytemuck::Pod, ::bytemuck::Zeroable)]
        #ast
        impl ::zkpow_types::WireBytes for #name {}
    }
    .into()
}
