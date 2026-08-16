//! InstructionData derive macro implementation.
//!
//! Supports multiple discriminator modes:
//! - Standard 8-byte Anchor discriminator (default)
//! - Variable-size discriminator (1, 2, 4, 8 bytes)
//! - No discriminator (for legacy programs with implicit instruction indices)
//!
//! # Examples
//!
//! ```ignore
//! // Standard 8-byte Anchor discriminator
//! #[derive(InstructionData)]
//! #[instruction_data(discriminator = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea])]
//! pub struct BuyParams { ... }
//!
//! // 1-byte instruction index (Raydium V4 style)
//! #[derive(InstructionData)]
//! #[instruction_data(discriminator = [16], discriminator_size = 1)]
//! pub struct SwapBaseInParams { ... }
//!
//! // No discriminator (instruction index handled externally)
//! #[derive(InstructionData)]
//! #[instruction_data(no_discriminator)]
//! pub struct SomeParams { ... }
//! ```

use darling::{ast::Data, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr, Type};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(instruction_data), supports(struct_named))]
struct InstructionDataArgs {
    ident: syn::Ident,
    generics: syn::Generics,
    data: Data<(), InstructionDataField>,

    /// Discriminator - either array literal or const reference.
    /// Not required if `no_discriminator` is set.
    #[darling(default)]
    discriminator: Option<Expr>,

    /// Discriminator size in bytes. Defaults to 8 for Anchor compatibility.
    /// Valid values: 1, 2, 4, 8.
    #[darling(default)]
    discriminator_size: Option<usize>,

    /// Skip discriminator entirely. For instruction indices handled externally.
    #[darling(default)]
    no_discriminator: bool,
}

#[derive(Debug, FromField)]
#[darling(attributes(instruction))]
struct InstructionDataField {
    ident: Option<syn::Ident>,
    ty: Type,
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match InstructionDataArgs::from_derive_input(&input) {
        Ok(args) => args,
        Err(e) => return e.write_errors().into(),
    };

    let expanded = generate_impl(&args);
    expanded.into()
}

fn generate_impl(args: &InstructionDataArgs) -> TokenStream2 {
    let name = &args.ident;
    let (impl_generics, ty_generics, where_clause) = args.generics.split_for_impl();

    // Determine discriminator mode
    let disc_size = if args.no_discriminator {
        0
    } else {
        args.discriminator_size.unwrap_or(8)
    };

    // Validate discriminator size
    if !matches!(disc_size, 0 | 1 | 2 | 4 | 8) {
        return syn::Error::new_spanned(name, "discriminator_size must be 0, 1, 2, 4, or 8")
            .to_compile_error();
    }

    // Validate discriminator is provided when needed
    if disc_size > 0 && args.discriminator.is_none() {
        return syn::Error::new_spanned(
            name,
            format!(
                "InstructionData requires discriminator when discriminator_size={} (use no_discriminator for size 0)",
                disc_size
            ),
        )
        .to_compile_error();
    }

    let fields = match &args.data {
        Data::Struct(fields) => fields,
        _ => {
            return syn::Error::new_spanned(name, "InstructionData only supports structs")
                .to_compile_error();
        }
    };

    let mut serialize_stmts = Vec::new();
    let mut deserialize_stmts = Vec::new();
    let mut field_names_for_deser = Vec::new();
    let mut size_calc = quote! { 0 };
    // Fields whose type this derive cannot decode. Previously the derive
    // silently emitted NO `from_instruction_data` for such a struct, which is
    // the house failure: machinery that reads as a finished layer while doing
    // nothing. A params struct with no parse path looks decorated and is
    // undecodable, and nothing anywhere says so.
    let mut undecodable: Vec<String> = Vec::new();

    for (idx, field) in fields.iter().enumerate() {
        let field_name = field.ident.as_ref().unwrap();
        let field_ty = &field.ty;

        // Try to generate deserialization for this field
        // A trailing `OptionBool` consumes whatever remains, including nothing.
        // It is the pump family's optional-argument convention and appears on
        // five instructions across two programs, so the derive knows it rather
        // than each struct hand-rolling the same three lines.
        //
        // Required to be LAST, and that is a guarantee rather than a
        // convention: it eats the rest of the buffer, so any field after it
        // could never be read. The compiler enforces the ordering instead of a
        // reviewer noticing.
        let is_option_bool = quote::ToTokens::to_token_stream(field_ty)
            .to_string()
            .replace(' ', "")
            .ends_with("OptionBool");
        if is_option_bool {
            if idx + 1 != fields.len() {
                return syn::Error::new_spanned(
                    field_ty,
                    "a trailing OptionBool must be the final field: it consumes the \
                     remainder of the instruction data, so any field after it is \
                     unreachable",
                )
                .to_compile_error();
            }
            deserialize_stmts.push(quote! {
                let #field_name = ::solana_protocols::protocols::OptionBool::from_bytes(&data[offset..])
                    .map_err(|e| crate::parsing::InstructionParseError::DeserializationFailed(
                        e.to_string()
                    ))?;
            });
            field_names_for_deser.push(field_name.clone());
            serialize_stmts.push(quote! {
                data.extend_from_slice(self.#field_name.to_bytes());
            });
            continue;
        }

        let (serialize, size) = generate_field_serialize(field_name, field_ty);
        serialize_stmts.push(serialize);
        size_calc = quote! { #size_calc + #size };

        if let Some((deser, _deser_size)) = generate_field_deserialize(field_name, field_ty) {
            deserialize_stmts.push(deser);
            field_names_for_deser.push(field_name.clone());
        } else {
            undecodable.push(format!(
                "`{field_name}: {}`",
                quote::ToTokens::to_token_stream(field_ty)
            ));
        }
    }

    // Generate code based on discriminator mode
    if !undecodable.is_empty() {
        // Refuse rather than skip. The author's options are to use a type this
        // derive understands, or to write the impl by hand deliberately — both
        // fine, and both better than an invisible gap.
        return syn::Error::new_spanned(
            name,
            format!(
                "InstructionData cannot decode {}: {}. This derive handles only \
                 fixed-size primitives (integers, bool, Pubkey, [u8; N]); strings, \
                 vectors and defined types need a hand-written \
                 `FromInstructionData`. Emitting no decoder silently is how a \
                 params struct ends up undecodable while looking complete.",
                if undecodable.len() == 1 {
                    "a field"
                } else {
                    "fields"
                },
                undecodable.join(", ")
            ),
        )
        .to_compile_error();
    }
    let deser_info = Some((&deserialize_stmts, &field_names_for_deser));

    match disc_size {
        0 => generate_no_discriminator_impl(
            name,
            &impl_generics,
            &ty_generics,
            where_clause,
            &size_calc,
            &serialize_stmts,
            deser_info,
        ),
        _ => generate_discriminator_impl(
            name,
            &impl_generics,
            &ty_generics,
            where_clause,
            args.discriminator.as_ref().unwrap(),
            disc_size,
            &size_calc,
            &serialize_stmts,
            deser_info,
        ),
    }
}

/// Generate implementation with no discriminator.
fn generate_no_discriminator_impl(
    name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    fields_size: &TokenStream2,
    serialize_stmts: &[TokenStream2],
    deser_info: Option<(&Vec<TokenStream2>, &Vec<syn::Ident>)>,
) -> TokenStream2 {
    let from_instruction_data_impl = if let Some((deser_stmts, field_names)) = deser_info {
        let expected_size = fields_size.clone();
        quote! {
            impl #impl_generics crate::parsing::FromInstructionData for #name #ty_generics #where_clause {
                fn from_instruction_data(data: &[u8]) -> std::result::Result<Self, crate::parsing::InstructionParseError> {
                    let expected: usize = #expected_size;
                    if data.len() < expected {
                        return Err(crate::parsing::InstructionParseError::DeserializationFailed(
                            format!("{}: expected {} bytes, got {}", stringify!(#name), expected, data.len())
                        ));
                    }
                    #[allow(unused_variables)]
                    let mut offset: usize = 0;
                    #(#deser_stmts)*
                    Ok(Self { #(#field_names),* })
                }
            }
        }
    } else {
        quote! {}
    };

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Discriminator size (0 = no discriminator).
            pub const DISCRIMINATOR_SIZE: usize = 0;

            /// Serialize to instruction data bytes (no discriminator prefix).
            #[must_use]
            pub fn to_data(&self) -> Vec<u8> {
                let capacity = #fields_size;
                let mut data = Vec::with_capacity(capacity);

                // Write fields only (no discriminator)
                #(#serialize_stmts)*

                data
            }
        }

        #from_instruction_data_impl
    }
}

/// Generate implementation with discriminator.
#[allow(clippy::too_many_arguments)]
fn generate_discriminator_impl(
    name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    discriminator: &Expr,
    disc_size: usize,
    fields_size: &TokenStream2,
    serialize_stmts: &[TokenStream2],
    deser_info: Option<(&Vec<TokenStream2>, &Vec<syn::Ident>)>,
) -> TokenStream2 {
    let total_size = quote! { #disc_size + #fields_size };

    // Generate discriminator constant and method with correct type
    let (disc_const, disc_method) = match disc_size {
        1 => (
            quote! {
                /// Instruction discriminator.
                pub const DISCRIMINATOR: [u8; 1] = #discriminator;
            },
            quote! {
                /// Get the instruction discriminator.
                #[must_use]
                pub const fn discriminator() -> [u8; 1] {
                    #discriminator
                }
            },
        ),
        2 => (
            quote! {
                /// Instruction discriminator.
                pub const DISCRIMINATOR: [u8; 2] = #discriminator;
            },
            quote! {
                /// Get the instruction discriminator.
                #[must_use]
                pub const fn discriminator() -> [u8; 2] {
                    #discriminator
                }
            },
        ),
        4 => (
            quote! {
                /// Instruction discriminator.
                pub const DISCRIMINATOR: [u8; 4] = #discriminator;
            },
            quote! {
                /// Get the instruction discriminator.
                #[must_use]
                pub const fn discriminator() -> [u8; 4] {
                    #discriminator
                }
            },
        ),
        8 => (
            quote! {
                /// Instruction discriminator.
                pub const DISCRIMINATOR: [u8; 8] = #discriminator;
            },
            quote! {
                /// Get the instruction discriminator.
                #[must_use]
                pub const fn discriminator() -> [u8; 8] {
                    #discriminator
                }
            },
        ),
        _ => unreachable!(),
    };

    let from_instruction_data_impl = if let Some((deser_stmts, field_names)) = deser_info {
        let expected_size = fields_size.clone();
        quote! {
            impl #impl_generics crate::parsing::FromInstructionData for #name #ty_generics #where_clause {
                fn from_instruction_data(data: &[u8]) -> std::result::Result<Self, crate::parsing::InstructionParseError> {
                    let expected: usize = #expected_size;
                    if data.len() < expected {
                        return Err(crate::parsing::InstructionParseError::DeserializationFailed(
                            format!("{}: expected {} bytes, got {}", stringify!(#name), expected, data.len())
                        ));
                    }
                    #[allow(unused_variables)]
                    let mut offset: usize = 0;
                    #(#deser_stmts)*
                    Ok(Self { #(#field_names),* })
                }
            }
        }
    } else {
        quote! {}
    };

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            #disc_const

            /// Discriminator size in bytes.
            pub const DISCRIMINATOR_SIZE: usize = #disc_size;

            /// Serialize to instruction data bytes.
            ///
            /// Format: discriminator (#disc_size bytes) + fields in order (little-endian)
            #[must_use]
            pub fn to_data(&self) -> Vec<u8> {
                let capacity = #total_size;
                let mut data = Vec::with_capacity(capacity);

                // Write discriminator
                data.extend_from_slice(&Self::DISCRIMINATOR);

                // Write fields
                #(#serialize_stmts)*

                data
            }

            #disc_method
        }

        #from_instruction_data_impl
    }
}

/// Generate deserialization code for a single field.
///
/// Returns (deserialize_stmt, size_expr) — the symmetric counterpart of
/// `generate_field_serialize`. Reads each field from `data` at `offset`.
fn generate_field_deserialize(
    field_name: &syn::Ident,
    field_ty: &Type,
) -> Option<(TokenStream2, TokenStream2)> {
    let ty_str = quote!(#field_ty).to_string();

    match ty_str.as_str() {
        "u8" => Some((
            quote! {
                let #field_name = data[offset];
                offset += 1;
            },
            quote! { 1 },
        )),
        "bool" => Some((
            quote! {
                let #field_name = data[offset] != 0;
                offset += 1;
            },
            quote! { 1 },
        )),
        "u16" => Some((
            quote! {
                let #field_name = u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap());
                offset += 2;
            },
            quote! { 2 },
        )),
        "i16" => Some((
            quote! {
                let #field_name = i16::from_le_bytes(data[offset..offset + 2].try_into().unwrap());
                offset += 2;
            },
            quote! { 2 },
        )),
        "u32" => Some((
            quote! {
                let #field_name = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                offset += 4;
            },
            quote! { 4 },
        )),
        "i32" => Some((
            quote! {
                let #field_name = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                offset += 4;
            },
            quote! { 4 },
        )),
        "u64" => Some((
            quote! {
                let #field_name = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                offset += 8;
            },
            quote! { 8 },
        )),
        "i64" => Some((
            quote! {
                let #field_name = i64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
                offset += 8;
            },
            quote! { 8 },
        )),
        "u128" => Some((
            quote! {
                let #field_name = u128::from_le_bytes(data[offset..offset + 16].try_into().unwrap());
                offset += 16;
            },
            quote! { 16 },
        )),
        "i128" => Some((
            quote! {
                let #field_name = i128::from_le_bytes(data[offset..offset + 16].try_into().unwrap());
                offset += 16;
            },
            quote! { 16 },
        )),
        "Pubkey" => Some((
            quote! {
                let #field_name = solana_program::pubkey::Pubkey::try_from(&data[offset..offset + 32])
                    .map_err(|_| crate::parsing::InstructionParseError::DeserializationFailed(
                        format!("invalid Pubkey at offset {}", offset)
                    ))?;
                offset += 32;
            },
            quote! { 32 },
        )),
        _ => {
            // Fixed-size arrays like [u8; 32]
            if ty_str.starts_with('[') && ty_str.contains(';') {
                Some((
                    quote! {
                        let size = std::mem::size_of::<#field_ty>();
                        let mut #field_name: #field_ty = unsafe { std::mem::zeroed() };
                        #field_name.copy_from_slice(&data[offset..offset + size]);
                        offset += size;
                    },
                    quote! { std::mem::size_of::<#field_ty>() },
                ))
            } else {
                // Variable-length or unsupported types (e.g. String) — cannot auto-generate
                None
            }
        }
    }
}

fn generate_field_serialize(
    field_name: &syn::Ident,
    field_ty: &Type,
) -> (TokenStream2, TokenStream2) {
    let ty_str = quote!(#field_ty).to_string();

    match ty_str.as_str() {
        "u8" => (quote! { data.push(self.#field_name); }, quote! { 1 }),
        "bool" => (
            quote! { data.push(if self.#field_name { 1 } else { 0 }); },
            quote! { 1 },
        ),
        "u16" => (
            quote! { data.extend_from_slice(&self.#field_name.to_le_bytes()); },
            quote! { 2 },
        ),
        "i16" => (
            quote! { data.extend_from_slice(&self.#field_name.to_le_bytes()); },
            quote! { 2 },
        ),
        "u32" => (
            quote! { data.extend_from_slice(&self.#field_name.to_le_bytes()); },
            quote! { 4 },
        ),
        "i32" => (
            quote! { data.extend_from_slice(&self.#field_name.to_le_bytes()); },
            quote! { 4 },
        ),
        "u64" => (
            quote! { data.extend_from_slice(&self.#field_name.to_le_bytes()); },
            quote! { 8 },
        ),
        "i64" => (
            quote! { data.extend_from_slice(&self.#field_name.to_le_bytes()); },
            quote! { 8 },
        ),
        "u128" => (
            quote! { data.extend_from_slice(&self.#field_name.to_le_bytes()); },
            quote! { 16 },
        ),
        "i128" => (
            quote! { data.extend_from_slice(&self.#field_name.to_le_bytes()); },
            quote! { 16 },
        ),
        "Pubkey" => (
            quote! { data.extend_from_slice(self.#field_name.as_ref()); },
            quote! { 32 },
        ),
        _ => {
            // Try to handle arrays like [u8; 32]
            if ty_str.starts_with('[') && ty_str.contains(';') {
                (
                    quote! { data.extend_from_slice(&self.#field_name); },
                    quote! { std::mem::size_of::<#field_ty>() },
                )
            } else {
                // Fallback: assume the type has as_ref() returning bytes
                (
                    quote! { data.extend_from_slice(self.#field_name.as_ref()); },
                    quote! { std::mem::size_of::<#field_ty>() },
                )
            }
        }
    }
}
