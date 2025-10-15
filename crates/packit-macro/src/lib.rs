extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Data, DataEnum, DataUnion, Error, Field, Fields, punctuated::Punctuated, spanned::Spanned,
    token::Comma,
};

#[proc_macro_derive(Pack)]
pub fn derive_pack(input: TokenStream) -> TokenStream {
    derive_pack_inner(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn derive_pack_inner(input: TokenStream) -> syn::Result<TokenStream2> {
    let input: syn::DeriveInput = syn::parse(input)?;
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let has_lifetime = input.generics.lifetimes().any(|l| l.lifetime.ident == "a");
    let pack = if has_lifetime {
        quote! { Pack<'a> }
    } else {
        quote! { Pack<'_> }
    };
    let unpack_bytes = if has_lifetime {
        quote! { bytes: &'a [u8] }
    } else {
        quote! { bytes: &[u8] }
    };

    let fields: Vec<_> = get_struct_fields(&input.data, "Pack")?.iter().collect();
    let field_idents = fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap())
        .collect::<Vec<_>>();
    let field_tys = fields.iter().map(|f| &f.ty);

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #pack for #ident #ty_generics #where_clause {
            #[allow(unused_assignments)]
            fn pack(&self, bytes: &mut [u8], align: NonZero<usize>) {
                let mut index = 0;
                #(self.#field_idents.pack({
                    let next_index = index + self.#field_idents.bytes(align);
                    let bytes = &mut bytes[index..next_index];
                    index = next_index;
                    bytes
                }, align);)*
            }
            #[allow(unused_assignments)]
            fn unpack(#unpack_bytes, align: NonZero<usize>) -> Self {
                let mut index = 0;
                Self {
                    #(#field_idents: {
                        let field = <#field_tys>::unpack(&bytes[index..], align);
                        index += field.bytes(align);
                        field
                    },)*
                }
            }
            fn bytes(&self, align: NonZero<usize>) -> usize {
                let mut sum = 0;
                #(sum += self.#field_idents.bytes(align);)*
                sum
            }
        }
    })
}

fn get_struct_fields<'a>(data: &'a Data, meta: &str) -> syn::Result<&'a Punctuated<Field, Comma>> {
    match data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields_named) => Ok(&fields_named.named),
            Fields::Unnamed(fields_unnamed) => Ok(&fields_unnamed.unnamed),
            Fields::Unit => Ok(const { &Punctuated::new() }),
        },
        Data::Enum(DataEnum { enum_token, .. }) => Err(Error::new(
            enum_token.span(),
            format!("#[{meta}] only supports structs, not enums"),
        )),
        Data::Union(DataUnion { union_token, .. }) => Err(Error::new(
            union_token.span(),
            format!("#[{meta}] only supports structs, not unions"),
        )),
    }
}
