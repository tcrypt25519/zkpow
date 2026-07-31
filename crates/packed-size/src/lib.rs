use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields};

#[proc_macro_derive(PackedSize)]
pub fn derive_packed_size(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let fields = match ast.data {
        Data::Struct(ref s) => &s.fields,
        _ => {
            return syn::Error::new_spanned(
                name,
                "PackedSize can only be derived for structs"
            )
            .to_compile_error()
            .into();
        }
    };

    let mut sizes = Vec::new();

    match fields {
        Fields::Named(named) => {
            for field in &named.named {
                let ty = &field.ty;
                sizes.push(quote! {
                    ::core::mem::size_of::<#ty>()
                });
            }
        }
        Fields::Unnamed(unnamed) => {
            for field in &unnamed.unnamed {
                let ty = &field.ty;
                sizes.push(quote! {
                    ::core::mem::size_of::<#ty>()
                });
            }
        }
        Fields::Unit => {}
    }

    // If any field is not Sized, size_of::<T>() will fail to compile.
    // We rely on Rust’s type system to produce the error.

    let expanded = quote! {
        impl #name {
            pub const PackedSize: usize = 0 #( + #sizes )*;
        }
    };

    expanded.into()
}
