use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

/// Make a struct serializable across the CPU and RISC-V boundary. Derives the bytemuck
/// traits needed for zero-copy conversion and adds `SIZE`, `as_bytes`, `to_bytes`, and
/// `parse` helpers.
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
        #[derive(::bytemuck::Pod, ::bytemuck::Zeroable, Clone, Copy, Debug, Default, Eq, PartialEq)]
        #[repr(C)]
        #ast

        impl #name {
            pub const SIZE: usize = core::mem::size_of::<#name>();
            pub const ALIGN: usize = core::mem::align_of::<#name>();

            #[must_use]
            pub fn as_bytes(&self) -> &[u8] {
                <Self as ::wiretype::WireType>::as_bytes(self)
            }

            #[must_use]
            pub fn to_bytes(&self) -> [u8; Self::SIZE] {
                <Self as ::wiretype::WireType>::to_bytes(self)
            }

            /// Borrow a value from bytes with exactly the required length and alignment.
            pub fn parse(bytes: &[u8]) -> Result<&Self, ::bytemuck::PodCastError> {
                <Self as ::wiretype::WireType>::parse(bytes)
            }
        }

        impl ::wiretype::WireType for #name {
            type Bytes = [u8; core::mem::size_of::<#name>()];
            type Inner = Self;

            #[must_use]
            fn to_bytes(&self) -> Self::Bytes {
                ::bytemuck::bytes_of(self)
                    .try_into()
                    .expect("bytes_of() should equal the type's ::SIZE")
            }

            fn as_bytes(&self) -> &[u8] {
                ::bytemuck::bytes_of(self)
            }

            #[must_use]
            fn parse(bytes: &[u8]) -> Result<&Self, ::bytemuck::PodCastError> {
                ::bytemuck::try_from_bytes(bytes)
            }
        }
    }
    .into()
}
