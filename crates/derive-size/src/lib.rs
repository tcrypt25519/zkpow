use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, parse_macro_input};

#[proc_macro_derive(Size)]
pub fn derive_struct_sizes(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let fields = match &ast.data {
        Data::Struct(data_struct) => &data_struct.fields,
        _ => panic!("Size only supports structs"),
    };

    // Calculate SIZE_NO_PAD
    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
    let size_no_pad = quote! {
        0 #( + core::mem::size_of::<#field_types>() )*
    };

    // Calculate SIZE_NO_TAIL_PAD
    let offset_calculations = fields.iter().enumerate().map(|(i, f)| {
        let ty = &f.ty;
        let field_accessor = match &f.ident {
            Some(ident) => quote! { #ident },
            None => {
                let idx = syn::Index::from(i);
                quote! { #idx }
            },
        };

        quote! {
            let end = core::mem::offset_of!(#name, #field_accessor) + core::mem::size_of::<#ty>();
            if end > max {
                max = end;
            }
        }
    });

    let size_no_tail_pad = quote! {
        {
            let mut max = 0;
            #(#offset_calculations)*
            max
        }
    };

    let expanded = quote! {
        impl #name {
            pub const ALIGN: usize = core::mem::align_of::<#name>();
            pub const SIZE: usize = core::mem::size_of::<#name>();
            pub const SIZE_NO_PAD: usize = #size_no_pad;
            pub const SIZE_NO_TAIL_PAD: usize = #size_no_tail_pad;
        }
    };

    TokenStream::from(expanded)
}
