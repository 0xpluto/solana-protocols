//! AccountMetas derive macro implementation.

use darling::{ast::Data, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
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
    /// Anchor `optional: true`: keeps its slot, holding the **program id**
    /// when absent. Typed `Option<Pubkey>`. Needs `#[accounts(program_id = …)]`
    /// on the struct, because the sentinel *is* that id.
    #[darling(default)]
    optional: bool,
    /// A named account past the declared list: absent means the slot does not
    /// exist. Typed `Conditional`, never `Option` — see
    /// `solana_protocols::parsing::accounts`.
    #[darling(default)]
    conditional: bool,
    /// An appended account located by **deriving its address and searching**,
    /// never by index.
    ///
    /// The value is a Rust expression evaluated during parsing, where every
    /// preceding field is in scope as a binding — so a PDA of the instruction's
    /// own mint or user is written directly. Position-independent by
    /// construction: pumpfun appends `bonding_curve_v2` at index 0 or 1
    /// depending on whether a cashback accumulator took index 0, and appends the
    /// same account at different indices on different instructions.
    ///
    /// Typed `Conditional`, since an appended account that is not there has no
    /// slot at all.
    #[darling(default)]
    resolved: Option<syn::Expr>,
    /// Captures every account past the declared list.
    ///
    /// Anchor programs may be sent more accounts than their IDL declares —
    /// pumpfun's `sell_v2` is declared at 26 and arrives at 26, 27, 28 and 29 —
    /// and they land as a suffix, past the `event_authority`/`program`
    /// terminator. Without this the parser dropped them: it never read a slot
    /// that was not this instruction's, but it did not account for every slot
    /// that was. Must be the final field, and typed `Vec<Pubkey>`.
    #[darling(default)]
    remaining: bool,
    /// Why this layout legitimately receives a variable tail.
    ///
    /// Mandatory with `remaining`, and deliberately so. Unmodelled accounts are
    /// an error now, and the cheapest way to silence any error is to widen the
    /// thing that raises it — a `Vec<Pubkey>` named `remaining_accounts` makes
    /// the message go away and teaches nobody anything. Requiring a stated
    /// reason, next to a field named for what it actually holds, makes the
    /// escape hatch cost more than identifying the accounts usually does.
    #[darling(default)]
    reason: Option<String>,
}

/// Field names that describe the mechanism instead of the contents.
///
/// Rejected on an `#[account(remaining)]` field: `remaining` is Anchor's word
/// for *how* the accounts arrive, and a consumer holding
/// `accounts.remaining[2]` has to know an index nobody wrote down. The whole
/// reason the tail is modelled at all is so that stops being true.
const PLACEHOLDER_NAMES: [&str; 8] = [
    "remaining",
    "remaining_accounts",
    "rest",
    "extra",
    "extras",
    "extra_accounts",
    "other",
    "unknown",
];

pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match AccountMetasArgs::from_derive_input(&input) {
        Ok(args) => args,
        Err(e) => return e.write_errors().into(),
    };

    // Verify the declared account order against the program's own IDL before
    // generating anything. This is the one fact about an instruction we cannot
    // derive — a discriminator comes from the name, a PDA from its seeds, but
    // "account 12 is the pool's quote vault" is data — so the IDL is the
    // authority and disagreeing with it must not compile.
    for attr in &input.attrs {
        if !attr.path().is_ident("idl") {
            continue;
        }
        let (mut program, mut instruction) = (None, None);
        if let Err(e) = attr.parse_nested_meta(|m| {
            if m.path.is_ident("program") {
                program = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else if m.path.is_ident("instruction") {
                instruction = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else {
                return Err(m.error("expected program = \"…\" and instruction = \"…\""));
            }
            Ok(())
        }) {
            return e.to_compile_error().into();
        }
        let (Some(program), Some(instruction)) = (program, instruction) else {
            return syn::Error::new_spanned(
                attr,
                "#[idl(...)] needs both program = \"…\" and instruction = \"…\"",
            )
            .to_compile_error()
            .into();
        };
        // `remaining` is ours, not the IDL's — it exists precisely because the
        // IDL's list is a minimum. Including it would fail every check by one.
        let field_names: Vec<String> = match &input.data {
            syn::Data::Struct(s) => s
                .fields
                .iter()
                .filter(|f| {
                    !f.attrs.iter().any(|a| {
                        let t = a.to_token_stream().to_string();
                        a.path().is_ident("account")
                            && (t.contains("remaining")
                                || t.contains("resolved")
                                || t.contains("conditional"))
                    })
                })
                .filter_map(|f| f.ident.as_ref().map(ToString::to_string))
                .collect(),
            _ => Vec::new(),
        };
        if let Err(msg) = crate::idl_check::check_accounts(&program, &instruction, &field_names) {
            return syn::Error::new_spanned(&input.ident, msg)
                .to_compile_error()
                .into();
        }
    }

    // The sentinel for an absent Anchor-optional account is the program's own
    // id, so a struct declaring one has to say which program.
    let mut program_id: Option<syn::Expr> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("accounts") {
            continue;
        }
        if let Err(e) = attr.parse_nested_meta(|m| {
            if m.path.is_ident("program_id") {
                program_id = Some(m.value()?.parse()?);
                Ok(())
            } else if m.path.is_ident("unverified") {
                // Read by the golden-fixture gate above; accepted here so one
                // `#[accounts(...)]` can carry both.
                let _: syn::LitStr = m.value()?.parse()?;
                Ok(())
            } else {
                Err(m.error("expected program_id = … or unverified = …"))
            }
        }) {
            return e.to_compile_error().into();
        }
    }

    // An accounts struct must be pinned against a real landed instruction, or
    // say why not. `#[derive(AccountMetas)]` alone generates a parser that has
    // never been compared to a real account list — which is how a struct
    // declaring 18 accounts against a real 15 shipped and stayed silent, because
    // nothing was watching it. `#[derive(OnchainInstruction)]` supplies the
    // proof; this makes its absence a build failure rather than an omission
    // nobody can see.
    let mut unverified: Option<String> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("accounts") {
            continue;
        }
        let _ = attr.parse_nested_meta(|m| {
            if m.path.is_ident("unverified") {
                unverified = Some(m.value()?.parse::<syn::LitStr>()?.value());
            } else {
                // Consume the value anyway. Returning early without it left the
                // parser sitting on `=`, so a struct listing `program_id` before
                // `unverified` never reached the reason it had supplied.
                let _: syn::Expr = m.value()?.parse()?;
            }
            Ok(())
        });
    }
    let has_proof = input
        .attrs
        .iter()
        .any(|a| a.path().is_ident("onchain_ix"));
    if !has_proof && unverified.as_ref().is_none_or(|r| r.trim().len() < 12) {
        return syn::Error::new_spanned(
            &input.ident,
            "an accounts struct needs #[derive(OnchainInstruction)] with \
             #[onchain_ix(fixtures(\"…\"))] — one real landed instruction per account \
             count observed on chain — or #[accounts(unverified = \"why not\")]. A \
             parser never compared against a real account list is a parser nobody \
             has checked",
        )
        .to_compile_error()
        .into();
    }

    let expanded = generate_impl(&args, program_id.as_ref());
    expanded.into()
}

fn generate_impl(args: &AccountMetasArgs, program_id: Option<&syn::Expr>) -> TokenStream2 {
    let name = &args.ident;
    let (impl_generics, ty_generics, where_clause) = args.generics.split_for_impl();

    let fields = match &args.data {
        Data::Struct(fields) => fields,
        _ => {
            return syn::Error::new_spanned(name, "AccountMetas only supports structs")
                .to_compile_error();
        }
    };

    // Slot kinds must appear in wire order: required/optional (declared, fixed
    // index), then conditionals (a prefix past the declared list), then the
    // rest. Each of these is a silent wrong answer at runtime rather than a
    // runtime condition, so the compiler says it.
    let kinds: Vec<(&syn::Ident, u8)> = fields
        .iter()
        .map(|f| {
            let k = if f.remaining {
                3
            } else if f.conditional || f.resolved.is_some() {
                2
            } else {
                1
            };
            (f.ident.as_ref().expect("named field"), k)
        })
        .collect();
    if let Some((bad, _)) = kinds.windows(2).find(|w| w[0].1 > w[1].1).map(|w| w[1]) {
        return syn::Error::new_spanned(
            bad,
            "account kinds must be declared in wire order: required/optional \
             (the IDL's fixed slots), then #[account(conditional)], then \
             #[account(remaining)] — a conditional before a required field would \
             shift every slot after it",
        )
        .to_compile_error();
    }
    for f in fields.iter().filter(|f| f.remaining) {
        let ident = f.ident.as_ref().expect("named field");
        if f.reason.as_ref().is_none_or(|r| r.trim().len() < 12) {
            return syn::Error::new_spanned(
                ident,
                "#[account(remaining)] needs reason = \"…\" saying what these accounts \
                 are and why they cannot be named individually. Unmodelled accounts are \
                 an error, and widening the layout to silence one is the failure this \
                 attribute exists to make expensive — identify them first, and reach for \
                 this only when the tail is genuinely a homogeneous list",
            )
            .to_compile_error();
        }
        if PLACEHOLDER_NAMES.contains(&ident.to_string().as_str()) {
            return syn::Error::new_spanned(
                ident,
                "name this field for what it holds, not for how it arrives. \
                 `remaining` is Anchor's word for the mechanism; a consumer reaching \
                 for `remaining[2]` has to know an index nobody wrote down, which is \
                 exactly what modelling the tail was supposed to end",
            )
            .to_compile_error();
        }
    }
    if kinds.iter().filter(|(_, k)| *k == 3).count() > 1 {
        return syn::Error::new_spanned(
            name,
            "only one #[account(remaining)] field: two would make the split ambiguous",
        )
        .to_compile_error();
    }
    let has_remaining = kinds.iter().any(|(_, k)| *k == 3);
    let conditionals: Vec<&syn::Ident> = fields
        .iter()
        .filter(|f| f.conditional)
        .map(|f| f.ident.as_ref().expect("named field"))
        .collect();
    let resolved: Vec<(&syn::Ident, &syn::Expr, bool, bool)> = fields
        .iter()
        .filter_map(|f| {
            f.resolved.as_ref().map(|e| {
                (
                    f.ident.as_ref().expect("named field"),
                    e,
                    f.writable,
                    f.signer,
                )
            })
        })
        .collect();
    if !resolved.is_empty() && !conditionals.is_empty() {
        return syn::Error::new_spanned(
            name,
            "a struct uses either #[account(conditional)] (a positional prefix) or \
             #[account(resolved = …)] (located by derivation), not both: they answer \
             the same question with incompatible rules and mixing them makes the \
             boundary between them undecidable",
        )
        .to_compile_error();
    }
    // A rest whose entries could land at an absent conditional's index is a rest
    // that cannot be read back. Anchor programs that use both always send the
    // conditionals first, so the shape is legal; the *ordering* above is what
    // makes it so, and the sequence check below is what keeps a builder honest.
    let named_count = kinds.iter().filter(|(_, k)| *k == 1).count();

    let field_count = named_count;
    let mut account_metas = Vec::new();
    let mut from_pubkeys_assignments = Vec::new();

    for field in fields.iter().take(named_count) {
        let field_name = field.ident.as_ref().unwrap();
        let is_writable = field.writable;
        let is_signer = field.signer;

        if field.optional {
            let Some(pid) = program_id else {
                return syn::Error::new_spanned(
                    field_name,
                    "#[account(optional)] needs #[accounts(program_id = …)] on the \
                     struct: an absent Anchor-optional account is encoded by putting \
                     the program's own id in its slot, so the id is the encoding",
                )
                .to_compile_error();
            };
            let mk = if is_writable {
                quote! { solana_sdk::instruction::AccountMeta::new(k, #is_signer) }
            } else {
                quote! { solana_sdk::instruction::AccountMeta::new_readonly(k, #is_signer) }
            };
            account_metas.push(quote! {
                match self.#field_name {
                    Some(k) => #mk,
                    // The slot still exists; the program id is the sentinel.
                    None => solana_sdk::instruction::AccountMeta::new_readonly(#pid, false),
                }
            });
            from_pubkeys_assignments.push(quote! {
                // Occupies its slot either way — take one, then read the
                // sentinel out of it.
                let #field_name = match cursor.next().ok_or(
                    ::solana_protocols::parsing::InstructionParseError::NotEnoughAccounts {
                        expected: Self::ACCOUNT_COUNT,
                        actual: total,
                    },
                )? {
                    k if k == #pid => None,
                    k => Some(k),
                };
            });
            continue;
        }

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
            let #field_name = cursor.next().ok_or(
                    ::solana_protocols::parsing::InstructionParseError::NotEnoughAccounts {
                        expected: Self::ACCOUNT_COUNT,
                        actual: total,
                    },
                )?;
        });
    }

    // Conditionals: parsed prefix-wise from the actual count, so a hole cannot
    // be produced here. Built with a check, because a caller sets the fields.
    let mut conditional_metas = Vec::new();
    let mut conditional_assignments = Vec::new();
    let mut sequence_checks = Vec::new();
    for (n, ident) in conditionals.iter().enumerate() {
        conditional_metas.push(quote! {
            if let ::solana_protocols::parsing::accounts::Conditional::Present(k) =
                self.#ident
            {
                metas.push(solana_sdk::instruction::AccountMeta::new(k, false));
            }
        });
        conditional_assignments.push(quote! {
            // A dry cursor stays dry, so a later conditional cannot be present
            // while this one is absent. The prefix rule *is* the iterator.
            let #ident = match cursor.next() {
                Some(k) => ::solana_protocols::parsing::accounts::Conditional::Present(k),
                None => ::solana_protocols::parsing::accounts::Conditional::Absent,
            };
        });
        let earlier: Vec<_> = conditionals[..n].to_vec();
        let this = ident.to_string();
        for e in earlier {
            let e_name = e.to_string();
            sequence_checks.push(quote! {
                if self.#ident.is_present() && !self.#e.is_present() {
                    return Err(::solana_protocols::parsing::accounts::RemainingSequence {
                        absent: #e_name,
                        present: #this,
                    });
                }
            });
        }
    }
    // A rest sits behind every conditional, so the same rule covers it.
    if has_remaining {
        if let Some(last) = conditionals.last() {
            let rest_ident = fields
                .iter()
                .find(|f| f.remaining)
                .and_then(|f| f.ident.as_ref())
                .expect("the remaining field exists");
            let last_name = last.to_string();
            let rest_name = rest_ident.to_string();
            sequence_checks.push(quote! {
                if !self.#rest_ident.is_empty() && !self.#last.is_present() {
                    return Err(::solana_protocols::parsing::accounts::RemainingSequence {
                        absent: #last_name,
                        present: #rest_name,
                    });
                }
            });
        }
    }

    let mut resolved_assignments = Vec::new();
    let mut resolved_metas = Vec::new();
    for (ident, expr, writable, signer) in &resolved {
        let (writable, signer) = (*writable, *signer);
        // The declared privileges, like any other slot. Assuming writable
        // over-declares, and the golden-fixture check compares against the real
        // instruction, which is how that surfaced.
        let mk = if writable {
            quote! { solana_sdk::instruction::AccountMeta::new(k, #signer) }
        } else {
            quote! { solana_sdk::instruction::AccountMeta::new_readonly(k, #signer) }
        };
        resolved_assignments.push(quote! {
            // Derive, then find it. Never an index: the same account is appended
            // at different positions by different callers, so a slot read would
            // be right until it silently was not.
            let #ident = {
                let want: solana_program::pubkey::Pubkey = #expr;
                match tail.iter().position(|k| *k == want) {
                    Some(i) => {
                        tail.remove(i);
                        ::solana_protocols::parsing::accounts::Conditional::Present(want)
                    }
                    None => ::solana_protocols::parsing::accounts::Conditional::Absent,
                }
            };
        });
        resolved_metas.push(quote! {
            if let ::solana_protocols::parsing::accounts::Conditional::Present(k) = self.#ident {
                metas.push(#mk);
            }
        });
    }
    let needs_tail = !resolved.is_empty();

    let all_idents: Vec<&syn::Ident> = fields
        .iter()
        .map(|f| f.ident.as_ref().expect("named field"))
        .collect();

    // Nothing may be left over. An Anchor program reads extra accounts as
    // `ctx.remaining_accounts`, so an account this layout ignores is one the
    // program acted on and we did not record. Before this, a longer list parsed
    // clean and read as "the layout matched".
    let unclaimed_check = if has_remaining {
        // The remaining field took everything; there is nothing left by
        // construction.
        quote!()
    } else if needs_tail {
        quote! {
            if let Some(first) = tail.first() {
                return Err(
                    ::solana_protocols::parsing::InstructionParseError::UnmodelledAccounts {
                        modelled: total - tail.len(),
                        actual: total,
                        first: *first,
                    },
                );
            }
        }
    } else {
        quote! {
            {
                let leftover: Vec<_> = cursor.collect();
                if let Some(first) = leftover.first() {
                    return Err(
                        ::solana_protocols::parsing::InstructionParseError::UnmodelledAccounts {
                            modelled: total - leftover.len(),
                            actual: total,
                            first: *first,
                        },
                    );
                }
            }
        }
    };

    let tail_collect = if needs_tail {
        quote! {
            // Everything past the declared list, as a set to be classified. The
            // declared slots were drained positionally above; from here on
            // position carries no meaning.
            let mut tail: Vec<solana_program::pubkey::Pubkey> = cursor.collect();
        }
    } else {
        quote!()
    };

    let (remaining_meta, remaining_assignment) = if has_remaining {
        // By flag, never by position: conditionals sit between the declared
        // slots and the rest, so `named_count` stopped pointing at it.
        let f = fields
            .iter()
            .find(|f| f.remaining)
            .and_then(|f| f.ident.as_ref())
            .expect("the remaining field exists");
        (
            {
                // The declared privilege for the whole list, defaulting to
                // readonly. A `Vec<Pubkey>` cannot carry per-entry flags and the
                // real ones are mixed — pumpswap's appended buyback vaults
                // arrive readonly and writable in the same instruction. Readonly
                // is the honest default: the golden-fixture check refuses
                // over-declaration, and a builder that needs one writable says
                // so with `#[account(writable, remaining, …)]`.
                let rest_writable = fields
                    .iter()
                    .find(|x| x.remaining)
                    .is_some_and(|x| x.writable);
                let mk = if rest_writable {
                    quote! { solana_sdk::instruction::AccountMeta::new(*k, false) }
                } else {
                    quote! { solana_sdk::instruction::AccountMeta::new_readonly(*k, false) }
                };
                quote! {
                    metas.extend(self.#f.iter().map(|k| #mk));
                }
            },
            // Whatever is left. With resolved fields the tail was already
            // collected and each match removed from it, so the rest is exactly
            // the accounts nothing claimed.
            if needs_tail {
                quote! { let #f = tail; }
            } else {
                quote! { let #f = cursor.collect(); }
            },
        )
    } else {
        (quote!(), quote!())
    };

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Convert to a vector of AccountMeta for instruction building.
            #[must_use]
            pub fn to_account_metas(&self) -> Vec<solana_sdk::instruction::AccountMeta> {
                #[allow(unused_mut)]
                let mut metas = vec![
                    #(#account_metas),*
                ];
                #(#conditional_metas)*
                #(#resolved_metas)*
                #remaining_meta
                metas
            }

            /// The account list, refusing a conditional prefix with a hole.
            ///
            /// [`to_account_metas`](Self::to_account_metas) stays infallible so
            /// the 80-odd existing builders are untouched; this is what a builder
            /// with conditional accounts should call. An absent conditional does
            /// not occupy a slot, so a present one after it would be sent at the
            /// absent one's index — an instruction we did not mean to build.
            ///
            /// # Errors
            ///
            /// [`RemainingSequence`](::solana_protocols::parsing::accounts::RemainingSequence)
            /// naming both accounts.
            pub fn try_to_account_metas(
                &self,
            ) -> ::core::result::Result<
                Vec<solana_sdk::instruction::AccountMeta>,
                ::solana_protocols::parsing::accounts::RemainingSequence,
            > {
                #(#sequence_checks)*
                Ok(self.to_account_metas())
            }

            /// Number of **named** accounts in this struct.
            ///
            /// A minimum, not a total, whenever the struct declares an
            /// `#[account(remaining)]` field: the program may be sent more.
            pub const ACCOUNT_COUNT: usize = #field_count;

            /// Parse from a slice of account pubkeys (parse direction).
            ///
            /// Field order = account order. Each field is assigned the pubkey
            /// at the corresponding index in declaration order.
            pub fn from_pubkeys(
                pubkeys: &[solana_program::pubkey::Pubkey],
            ) -> std::result::Result<Self, ::solana_protocols::parsing::InstructionParseError> {
                // One cursor, drained in declaration order. There is deliberately
                // no index arithmetic here: a computed offset is the bug class a
                // generated layout walk exists to avoid, and it is just as wrong
                // applied to accounts as it was applied to bytes. A field takes
                // the next account; the rest takes what is left; an exhausted
                // cursor is what "absent" means, so nothing needs a bounds guard.
                let total = pubkeys.len();
                let mut cursor = pubkeys.iter().copied();
                #(#from_pubkeys_assignments)*
                #(#conditional_assignments)*
                #tail_collect
                #(#resolved_assignments)*
                #remaining_assignment
                #unclaimed_check
                Ok(Self { #(#all_idents),* })
            }
        }

        impl #impl_generics ::solana_protocols::parsing::FromAccountKeys for #name #ty_generics #where_clause {
            const MIN_ACCOUNTS: usize = #field_count;

            fn from_account_keys(
                keys: &[solana_program::pubkey::Pubkey],
            ) -> std::result::Result<Self, ::solana_protocols::parsing::InstructionParseError> {
                Self::from_pubkeys(keys)
            }
        }
    }
}
