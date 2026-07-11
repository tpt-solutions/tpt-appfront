//! `view!` — a small, HTML-like templating macro for building `UITree`s.
//!
//! v1 scope (per `todo.md` Phase 5): `Container` / `Heading` / `Text` /
//! `Button` / `Input`. The macro is purely additive — it expands to the same
//! `UITree::container(|c| { ... })` builder calls you'd hand-write, so there's
//! no hidden runtime cost and the resulting tree is identical to one built
//! manually. Attribute values are Rust expressions in `{ ... }` (literals are
//! also accepted as sugar), and text children are either string literals or
//! `{ expr }` expressions.
//!
//! ```ignore
//! let ui = appfront_core::view! {
//!     <Container>
//!         <Heading level={1u8}>"Counter"</Heading>
//!         <Text>{ format!("Count: {}", count.get()) }</Text>
//!         <Button on_click={Msg::Increment}>"+1"</Button>
//!         <Input value={count.get().to_string()} />
//!     </Container>
//! };
//! ```
//!
//! Errors (unknown tag, missing required attribute, wrong child shape)
//! surface at the offending token's span rather than deep inside generated
//! code, which is the "good-enough compile error spans" goal for v1 — full
//! parity with Leptos/Dioxus error reporting is out of scope for day one.

use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{Error, Expr, ExprLit, Lit};

/// Node types the v1 macro understands, with their allowed/required attributes.
const TAGS: &[&str] = &["Container", "Heading", "Text", "Button", "Input"];

const ALLOWED: &[(&str, &[&str], &[&str])] = &[
    ("Container", &["class", "key"], &[]),
    ("Heading", &["level", "class", "key"], &["level"]),
    ("Text", &["class", "key"], &[]),
    ("Button", &["on_click", "class", "key"], &["on_click"]),
    ("Input", &["value", "class", "key"], &["value"]),
];

enum Child {
    Node(Node),
    Text(Expr),
}

struct Node {
    tag: Ident,
    attrs: Vec<(Ident, Expr)>,
    children: Vec<Child>,
    self_closing: bool,
}

struct Cursor<'a> {
    toks: &'a [TokenTree],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&TokenTree> {
        self.toks.get(self.pos)
    }
    fn peek_nth(&self, n: usize) -> Option<&TokenTree> {
        self.toks.get(self.pos + n)
    }
    fn next(&mut self) -> Option<TokenTree> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }
}

fn is_punct(t: &TokenTree, c: char) -> bool {
    matches!(t, TokenTree::Punct(p) if p.as_char() == c)
}

fn expect_gt(cur: &mut Cursor) -> Result<(), Error> {
    let t = cur
        .next()
        .ok_or_else(|| Error::new(Span::call_site(), "expected `>`"))?;
    if !is_punct(&t, '>') {
        return Err(Error::new(t.span(), "expected `>`"));
    }
    Ok(())
}

fn parse_attr(cur: &mut Cursor) -> Result<(Ident, Expr), Error> {
    let name = match cur.next() {
        Some(TokenTree::Ident(i)) => i,
        Some(other) => return Err(Error::new(other.span(), "expected attribute name")),
        None => return Err(Error::new(Span::call_site(), "expected attribute name")),
    };
    let eq = cur
        .next()
        .ok_or_else(|| Error::new(name.span(), "expected `=` after attribute name"))?;
    if !is_punct(&eq, '=') {
        return Err(Error::new(eq.span(), "expected `=` after attribute name"));
    }
    let value_tok = cur
        .next()
        .ok_or_else(|| Error::new(name.span(), "expected attribute value"))?;
    let expr = parse_value_expr(value_tok)?;
    Ok((name, expr))
}

fn parse_value_expr(tok: TokenTree) -> Result<Expr, Error> {
    match tok {
        TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
            syn::parse2(g.stream()).map_err(|e| Error::new(g.span(), e))
        }
        TokenTree::Literal(l) => Ok(Expr::Lit(ExprLit {
            attrs: vec![],
            lit: Lit::new(l),
        })),
        other => Err(Error::new(
            other.span(),
            "attribute value must be `{ expr }` or a literal (e.g. `level={1u8}` or `class=\"x\"`)",
        )),
    }
}

fn parse_node(cur: &mut Cursor) -> Result<Node, Error> {
    let lt = cur
        .next()
        .ok_or_else(|| Error::new(Span::call_site(), "expected `<Tag ...>`"))?;
    if !is_punct(&lt, '<') {
        return Err(Error::new(lt.span(), "expected `<Tag ...>`"));
    }
    let tag = match cur.next() {
        Some(TokenTree::Ident(i)) => i,
        Some(other) => return Err(Error::new(other.span(), "expected tag name")),
        None => return Err(Error::new(Span::call_site(), "unexpected end of input")),
    };
    if !TAGS.contains(&tag.to_string().as_str()) {
        return Err(Error::new(
            tag.span(),
            format!("unknown tag `<{}>`; v1 supports: {}", tag, TAGS.join(", ")),
        ));
    }

    let mut attrs = Vec::new();
    loop {
        match cur.peek() {
            Some(TokenTree::Punct(p)) if p.as_char() == '/' => {
                cur.next();
                expect_gt(cur)?;
                return Ok(Node {
                    tag,
                    attrs,
                    children: vec![],
                    self_closing: true,
                });
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '>' => {
                cur.next();
                let children = parse_children(cur, &tag)?;
                return Ok(Node {
                    tag,
                    attrs,
                    children,
                    self_closing: false,
                });
            }
            Some(TokenTree::Ident(_)) => {
                let (name, value) = parse_attr(cur)?;
                attrs.push((name, value));
            }
            Some(other) => {
                return Err(Error::new(other.span(), "unexpected token inside tag"));
            }
            None => return Err(Error::new(Span::call_site(), "unexpected end of input in tag")),
        }
    }
}

fn parse_children(cur: &mut Cursor, close_tag: &Ident) -> Result<Vec<Child>, Error> {
    let mut children = Vec::new();
    loop {
        match cur.peek() {
            None => {
                return Err(Error::new(
                    close_tag.span(),
                    format!("missing closing `</{}>`", close_tag),
                ))
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '<' => {
                // Closing tag if the next token is `/`.
                if matches!(cur.peek_nth(1), Some(TokenTree::Punct(p2)) if p2.as_char() == '/') {
                    consume_closing(cur, close_tag)?;
                    return Ok(children);
                }
                let node = parse_node(cur)?;
                children.push(Child::Node(node));
            }
            Some(TokenTree::Literal(l)) => {
                let l = l.clone();
                cur.next();
                let lit = Lit::new(l.clone());
                let s = match &lit {
                    Lit::Str(s) => s.value(),
                    other => {
                        return Err(Error::new(other.span(), "expected a string literal as text"));
                    }
                };
                let lit_str = syn::LitStr::new(&s, l.span());
                let expr = Expr::Lit(ExprLit {
                    attrs: vec![],
                    lit: Lit::Str(lit_str),
                });
                children.push(Child::Text(expr));
            }
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                let g = g.clone();
                cur.next();
                let expr = syn::parse2(g.stream())
                    .map_err(|e| Error::new(g.span(), format!("invalid text expression: {e}")))?;
                children.push(Child::Text(expr));
            }
            Some(other) => {
                return Err(Error::new(
                    other.span(),
                    "expected a child node (`<Tag>`) or text (`\"...\"` / `{ expr }`)",
                ));
            }
        }
    }
}

fn consume_closing(cur: &mut Cursor, close_tag: &Ident) -> Result<(), Error> {
    cur.next(); // '<'
    cur.next(); // '/'
    let name = match cur.next() {
        Some(TokenTree::Ident(i)) => i,
        Some(o) => return Err(Error::new(o.span(), "expected closing tag name")),
        None => return Err(Error::new(close_tag.span(), "expected closing tag")),
    };
    if name != *close_tag {
        return Err(Error::new(
            name.span(),
            format!("mismatched closing tag: expected `</{}>`", close_tag),
        ));
    }
    expect_gt(cur)
}

// ---------------------------------------------------------------------------
// Codegen
// ---------------------------------------------------------------------------

fn allowed_for(tag: &str) -> &'static [&'static str] {
    ALLOWED.iter().find(|(t, _, _)| *t == tag).map(|(_, a, _)| *a).unwrap_or(&[])
}

fn required_for(tag: &str) -> &'static [&'static str] {
    ALLOWED
        .iter()
        .find(|(t, _, _)| *t == tag)
        .map(|(_, _, r)| *r)
        .unwrap_or(&[])
}

fn attr_expr<'a>(node: &'a Node, name: &str) -> Option<&'a Expr> {
    node.attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, e)| e)
}

fn single_text(node: &Node) -> Result<&Expr, Error> {
    if node.self_closing || node.children.len() != 1 {
        return Err(Error::new(
            node.tag.span(),
            format!(
                "`<{}>` must have exactly one text child (a string literal or `{{ expr }}`)",
                node.tag
            ),
        ));
    }
    match &node.children[0] {
        Child::Text(e) => Ok(e),
        Child::Node(_) => Err(Error::new(
            node.tag.span(),
            format!("`<{}>` text must be a string literal or `{{ expr }}`, not a child node", node.tag),
        )),
    }
}

/// Validates attributes/children and emits a statement that builds this node
/// as a child of `parent` (a `&mut ContainerBuilder<Msg>`), including any
/// trailing `.class(..)`/`.key(..)` chain.
fn gen_node_stmt(node: &Node, parent: &Ident, id: &mut usize) -> Result<TokenStream, Error> {
    let tag = node.tag.to_string();

    // Validate attributes are allowed for this tag.
    for (name, _) in &node.attrs {
        let allowed = allowed_for(&tag);
        if !allowed.contains(&name.to_string().as_str()) {
            return Err(Error::new(
                name.span(),
                format!("`<{}>` does not accept a `{name}` attribute", tag),
            ));
        }
    }
    // Validate required attributes are present.
    for req in required_for(&tag) {
        if attr_expr(node, req).is_none() {
            return Err(Error::new(
                node.tag.span(),
                format!("`<{}>` requires a `{req}` attribute", tag),
            ));
        }
    }

    let chain = chain_suffix(&node.attrs);

    match tag.as_str() {
        "Container" => {
            let child_param = format_ident!("__c{}", *id);
            *id += 1;
            let inner = gen_children(&node.children, &child_param, id)?;
            let body = if inner.is_empty() {
                quote! { let _ = & #child_param; }
            } else {
                quote! { #(#inner)* }
            };
            Ok(quote! {
                #parent.container(|#child_param| { #body })#chain;
            })
        }
        "Heading" => {
            let level = attr_expr(node, "level").unwrap();
            let text = single_text(node)?;
            Ok(quote! { #parent.heading((#level) as u8, #text)#chain; })
        }
        "Text" => {
            let text = single_text(node)?;
            Ok(quote! { #parent.text(#text)#chain; })
        }
        "Button" => {
            let on_click = attr_expr(node, "on_click").unwrap();
            let label = single_text(node)?;
            Ok(quote! { #parent.button(#label).on_click(#on_click)#chain; })
        }
        "Input" => {
            let value = attr_expr(node, "value").unwrap();
            if !node.children.is_empty() {
                return Err(Error::new(
                    node.tag.span(),
                    "`<Input>` is self-closing and must not have children",
                ));
            }
            Ok(quote! { #parent.input(#value)#chain; })
        }
        other => Err(Error::new(
            node.tag.span(),
            format!("unknown tag `<{}>`", other),
        )),
    }
}

/// Emits `.class(..)`/`.key(..)` for any of those attributes present on a node.
fn chain_suffix(attrs: &[(Ident, Expr)]) -> TokenStream {
    let mut out = TokenStream::new();
    for (name, expr) in attrs {
        match name.to_string().as_str() {
            "class" => out.extend(quote! { .class(#expr) }),
            "key" => out.extend(quote! { .key(#expr) }),
            _ => {}
        }
    }
    out
}

fn gen_children(
    children: &[Child],
    parent: &Ident,
    id: &mut usize,
) -> Result<Vec<TokenStream>, Error> {
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        match child {
            Child::Node(n) => out.push(gen_node_stmt(n, parent, id)?),
            Child::Text(e) => out.push(quote! { #parent.text(#e); }),
        }
    }
    Ok(out)
}

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let toks: Vec<TokenTree> = input.into_iter().collect();
    let mut cur = Cursor { toks: &toks, pos: 0 };

    let root = parse_node(&mut cur)?;
    if !cur.at_end() {
        return Err(Error::new(
            Span::call_site(),
            "unexpected tokens after the root node",
        ));
    }
    if root.tag != "Container" {
        return Err(Error::new(
            root.tag.span(),
            "view! root must be a single `<Container>` element",
        ));
    }

    // Root attributes land on the root node's meta (class/key). Matches
    // ContainerBuilder's `impl Into<String>` acceptance on non-root nodes
    // (see `chain_suffix`), so e.g. `class={"page"}` (a `&str`) works here too.
    let mut root_stmts = TokenStream::new();
    if let Some(e) = attr_expr(&root, "class") {
        root_stmts.extend(quote! { __ui.meta.class = Some(::std::string::ToString::to_string(&(#e))); });
    }
    if let Some(e) = attr_expr(&root, "key") {
        root_stmts.extend(quote! { __ui.meta.key = Some(::std::string::ToString::to_string(&(#e))); });
    }

    let mut id = 0usize;
    let root_param = format_ident!("__c{}", id);
    id += 1;
    let inner = gen_children(&root.children, &root_param, &mut id)?;
    let body = if inner.is_empty() {
        quote! { let _ = & #root_param; }
    } else {
        quote! { #(#inner)* }
    };

    Ok(quote! {
        {
            let mut __ui = UITree::container(|#root_param| { #body });
            #root_stmts
            __ui
        }
    })
}
