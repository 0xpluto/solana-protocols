//! LogParser derive macro implementation.
//!
//! Generates parsing for Solana program logs that contain base64-encoded event data.

use darling::{ast::Data, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr, Type};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(log_parser), supports(struct_named))]
struct LogParserArgs {
    ident: syn::Ident,
    generics: syn::Generics,
    data: Data<(), LogField>,
    /// Event discriminator - 8 bytes
    #[darling(default)]
    discriminator: Option<Expr>,
    /// Optional log prefix to strip (e.g., "Program data: ")
    #[darling(default)]
    log_prefix: Option<String>,
}

#[derive(Debug, FromField)]
#[darling(attributes(log))]
struct LogField {
    ident: Option<syn::Ident>,
    ty: Type,
    /// Skip N bytes before parsing this field
    #[darling(default)]
    skip_bytes: Option<usize>,
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match LogParserArgs::from_derive_input(&input) {
        Ok(args) => args,
        Err(e) => return e.write_errors().into(),
    };

    let expanded = generate_impl(&args);
    expanded.into()
}

fn generate_impl(args: &LogParserArgs) -> TokenStream2 {
    let name = &args.ident;
    let (impl_generics, ty_generics, where_clause) = args.generics.split_for_impl();

    let discriminator = match &args.discriminator {
        Some(expr) => quote! { #expr },
        None => {
            return syn::Error::new_spanned(
                name,
                "LogParser requires #[discriminator([...])] or #[discriminator(CONST)]",
            )
            .to_compile_error();
        }
    };

    let log_prefix = args.log_prefix.as_deref().unwrap_or("Program data: ");

    let fields = match &args.data {
        Data::Struct(fields) => fields,
        _ => {
            return syn::Error::new_spanned(name, "LogParser only supports structs")
                .to_compile_error();
        }
    };

    let mut parse_stmts = Vec::new();
    let mut field_names = Vec::new();
    let mut offset = quote! { 0usize };

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap();
        field_names.push(field_name.clone());

        // Handle skip_bytes attribute
        if let Some(skip) = field.skip_bytes {
            offset = quote! { #offset + #skip };
        }

        let field_ty = &field.ty;
        let (parse_stmt, size) = generate_field_parse(field_name, field_ty, &offset);
        parse_stmts.push(parse_stmt);

        offset = quote! { #offset + #size };
    }

    let total_size = quote! { 8 + #offset }; // discriminator + fields

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Event discriminator.
            pub const DISCRIMINATOR: [u8; 8] = #discriminator;

            /// Expected minimum size of event data.
            pub const DATA_SIZE: usize = #total_size;

            /// Log prefix that precedes the base64 data.
            pub const LOG_PREFIX: &'static str = #log_prefix;

            /// Try to parse this event from a log line.
            ///
            /// Returns `None` if the log doesn't match this event type.
            /// Returns `Some(Err(...))` if parsing fails.
            /// Returns `Some(Ok(event))` on success.
            pub fn try_from_log(log: &str) -> Option<Result<Self, Box<dyn std::error::Error + Send + Sync>>> {
                // Strip prefix
                let data_str = log.strip_prefix(Self::LOG_PREFIX)?;

                // Decode base64
                let data = match Self::decode_base64(data_str.trim()) {
                    Ok(d) => d,
                    Err(e) => return Some(Err(e)),
                };

                // Check discriminator
                if data.len() < 8 {
                    return None;
                }
                if &data[0..8] != Self::DISCRIMINATOR {
                    return None;
                }

                Some(Self::from_bytes(&data))
            }

            /// Parse from raw bytes (including discriminator).
            pub fn from_bytes(data: &[u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
                if data.len() < Self::DATA_SIZE {
                    return Err(format!(
                        "event data too short: {} bytes, need {}",
                        data.len(),
                        Self::DATA_SIZE
                    ).into());
                }

                // Verify discriminator
                if &data[0..8] != Self::DISCRIMINATOR {
                    return Err(format!(
                        "discriminator mismatch: expected {:?}, got {:?}",
                        Self::DISCRIMINATOR,
                        &data[0..8]
                    ).into());
                }

                // Skip discriminator
                let data = &data[8..];

                // Parse fields
                #(#parse_stmts)*

                Ok(Self {
                    #(#field_names),*
                })
            }

            /// Check if a log line matches this event's discriminator.
            pub fn matches_log(log: &str) -> bool {
                if let Some(data_str) = log.strip_prefix(Self::LOG_PREFIX) {
                    if let Ok(data) = Self::decode_base64(data_str.trim()) {
                        return data.len() >= 8 && &data[0..8] == Self::DISCRIMINATOR;
                    }
                }
                false
            }

            /// Decode base64 with standard alphabet.
            fn decode_base64(s: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
                // Simple base64 decode - in production use base64 crate
                use std::collections::HashMap;

                const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

                let mut decode_table: [u8; 256] = [255; 256];
                for (i, &c) in ALPHABET.iter().enumerate() {
                    decode_table[c as usize] = i as u8;
                }

                let input = s.as_bytes();
                let mut output = Vec::with_capacity(input.len() * 3 / 4);

                let mut buffer = 0u32;
                let mut bits = 0;

                for &byte in input {
                    if byte == b'=' {
                        break;
                    }

                    let value = decode_table[byte as usize];
                    if value == 255 {
                        continue; // Skip whitespace/invalid
                    }

                    buffer = (buffer << 6) | (value as u32);
                    bits += 6;

                    if bits >= 8 {
                        bits -= 8;
                        output.push((buffer >> bits) as u8);
                        buffer &= (1 << bits) - 1;
                    }
                }

                Ok(output)
            }

            /// Parse multiple events from a list of log lines.
            ///
            /// Returns all successfully parsed events of this type.
            pub fn parse_logs(logs: &[String]) -> Vec<Self> {
                logs.iter()
                    .filter_map(|log| Self::try_from_log(log))
                    .filter_map(|r| r.ok())
                    .collect()
            }
        }
    }
}

fn generate_field_parse(
    field_name: &syn::Ident,
    field_ty: &Type,
    offset: &TokenStream2,
) -> (TokenStream2, TokenStream2) {
    let ty_str = quote!(#field_ty).to_string();
    let field_name_str = field_name.to_string();

    match ty_str.as_str() {
        "u8" => (
            quote! {
                let #field_name = data[#offset];
            },
            quote! { 1usize },
        ),
        "bool" => (
            quote! {
                let #field_name = data[#offset] != 0;
            },
            quote! { 1usize },
        ),
        "u16" => (
            quote! {
                let #field_name = u16::from_le_bytes(
                    data[#offset..#offset + 2]
                        .try_into()
                        .map_err(|_| format!("failed to parse {}", #field_name_str))?
                );
            },
            quote! { 2usize },
        ),
        "i16" => (
            quote! {
                let #field_name = i16::from_le_bytes(
                    data[#offset..#offset + 2]
                        .try_into()
                        .map_err(|_| format!("failed to parse {}", #field_name_str))?
                );
            },
            quote! { 2usize },
        ),
        "u32" => (
            quote! {
                let #field_name = u32::from_le_bytes(
                    data[#offset..#offset + 4]
                        .try_into()
                        .map_err(|_| format!("failed to parse {}", #field_name_str))?
                );
            },
            quote! { 4usize },
        ),
        "i32" => (
            quote! {
                let #field_name = i32::from_le_bytes(
                    data[#offset..#offset + 4]
                        .try_into()
                        .map_err(|_| format!("failed to parse {}", #field_name_str))?
                );
            },
            quote! { 4usize },
        ),
        "u64" => (
            quote! {
                let #field_name = u64::from_le_bytes(
                    data[#offset..#offset + 8]
                        .try_into()
                        .map_err(|_| format!("failed to parse {}", #field_name_str))?
                );
            },
            quote! { 8usize },
        ),
        "i64" => (
            quote! {
                let #field_name = i64::from_le_bytes(
                    data[#offset..#offset + 8]
                        .try_into()
                        .map_err(|_| format!("failed to parse {}", #field_name_str))?
                );
            },
            quote! { 8usize },
        ),
        "u128" => (
            quote! {
                let #field_name = u128::from_le_bytes(
                    data[#offset..#offset + 16]
                        .try_into()
                        .map_err(|_| format!("failed to parse {}", #field_name_str))?
                );
            },
            quote! { 16usize },
        ),
        "i128" => (
            quote! {
                let #field_name = i128::from_le_bytes(
                    data[#offset..#offset + 16]
                        .try_into()
                        .map_err(|_| format!("failed to parse {}", #field_name_str))?
                );
            },
            quote! { 16usize },
        ),
        "Pubkey" => (
            quote! {
                let #field_name = solana_program::pubkey::Pubkey::try_from(
                    &data[#offset..#offset + 32]
                ).map_err(|_| format!("failed to parse {} as Pubkey", #field_name_str))?;
            },
            quote! { 32usize },
        ),
        _ => {
            // Try to handle arrays like [u8; 32]
            if ty_str.starts_with('[') && ty_str.contains(';') {
                (
                    quote! {
                        let #field_name: #field_ty = data[#offset..#offset + std::mem::size_of::<#field_ty>()]
                            .try_into()
                            .map_err(|_| format!("failed to parse {}", #field_name_str))?;
                    },
                    quote! { std::mem::size_of::<#field_ty>() },
                )
            } else {
                (
                    quote! {
                        compile_error!(concat!("Unsupported type for LogParser: ", stringify!(#field_ty)));
                    },
                    quote! { 0usize },
                )
            }
        }
    }
}
