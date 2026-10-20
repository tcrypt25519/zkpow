use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

/// Make a struct ready for serialization/deserialization across the CPU and RISC-V boundary.
/// Uses bytemuck for zero-copy conversions; deriving the required traits and providing
/// standardized helper methods.
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
                bytemuck::bytes_of(self)
            }

            #[must_use]
            pub fn to_bytes(&self) -> [u8; Self::SIZE] {
                *bytemuck::bytes_of::<Self>(self)
                    .as_array()
                    .expect("bytes_of() should equal the type's ::SIZE")
            }

            /// Cast `bytes` to the inner type. Panics unless the length and alignment match exactly.
            #[must_use]
            pub fn parse(bytes: &[u8]) -> Self {
                *bytemuck::from_bytes::<Self>(bytes)
            }
        }
    }
    .into()
}
