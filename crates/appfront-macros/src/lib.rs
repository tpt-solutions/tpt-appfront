//! `#[component]` attribute macro for functions returning `UITree<Msg>`.
//!
//! Re-exported by `appfront-core` as `appfront_core::component`, so most
//! users write `#[appfront_core::component]` rather than depending on this
//! crate directly.
//!
//! The macro is a thin, purely-additive wrapper: it does not change what the
//! function computes. It renames the original body into a hidden inner
//! function and generates a public function (same name/signature) that
//! calls it and then fills in two pieces of metadata on the returned root
//! node, but only if the function didn't already set them explicitly:
//!
//! - `meta.class`: defaults to the kebab-case of the function name (e.g.
//!   `fn counter_row(..)` -> `"counter-row"`), giving every component a
//!   stable, human-readable identifier for devtools/debugging without
//!   requiring the author to call `.class(..)` on the root node by hand.
//! - `meta.ai.description`: defaults to the function's doc comment, so the
//!   AI-schema/agent backends (`appfront-ai-schema`, `appfront_core::agent`)
//!   automatically pick up a human-readable description of the component
//!   for LLM consumption without duplicating the doc comment as a
//!   `.ai_description(..)` call.
//!
//! It also does one piece of static analysis: scanning the function body's
//! token stream for signs it reads reactive state (`Signal`, `.get()`,
//! `create_effect`, `route_signal`/`current_route`). If none are found, the
//! component is assumed to render the same tree every time and
//! `meta.is_dynamic` is left `false`; the macro has no type information at
//! expansion time, so this is a token-level heuristic, not a real
//! dataflow analysis — it can't see through helper functions that read a
//! signal internally, and a coincidental method also named `get` will read
//! as dynamic. Treat it as a best-effort hint (e.g. for skipping hydration
//! work on subtrees that never change), not a soundness guarantee.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type};

#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = input;

    if !returns_ui_tree(&sig.output) {
        return Err(syn::Error::new(
            sig.output.span(),
            "#[component] can only be applied to functions returning `UITree<Msg>`",
        ));
    }

    let mut arg_idents = Vec::with_capacity(sig.inputs.len());
    for arg in &sig.inputs {
        match arg {
            FnArg::Receiver(r) => {
                return Err(syn::Error::new(
                    r.span(),
                    "#[component] does not support methods (functions taking `self`)",
                ));
            }
            FnArg::Typed(pat_type) => match &*pat_type.pat {
                Pat::Ident(pat_ident) => arg_idents.push(pat_ident.ident.clone()),
                other => {
                    return Err(syn::Error::new(
                        other.span(),
                        "#[component] requires simple identifier parameters (no destructuring)",
                    ));
                }
            },
        }
    }

    let description = doc_description(&attrs);
    let component_name = kebab_case(&sig.ident.to_string());
    let is_dynamic = body_reads_signal(&quote!(#block));

    let inner_ident = format_ident!("__appfront_component_inner_{}", sig.ident, span = Span::call_site());
    let mut inner_sig = sig.clone();
    inner_sig.ident = inner_ident.clone();

    let description_tokens = match description {
        Some(d) => quote! { Some(::std::string::ToString::to_string(#d)) },
        None => quote! { None },
    };

    Ok(quote! {
        #[doc(hidden)]
        #[allow(clippy::too_many_arguments)]
        #inner_sig #block

        #(#attrs)*
        #vis #sig {
            let mut __appfront_ui = #inner_ident(#(#arg_idents),*);
            if __appfront_ui.meta.class.is_none() {
                __appfront_ui.meta.class = Some(::std::string::ToString::to_string(#component_name));
            }
            if __appfront_ui.meta.ai.description.is_none() {
                __appfront_ui.meta.ai.description = #description_tokens;
            }
            __appfront_ui.meta.is_dynamic = #is_dynamic;
            __appfront_ui
        }
    })
}

/// Token-level heuristic: does this function body mention any of the
/// reactive-system entry points? See the module doc comment for caveats.
fn body_reads_signal(tokens: &TokenStream2) -> bool {
    const MARKERS: &[&str] = &[
        "Signal",
        "get",
        "get_untracked",
        "create_effect",
        "route_signal",
        "current_route",
    ];
    fn walk(tokens: TokenStream2, found: &mut bool) {
        if *found {
            return;
        }
        for tt in tokens {
            match tt {
                TokenTree::Ident(ident) => {
                    if MARKERS.contains(&ident.to_string().as_str()) {
                        *found = true;
                        return;
                    }
                }
                TokenTree::Group(group) => {
                    walk(group.stream(), found);
                }
                TokenTree::Punct(_) | TokenTree::Literal(_) => {}
            }
        }
    }
    let mut found = false;
    walk(tokens.clone(), &mut found);
    found
}

fn returns_ui_tree(output: &ReturnType) -> bool {
    let ty = match output {
        ReturnType::Type(_, ty) => ty,
        ReturnType::Default => return false,
    };
    matches!(
        &**ty,
        Type::Path(p) if p.path.segments.last().is_some_and(|seg| seg.ident == "UITree")
    )
}

fn doc_description(attrs: &[syn::Attribute]) -> Option<String> {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                lines.push(s.value().trim().to_string());
            }
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" ").trim().to_string())
    }
}

fn kebab_case(ident: &str) -> String {
    ident.replace('_', "-")
}
