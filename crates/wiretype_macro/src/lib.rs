use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

/// Derive `bytemuck::{Pod, Zeroable}` and implement `zkpow_types::WireBytes`.
#[proc_macro_attribute]
pub fn wiretype(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new_spanned(proc_macro2::TokenStream::from(attr), "wiretype takes no arguments")
            .to_compile_error()
            .into();
    }
    let ast = parse_macro_input!(item as DeriveInput);
    let name = &ast.ident;
    quote! {
        #[derive(::bytemuck::Pod, ::bytemuck::Zeroable, Copy, Default)]
        #ast

        // impl ::wiretype::WireType for #name {}
        impl #name {
            pub const SIZE: usize = core::mem::size_of::<#name>();
            pub const ALIGN: usize = core::mem::align_of::<#name>();

            #[must_use]
            pub fn as_bytes(&self) -> &[u8] {
                bytemuck::bytes_of(self)
            }

            pub fn to_bytes(&self) -> [u8; Self::SIZE] {
                *bytemuck::bytes_of::<Self>(self)
                    .as_array()
                    .expect("bytes_of(State)'s len should equal State::SIZE")
            }


            #[must_use]
            pub fn parse(bytes: &[u8]) -> Result<&Self, ::bytemuck::PodCastError> {
                bytemuck::try_from_bytes::<Self>(bytes)
                /**/
            }
        }
    }
    .into()
}
