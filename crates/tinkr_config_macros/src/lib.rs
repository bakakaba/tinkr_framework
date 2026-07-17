//! Derive macro for `tinkr_config`.
//!
//! Don't depend on this crate directly; `tinkr_config` re-exports
//! [`Configurable`](macro@Configurable).

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Expr, Fields, Ident, LitStr, Type, parse_macro_input};

/// Derives `tinkr_config::Configurable` for a struct with named fields.
///
/// Field behavior is controlled with `#[config(...)]` attributes:
///
/// - `env = "NAME"` — the value can be overridden by the `NAME` environment
///   variable (parsed with [`FromStr`](std::str::FromStr))
/// - `default = <expr>` — value used when neither the environment nor the
///   config file provides one; fields without a default (and that are not
///   `Option`) are required
/// - `nested` — the field is a nested configuration struct (its own
///   `#[derive(Configurable)]`), mapped to a TOML table
/// - `secret` — the value is redacted in the source readout
///
/// Doc comments on the struct and its fields become descriptions in the
/// generated JSON Schema.
#[proc_macro_derive(Configurable, attributes(config))]
pub fn derive_configurable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Everything the expansion needs to know about one field.
struct FieldSpec {
    ident: Ident,
    /// The declared field type.
    ty: Type,
    /// For `Option<T>` fields, `T`; otherwise the field type itself.
    value_ty: Type,
    optional: bool,
    docs: String,
    env: Option<LitStr>,
    default: Option<Expr>,
    nested: bool,
    secret: bool,
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[derive(Configurable)] only supports structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[derive(Configurable)] requires named fields",
        ));
    };

    let base = base_path()?;
    let ident = &input.ident;
    let struct_docs = doc_string(&input.attrs);
    let specs = fields
        .named
        .iter()
        .map(parse_field)
        .collect::<syn::Result<Vec<_>>>()?;

    let layer_ident = format_ident!("__TinkrConfigLayer{ident}");
    // The path serde's generated code uses to reach the serde crate.
    let serde_path = LitStr::new(
        &quote!(#base::__private::serde).to_string().replace(' ', ""),
        Span::call_site(),
    );

    let layer_fields = specs.iter().map(|f| layer_field(f, &base));
    let from_env_fields = specs.iter().map(|f| from_env_field(f, &base));
    let defaults_fields = specs.iter().map(|f| defaults_field(f, &base));
    let merge_fields = specs.iter().map(|f| merge_field(f, &base));
    let schema_props = specs.iter().map(|f| schema_property(f, &base));

    Ok(quote! {
        #[doc(hidden)]
        #[derive(::core::default::Default, #base::__private::serde::Deserialize)]
        #[serde(crate = #serde_path)]
        #[allow(non_camel_case_types)]
        pub struct #layer_ident {
            #(#layer_fields,)*
        }

        #[automatically_derived]
        impl #base::Layer for #layer_ident {
            fn from_env() -> ::core::result::Result<Self, #base::Error> {
                ::core::result::Result::Ok(Self {
                    #(#from_env_fields,)*
                })
            }

            fn defaults() -> Self {
                Self {
                    #(#defaults_fields,)*
                }
            }
        }

        #[automatically_derived]
        impl #base::Configurable for #ident {
            type Layer = #layer_ident;

            fn doc() -> &'static str {
                #struct_docs
            }

            fn schema_node() -> #base::schema::Node {
                #base::schema::Node::object(::std::vec![
                    #(#schema_props,)*
                ])
            }

            fn from_layers(
                env: Self::Layer,
                file: Self::Layer,
                defaults: Self::Layer,
                prefix: &str,
                sources: &mut ::std::vec::Vec<#base::FieldSource>,
            ) -> ::core::result::Result<Self, #base::Error> {
                ::core::result::Result::Ok(Self {
                    #(#merge_fields,)*
                })
            }
        }
    })
}

fn layer_field(f: &FieldSpec, base: &syn::Path) -> proc_macro2::TokenStream {
    let ident = &f.ident;
    if f.nested {
        let ty = &f.ty;
        quote! { #ident: ::core::option::Option<<#ty as #base::Configurable>::Layer> }
    } else {
        let ty = &f.value_ty;
        quote! { #ident: ::core::option::Option<#ty> }
    }
}

fn from_env_field(f: &FieldSpec, base: &syn::Path) -> proc_macro2::TokenStream {
    let ident = &f.ident;
    if f.nested {
        let ty = &f.ty;
        return quote! {
            #ident: ::core::option::Option::Some(
                <<#ty as #base::Configurable>::Layer as #base::Layer>::from_env()?
            )
        };
    }
    match &f.env {
        Some(var) => {
            let ty = &f.value_ty;
            quote! { #ident: #base::__private::env_value::<#ty>(#var)? }
        }
        None => quote! { #ident: ::core::option::Option::None },
    }
}

fn defaults_field(f: &FieldSpec, base: &syn::Path) -> proc_macro2::TokenStream {
    let ident = &f.ident;
    if f.nested {
        let ty = &f.ty;
        return quote! {
            #ident: ::core::option::Option::Some(
                <<#ty as #base::Configurable>::Layer as #base::Layer>::defaults()
            )
        };
    }
    match &f.default {
        Some(expr) => {
            let value = default_value(expr);
            quote! { #ident: ::core::option::Option::Some(#value) }
        }
        None => quote! { #ident: ::core::option::Option::None },
    }
}

/// Adapts a `default = ...` expression to the field type: numeric and bool
/// literals coerce natively, everything else goes through `Into` (so string
/// literals become `String`s).
fn default_value(expr: &Expr) -> proc_macro2::TokenStream {
    fn is_numeric_or_bool(expr: &Expr) -> bool {
        match expr {
            Expr::Lit(lit) => matches!(
                lit.lit,
                syn::Lit::Int(_) | syn::Lit::Float(_) | syn::Lit::Bool(_)
            ),
            // Negative literals parse as unary negation.
            Expr::Unary(unary) => is_numeric_or_bool(&unary.expr),
            _ => false,
        }
    }
    if is_numeric_or_bool(expr) {
        quote! { #expr }
    } else {
        quote! { ::core::convert::Into::into(#expr) }
    }
}

fn merge_field(f: &FieldSpec, base: &syn::Path) -> proc_macro2::TokenStream {
    let ident = &f.ident;
    let name = LitStr::new(&ident.to_string(), ident.span());
    if f.nested {
        let ty = &f.ty;
        return quote! {
            #ident: <#ty as #base::Configurable>::from_layers(
                env.#ident.unwrap_or_default(),
                file.#ident.unwrap_or_default(),
                defaults.#ident.unwrap_or_default(),
                &#base::__private::child_prefix(prefix, #name),
                sources,
            )?
        };
    }
    let env = option_tokens(f.env.as_ref().map(|v| quote! { #v }));
    let secret = f.secret;
    if f.optional {
        quote! {
            #ident: #base::__private::merge_optional(
                env.#ident, file.#ident, defaults.#ident,
                prefix, #name, #env, #secret, sources,
            )
        }
    } else {
        quote! {
            #ident: #base::__private::merge_required(
                env.#ident, file.#ident, defaults.#ident,
                prefix, #name, #env, #secret, sources,
            )?
        }
    }
}

fn schema_property(f: &FieldSpec, base: &syn::Path) -> proc_macro2::TokenStream {
    let name = LitStr::new(&f.ident.to_string(), f.ident.span());
    let description = option_tokens((!f.docs.is_empty()).then(|| {
        let docs = &f.docs;
        quote! { #docs }
    }));
    if f.nested {
        let ty = &f.ty;
        return quote! {
            #base::schema::Property {
                name: #name,
                description: #description,
                required: false,
                default: ::core::option::Option::None,
                env: ::core::option::Option::None,
                node: <#ty as #base::Configurable>::schema_node(),
            }
        };
    }
    let ty = &f.value_ty;
    let env = option_tokens(f.env.as_ref().map(|v| quote! { #v }));
    // A field must appear in the file only when nothing else can provide it.
    let required = f.default.is_none() && !f.optional && f.env.is_none();
    // `default_json` already returns an Option, so no `Some(...)` wrapping.
    let default = match &f.default {
        Some(expr) => {
            let value = default_value(expr);
            quote! {
                #base::__private::default_json(&{
                    let __value: #ty = #value;
                    __value
                })
            }
        }
        None => quote! { ::core::option::Option::None },
    };
    quote! {
        #base::schema::Property {
            name: #name,
            description: #description,
            required: #required,
            default: #default,
            env: #env,
            node: <#ty as #base::schema::ToSchema>::node(),
        }
    }
}

/// Wraps optional tokens in `Some(...)`/`None`.
fn option_tokens(tokens: Option<proc_macro2::TokenStream>) -> proc_macro2::TokenStream {
    match tokens {
        Some(t) => quote! { ::core::option::Option::Some(#t) },
        None => quote! { ::core::option::Option::None },
    }
}

fn parse_field(field: &syn::Field) -> syn::Result<FieldSpec> {
    let ident = field.ident.clone().expect("named field");
    let mut env = None;
    let mut default = None;
    let mut nested = false;
    let mut secret = false;

    for attr in &field.attrs {
        if !attr.path().is_ident("config") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("env") {
                env = Some(meta.value()?.parse::<LitStr>()?);
            } else if meta.path.is_ident("default") {
                default = Some(meta.value()?.parse::<Expr>()?);
            } else if meta.path.is_ident("nested") {
                nested = true;
            } else if meta.path.is_ident("secret") {
                secret = true;
            } else {
                return Err(meta.error(
                    "unsupported #[config(...)] attribute; expected \
                     `env`, `default`, `nested`, or `secret`",
                ));
            }
            Ok(())
        })?;
    }

    let (optional, value_ty) = match option_inner(&field.ty) {
        Some(inner) => (true, inner.clone()),
        None => (false, field.ty.clone()),
    };

    if nested {
        if env.is_some() || default.is_some() || secret {
            return Err(syn::Error::new_spanned(
                field,
                "#[config(nested)] cannot be combined with `env`, `default`, or `secret`",
            ));
        }
        if optional {
            return Err(syn::Error::new_spanned(
                field,
                "#[config(nested)] fields cannot be Option",
            ));
        }
    }
    if optional && default.is_some() {
        return Err(syn::Error::new_spanned(
            field,
            "Option fields cannot have a `default`; drop the Option or the default",
        ));
    }

    Ok(FieldSpec {
        ident,
        ty: field.ty.clone(),
        value_ty,
        optional,
        docs: doc_string(&field.attrs),
        env,
        default,
        nested,
        secret,
    })
}

/// Extracts `T` from an `Option<T>` type, if the field is an `Option`.
fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// Collects `///` doc comment lines into a single trimmed string.
fn doc_string(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(lit) = &nv.value
            && let syn::Lit::Str(s) = &lit.lit
        {
            lines.push(s.value().trim().to_string());
        }
    }
    lines.join("\n").trim().to_string()
}

/// Resolves the path to the `tinkr_config` crate as seen from the deriving
/// crate: directly, or through the `tinkr_framework::config` re-export.
fn base_path() -> syn::Result<syn::Path> {
    use proc_macro_crate::{FoundCrate, crate_name};

    if let Ok(found) = crate_name("tinkr_config") {
        return Ok(match found {
            // In tinkr_config's own integration tests and doctests the crate
            // is an ordinary extern crate.
            FoundCrate::Itself => syn::parse_quote!(::tinkr_config),
            FoundCrate::Name(name) => {
                let ident = Ident::new(&name, Span::call_site());
                syn::parse_quote!(::#ident)
            }
        });
    }
    if let Ok(found) = crate_name("tinkr_framework") {
        return Ok(match found {
            FoundCrate::Itself => syn::parse_quote!(::tinkr_framework::config),
            FoundCrate::Name(name) => {
                let ident = Ident::new(&name, Span::call_site());
                syn::parse_quote!(::#ident::config)
            }
        });
    }
    Err(syn::Error::new(
        Span::call_site(),
        "#[derive(Configurable)] requires a dependency on `tinkr_config` or `tinkr_framework`",
    ))
}
