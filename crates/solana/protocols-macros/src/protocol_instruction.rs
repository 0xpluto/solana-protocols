//! ProtocolInstruction derive macro implementation.
//!
//! This macro generates comprehensive instruction parsing for protocol enums.
//! It creates:
//! - `try_from_slice()` - Parse bytes by discriminator matching
//! - `discriminator()` - Get discriminator for each variant
//! - `data()` - Serialize instruction back to bytes
//! - Accounts enum with variants matching instruction variants
//! - Event struct combining instruction + accounts + optional log

use darling::{ast::Data, FromDeriveInput, FromVariant};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, Expr, Ident, Path};

/// Top-level attributes on the enum.
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(protocol), supports(enum_any))]
struct ProtocolInstructionArgs {
    ident: Ident,
    generics: syn::Generics,
    data: Data<InstructionVariant, ()>,

    /// Program ID constant or expression.
    #[darling(default)]
    program_id: Option<Expr>,

    /// Optional custom name for the generated Event struct.
    /// Defaults to `{EnumName}Event`.
    #[darling(default)]
    event_name: Option<Ident>,

    /// Optional custom name for the generated Accounts enum.
    /// Defaults to `{EnumName}Accounts`.
    #[darling(default)]
    accounts_name: Option<Ident>,

    /// Discriminator size in bytes. Defaults to 8 (Anchor-style).
    /// Use 1 for SPL Token-style programs that use a single-byte instruction index.
    /// Valid values: 1, 2, 4, 8.
    #[darling(default)]
    discriminator_size: Option<usize>,
}

/// Per-variant attributes.
#[derive(Debug, FromVariant)]
#[darling(attributes(instruction))]
struct InstructionVariant {
    ident: Ident,
    fields: darling::ast::Fields<syn::Field>,

    /// Discriminator - array literal or const reference.
    /// Required for each variant.
    discriminator: Expr,

    /// Accounts builder struct for this instruction.
    /// If provided, will be included in the generated Accounts enum.
    #[darling(default)]
    accounts: Option<Path>,

    /// Log struct for this instruction.
    /// If provided, enables log parsing for this variant.
    // Parsed from the derive attribute but not yet consumed in codegen.
    #[allow(dead_code)]
    #[darling(default)]
    log: Option<Path>,
}

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match ProtocolInstructionArgs::from_derive_input(&input) {
        Ok(args) => args,
        Err(e) => return e.write_errors().into(),
    };

    let expanded = match generate_impl(&args) {
        Ok(tokens) => tokens,
        Err(e) => return e.to_compile_error().into(),
    };

    expanded.into()
}

fn generate_impl(args: &ProtocolInstructionArgs) -> Result<TokenStream2, syn::Error> {
    let enum_name = &args.ident;
    let (impl_generics, ty_generics, where_clause) = args.generics.split_for_impl();
    let disc_size = args.discriminator_size.unwrap_or(8);

    // Validate discriminator size
    if !matches!(disc_size, 1 | 2 | 4 | 8) {
        return Err(syn::Error::new_spanned(
            enum_name,
            "discriminator_size must be 1, 2, 4, or 8",
        ));
    }

    let variants = match &args.data {
        Data::Enum(variants) => variants,
        _ => {
            return Err(syn::Error::new_spanned(
                enum_name,
                "ProtocolInstruction only supports enums",
            ));
        }
    };

    // Validate all variants have exactly one unnamed field
    for variant in variants {
        match &variant.fields.style {
            darling::ast::Style::Tuple => {
                if variant.fields.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        &variant.ident,
                        "Each variant must have exactly one field (the params type)",
                    ));
                }
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    &variant.ident,
                    "Each variant must be a tuple variant with one field, e.g., Buy(BuyParams)",
                ));
            }
        }
    }

    // Generate names for derived types
    let event_name = args
        .event_name
        .clone()
        .unwrap_or_else(|| format_ident!("{}Event", enum_name));
    let accounts_enum_name = args
        .accounts_name
        .clone()
        .unwrap_or_else(|| format_ident!("{}Accounts", enum_name));

    // Generate try_from_slice implementation
    let try_from_slice_impl = generate_try_from_slice(enum_name, variants, disc_size)?;

    // Generate discriminator implementation
    let discriminator_impl = generate_discriminator(enum_name, variants, disc_size);

    // Generate data implementation
    let data_impl = generate_data(enum_name, variants);

    // Generate accounts enum
    let accounts_enum = generate_accounts_enum(&accounts_enum_name, variants);

    // Generate from_accounts implementation
    let from_accounts_impl = generate_from_accounts(enum_name, &accounts_enum_name, variants);

    // Generate Event struct
    let event_struct =
        generate_event_struct(&event_name, enum_name, &accounts_enum_name, disc_size);

    // Generate program_id method if provided
    let program_id_impl = if let Some(program_id) = &args.program_id {
        quote! {
            /// Get the program ID for this instruction.
            #[must_use]
            pub fn program_id() -> solana_program::pubkey::Pubkey {
                #program_id
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        impl #impl_generics #enum_name #ty_generics #where_clause {
            #try_from_slice_impl

            #discriminator_impl

            #data_impl

            #from_accounts_impl

            #program_id_impl
        }

        #accounts_enum

        #event_struct
    })
}

/// Generate try_from_slice method.
fn generate_try_from_slice(
    enum_name: &Ident,
    variants: &[InstructionVariant],
    disc_size: usize,
) -> Result<TokenStream2, syn::Error> {
    let mut disc_checks = Vec::new();
    let disc_size_lit = proc_macro2::Literal::usize_unsuffixed(disc_size);

    for variant in variants {
        let variant_name = &variant.ident;
        let discriminator = &variant.discriminator;

        // Get the params type from the single field
        let params_type = &variant.fields.fields[0].ty;

        disc_checks.push(quote! {
            {
                let disc: [u8; #disc_size_lit] = #discriminator;
                if data.len() >= #disc_size_lit && data[..#disc_size_lit] == disc {
                    let params = <#params_type as crate::parsing::FromInstructionData>::from_instruction_data(&data[#disc_size_lit..])?;
                    return Ok(#enum_name::#variant_name(params));
                }
            }
        });
    }

    // Error reporting: pad discriminator bytes to 8 for UnknownDiscriminator
    let error_report = if disc_size >= 8 {
        quote! {
            let mut disc = [0u8; 8];
            disc.copy_from_slice(&data[..8]);
            Err(crate::parsing::InstructionParseError::UnknownDiscriminator(disc))
        }
    } else {
        quote! {
            let mut disc = [0u8; 8];
            disc[..#disc_size_lit].copy_from_slice(&data[..#disc_size_lit]);
            Err(crate::parsing::InstructionParseError::UnknownDiscriminator(disc))
        }
    };

    let doc = format!(
        "Parse instruction data into the appropriate variant.\n\nMatches the first {} byte(s) against known discriminators.",
        disc_size
    );

    Ok(quote! {
        #[doc = #doc]
        pub fn try_from_slice(data: &[u8]) -> Result<Self, crate::parsing::InstructionParseError> {
            if data.len() < #disc_size_lit {
                return Err(crate::parsing::InstructionParseError::DataTooShort);
            }

            #(#disc_checks)*

            // No discriminator matched
            #error_report
        }
    })
}

/// Generate discriminator method.
fn generate_discriminator(
    enum_name: &Ident,
    variants: &[InstructionVariant],
    disc_size: usize,
) -> TokenStream2 {
    let mut match_arms = Vec::new();
    let disc_size_lit = proc_macro2::Literal::usize_unsuffixed(disc_size);

    for variant in variants {
        let variant_name = &variant.ident;
        let discriminator = &variant.discriminator;

        match_arms.push(quote! {
            #enum_name::#variant_name(_) => #discriminator,
        });
    }

    quote! {
        /// Get the discriminator for this instruction variant.
        #[must_use]
        pub fn discriminator(&self) -> [u8; #disc_size_lit] {
            match self {
                #(#match_arms)*
            }
        }
    }
}

/// Generate data method.
fn generate_data(enum_name: &Ident, variants: &[InstructionVariant]) -> TokenStream2 {
    let mut match_arms = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        match_arms.push(quote! {
            #enum_name::#variant_name(params) => params.to_data(),
        });
    }

    quote! {
        /// Serialize the instruction back to bytes.
        #[must_use]
        pub fn data(&self) -> Vec<u8> {
            match self {
                #(#match_arms)*
            }
        }
    }
}

/// Generate the Accounts enum.
fn generate_accounts_enum(
    accounts_enum_name: &Ident,
    variants: &[InstructionVariant],
) -> TokenStream2 {
    let mut enum_variants = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        if let Some(accounts_type) = &variant.accounts {
            enum_variants.push(quote! {
                #variant_name(#accounts_type)
            });
        } else {
            // If no accounts specified, use unit variant
            enum_variants.push(quote! {
                #variant_name
            });
        }
    }

    quote! {
        /// Accounts enum for instruction variants.
        ///
        /// Each variant contains the resolved account pubkeys for that instruction.
        #[derive(Debug, Clone)]
        pub enum #accounts_enum_name {
            #(#enum_variants),*
        }
    }
}

/// Generate from_accounts method.
fn generate_from_accounts(
    enum_name: &Ident,
    accounts_enum_name: &Ident,
    variants: &[InstructionVariant],
) -> TokenStream2 {
    let mut match_arms = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        if let Some(accounts_type) = &variant.accounts {
            match_arms.push(quote! {
                #enum_name::#variant_name(_) => {
                    let accounts = <#accounts_type as crate::parsing::FromAccountKeys>::from_account_keys(account_keys)?;
                    Ok(#accounts_enum_name::#variant_name(accounts))
                }
            });
        } else {
            match_arms.push(quote! {
                #enum_name::#variant_name(_) => Ok(#accounts_enum_name::#variant_name),
            });
        }
    }

    quote! {
        /// Parse account pubkeys into the corresponding accounts variant.
        pub fn from_accounts(
            &self,
            account_keys: &[solana_program::pubkey::Pubkey],
        ) -> Result<#accounts_enum_name, crate::parsing::InstructionParseError> {
            match self {
                #(#match_arms)*
            }
        }
    }
}

/// Generate the Event struct.
fn generate_event_struct(
    event_name: &Ident,
    enum_name: &Ident,
    accounts_enum_name: &Ident,
    disc_size: usize,
) -> TokenStream2 {
    let disc_size_lit = proc_macro2::Literal::usize_unsuffixed(disc_size);

    quote! {
        /// Parsed protocol event combining instruction, accounts, and log.
        ///
        /// This struct represents a fully parsed instruction from a transaction,
        /// including the resolved account pubkeys and any associated log data.
        #[derive(Debug, Clone)]
        pub struct #event_name {
            /// The parsed instruction with its parameters.
            pub instruction: #enum_name,

            /// The resolved account pubkeys.
            pub accounts: #accounts_enum_name,

            /// Associated log data (discriminator + payload).
            /// None if no matching log was found.
            pub log_data: Option<Vec<u8>>,

            /// Whether transaction logs were truncated.
            pub logs_truncated: bool,
        }

        impl #event_name {
            /// Create a new event from parsed components.
            #[must_use]
            pub fn new(
                instruction: #enum_name,
                accounts: #accounts_enum_name,
                log_data: Option<Vec<u8>>,
                logs_truncated: bool,
            ) -> Self {
                Self {
                    instruction,
                    accounts,
                    log_data,
                    logs_truncated,
                }
            }

            /// Get the instruction discriminator.
            #[must_use]
            pub fn discriminator(&self) -> [u8; #disc_size_lit] {
                self.instruction.discriminator()
            }
        }
    }
}
