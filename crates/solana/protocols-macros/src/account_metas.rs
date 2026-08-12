//! AccountMetas derive macro implementation.

use darling::{ast::Data, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Type};

#[derive(Debug, FromDeriveInput)]
#[darling(supports(struct_named))]
struct AccountMetasArgs {
    ident: syn::Ident,
    generics: syn::Generics,
    data: Data<(), AccountField>,
}

#[derive(Debug, FromField)]
#[darling(attributes(account))]
struct AccountField {
    ident: Option<syn::Ident>,
    /// Field type - must be Pubkey but stored for potential future validation.
    #[allow(dead_code)]
    ty: Type,
    /// Is the account writable?
    #[darling(default)]
    writable: bool,
    /// Is the account a signer?
    #[darling(default)]
    signer: bool,
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match AccountMetasArgs::from_derive_input(&input) {
        Ok(args) => args,
        Err(e) => return e.write_errors().into(),
    };

    let expanded = generate_impl(&args);
    expanded.into()
}

fn generate_impl(args: &AccountMetasArgs) -> TokenStream2 {
    let name = &args.ident;
    let (impl_generics, ty_generics, where_clause) = args.generics.split_for_impl();

    let fields = match &args.data {
        Data::Struct(fields) => fields,
        _ => {
            return syn::Error::new_spanned(name, "AccountMetas only supports structs")
                .to_compile_error();
        }
    };

    let field_count = fields.len();
    let mut account_metas = Vec::new();
    let mut from_pubkeys_assignments = Vec::new();

    for (idx, field) in fields.iter().enumerate() {
        let field_name = field.ident.as_ref().unwrap();
        let is_writable = field.writable;
        let is_signer = field.signer;

        let meta = match (is_writable, is_signer) {
            (true, true) => quote! {
                solana_sdk::instruction::AccountMeta::new(self.#field_name, true)
            },
            (true, false) => quote! {
                solana_sdk::instruction::AccountMeta::new(self.#field_name, false)
            },
            (false, true) => quote! {
                solana_sdk::instruction::AccountMeta::new_readonly(self.#field_name, true)
            },
            (false, false) => quote! {
                solana_sdk::instruction::AccountMeta::new_readonly(self.#field_name, false)
            },
        };

        account_metas.push(meta);

        // Generate field assignment for from_pubkeys (parse direction)
        from_pubkeys_assignments.push(quote! {
            #field_name: pubkeys[#idx]
        });
    }

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Convert to a vector of AccountMeta for instruction building.
            #[must_use]
            pub fn to_account_metas(&self) -> Vec<solana_sdk::instruction::AccountMeta> {
                vec![
                    #(#account_metas),*
                ]
            }

            /// Number of accounts in this struct.
            pub const ACCOUNT_COUNT: usize = #field_count;

            /// Parse from a slice of account pubkeys (parse direction).
            ///
            /// Field order = account order. Each field is assigned the pubkey
            /// at the corresponding index in declaration order.
            pub fn from_pubkeys(
                pubkeys: &[solana_program::pubkey::Pubkey],
            ) -> std::result::Result<Self, crate::parsing::InstructionParseError> {
                if pubkeys.len() < Self::ACCOUNT_COUNT {
                    return Err(crate::parsing::InstructionParseError::NotEnoughAccounts {
                        expected: Self::ACCOUNT_COUNT,
                        actual: pubkeys.len(),
                    });
                }
                Ok(Self {
                    #(#from_pubkeys_assignments),*
                })
            }
        }

        impl #impl_generics crate::parsing::FromAccountKeys for #name #ty_generics #where_clause {
            const MIN_ACCOUNTS: usize = #field_count;

            fn from_account_keys(
                keys: &[solana_program::pubkey::Pubkey],
            ) -> std::result::Result<Self, crate::parsing::InstructionParseError> {
                Self::from_pubkeys(keys)
            }
        }
    }
}
