//! `#[component]` attribute macro for functions returning `UITree<Msg>`.
//!
//! Re-exported by `tpt-appfront-core` as `tpt_appfront_core::component`, so most
//! users write `#[tpt_appfront_core::component]` rather than depending on this
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
//!   AI-schema/agent backends (`tpt-appfront-ai-schema`, `tpt_appfront_core::agent`)
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

mod view;

#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);
    let memo = parse_component_attr(&_attr);
    expand(input, memo)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Parses the `#[component(...)]` attribute arguments. Currently the only
/// recognised argument is `memo` (enable props-equality memoization); any other
/// token is ignored. Returns `true` when memoization is requested.
fn parse_component_attr(attr: &TokenStream) -> bool {
    let tokens: proc_macro2::TokenStream = proc_macro2::TokenStream::from(attr.clone());
    for tt in tokens {
        if let TokenTree::Ident(ident) = tt {
            if ident == "memo" {
                return true;
            }
        }
    }
    false
}

#[proc_macro]
pub fn view(item: TokenStream) -> TokenStream {
    match view::expand(item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Alias of [`view`]; some authors prefer the `rsx!` name (mirroring other
/// Rust UI frameworks). Both expand to identical codegen.
#[proc_macro]
pub fn rsx(item: TokenStream) -> TokenStream {
    match view::expand(item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(input: ItemFn, memo: bool) -> syn::Result<proc_macro2::TokenStream> {
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

    // The body that builds the component's tree and fills in default metadata.
    let build_body = quote! {
        let mut __appfront_ui = #inner_ident(#(#arg_idents),*);
        if __appfront_ui.meta.class.is_none() {
            __appfront_ui.meta.class = Some(::std::string::ToString::to_string(#component_name));
        }
        if __appfront_ui.meta.ai.description.is_none() {
            __appfront_ui.meta.ai.description = #description_tokens;
        }
        __appfront_ui.meta.is_dynamic = #is_dynamic;
        __appfront_ui
    };

    // With `#[component(memo)]`, wrap the build in `tpt_appfront_core::memoize`,
    // keyed on the first argument (the component's props). When the props are
    // `PartialEq` to the previous render's, the cached `UITree` is returned
    // unchanged — the component's subtree is not rebuilt. The props type must
    // therefore be `PartialEq + Clone`; the compiler enforces this via the
    // `memoize` bound when memo is enabled.
    let wrapper_body = if memo {
        let key_ident = arg_idents
            .first()
            .cloned()
            .unwrap_or_else(|| format_ident!("__appfront_unit_key"));
        quote! {
            static __appfront_memo_sentinel: u8 = 0;
            let __appfront_memo_id = (&__appfront_memo_sentinel as *const u8) as u64;
            tpt_appfront_core::memoize(
                __appfront_memo_id,
                #key_ident.clone(),
                move |#key_ident| {
                    let #key_ident = #key_ident.clone();
                    #build_body
                },
            )
        }
    } else {
        build_body
    };

    Ok(quote! {
        #[doc(hidden)]
        #[allow(clippy::too_many_arguments)]
        #inner_sig #block

        #(#attrs)*
        #vis #sig {
            #wrapper_body
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn kebab_case_replaces_underscores() {
        assert_eq!(kebab_case("counter_row"), "counter-row");
        assert_eq!(kebab_case("single"), "single");
    }

    #[test]
    fn doc_description_joins_multiple_lines() {
        let input: ItemFn = syn::parse_quote! {
            /// Line one.
            /// Line two.
            fn f() -> UITree<Msg> { ui }
        };
        assert_eq!(
            doc_description(&input.attrs).as_deref(),
            Some("Line one. Line two.")
        );
    }

    #[test]
    fn doc_description_is_none_without_doc_comment() {
        let input: ItemFn = syn::parse_quote! {
            fn f() -> UITree<Msg> { ui }
        };
        assert_eq!(doc_description(&input.attrs), None);
    }

    #[test]
    fn body_reads_signal_detects_get_call() {
        let tokens = quote! { { count.get().to_string() } };
        assert!(body_reads_signal(&tokens));
    }

    #[test]
    fn body_reads_signal_detects_route_signal() {
        let tokens = quote! { { let r = route_signal(); r } };
        assert!(body_reads_signal(&tokens));
    }

    #[test]
    fn body_reads_signal_false_when_no_markers_present() {
        let tokens = quote! { { "static".to_string() } };
        assert!(!body_reads_signal(&tokens));
    }

    #[test]
    fn body_reads_signal_flags_coincidental_get_method() {
        // Known false-positive: any identifier named `get` trips the
        // heuristic, even one unrelated to `Signal::get`.
        let tokens = quote! { { some_map.get(&key) } };
        assert!(body_reads_signal(&tokens));
    }

    #[test]
    fn returns_ui_tree_true_for_ui_tree_return_type() {
        let sig: syn::Signature = syn::parse_quote! { fn f() -> UITree<Msg> };
        assert!(returns_ui_tree(&sig.output));
    }

    #[test]
    fn returns_ui_tree_false_for_other_return_type() {
        let sig: syn::Signature = syn::parse_quote! { fn f() -> String };
        assert!(!returns_ui_tree(&sig.output));

        let sig_unit: syn::Signature = syn::parse_quote! { fn f() };
        assert!(!returns_ui_tree(&sig_unit.output));
    }
}
