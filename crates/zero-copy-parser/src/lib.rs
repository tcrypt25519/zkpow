// use proc_macro::TokenStream;
// use quote::{format_ident, quote};
// use syn::{parse_macro_input, Data, DeriveInput, Fields};

// #[proc_macro_derive(ZcParse)]
// pub fn zc_parse_derive(input: TokenStream) -> TokenStream {
//     let input = parse_macro_input!(input as DeriveInput);
//     let struct_name = &input.ident;
//     let vis = &input.vis;

//     // Generate the wrapper name: e.g., ParsedFoo
//     let wrapper_name = format_ident!("Parsed{}", struct_name);

//     let Data::Struct(data_struct) = &input.data else {
//         panic!("ZcParse only supports structs");
//     };

//     let Fields::Named(fields) = &data_struct.fields else {
//         panic!("ZcParse only supports structs with named fields");
//     };

//     let mut getters_setters = Vec::new();
//     let mut current_offset = quote! { 0 };

//     for field in &fields.named {
//         let field_name = field.ident.as_ref().unwrap();
//         let field_type = &field.ty;
//         let field_vis = &field.vis;

//         let set_field_name = format_ident!("set_{}", field_name);

//         // Chain the offsets so the compiler calculates them at compile-time
//         let next_offset = quote! { #current_offset + ::core::mem::size_of::<#field_type>() };

//         getters_setters.push(quote! {
//             #field_vis fn #field_name<'a>(&'a self) -> &'a #field_type {
//                 let start = #current_offset;
//                 let end = #next_offset;
//                 ::bytemuck::pod_read_unaligned(&self.bytes[start..end])
//             }

//             #field_vis fn #set_field_name(&mut self, v: #field_type) {
//                 let start = #current_offset;
//                 let end = #next_offset;
//                 self.bytes[start..end].copy_from_slice(::bytemuck::bytes_of(&v));
//             }
//         });

//         current_offset = next_offset;
//     }

//     let total_size = current_offset;

//     let expanded = quote! {
//         #vis struct #wrapper_name<'bytes> {
//             bytes: &'bytes mut [u8],
//         }

//         impl<'bytes> #wrapper_name<'bytes> {
//             /// Constructs the parsed wrapper from a mutable byte slice.
//             /// Panics if the slice is too small to contain the struct.
//             #vis fn from_bytes(bytes: &'bytes mut [u8]) -> Self {
//                 let required_len = #total_size;
//                 assert!(
//                     bytes.len() >= required_len,
//                     "Slice is too small to parse {}",
//                     stringify!(#struct_name)
//                 );

//                 // Truncate the slice to the exact size of the packed struct
//                 Self {
//                     bytes: &mut bytes[..required_len]
//                 }
//             }

//             #(#getters_setters)*

//             /// Copies the underlying bytes into a new, owned Vec<u8>
//             #vis fn copied_bytes(&self) -> ::std::vec::Vec<u8> {
//                 self.bytes.to_vec()
//             }
//         }
//     };

//     TokenStream::from(expanded)
// }
