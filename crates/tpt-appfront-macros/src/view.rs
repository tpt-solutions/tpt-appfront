//! `view!` — a small, HTML-like templating macro for building `UITree`s.
//!
//! Scope (per `todo.md` Phase 5 / Phase 14): `Container` / `Heading` / `Text` /
//! `Button` / `Input` / `List` / `DataGrid`. The macro is purely additive — it
//! expands to the same `UITree::container(|c| { ... })` builder calls you'd
//! hand-write, so there's no hidden runtime cost and the resulting tree is
//! identical to one built manually. Attribute values are Rust expressions in
//! `{ ... }` (literals are also accepted as sugar), and text children are
//! either string literals or `{ expr }` expressions.
//!
//! Two-way binding is supported on `<Input>` via the `on_input` attribute,
//! which takes a `Fn(String) -> Msg` (e.g. `on_input={Msg::Set}` or
//! `on_input={|s| Msg::Set(s)}`); the closure is invoked with the input's new
//! value on every change, exactly like `NodeRef::on_input`.
//!
//! ```ignore
//! let ui = tpt_appfront_core::view! {
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
use syn::parse::Parser;
use syn::{Error, Expr, ExprLit, Lit, Pat};

/// Node types the macro understands, with their allowed/required attributes.
const TAGS: &[&str] = &["Container", "Heading", "Text", "Button", "Input", "List", "DataGrid"];

const ALLOWED: &[(&str, &[&str], &[&str])] = &[
    ("Container", &["class", "key"], &[]),
    ("Heading", &["level", "class", "key"], &["level"]),
    ("Text", &["class", "key"], &[]),
    ("Button", &["on_click", "class", "key"], &["on_click"]),
    (
        "Input",
        &["value", "class", "key", "on_input"],
        &["value"],
    ),
    ("List", &["class", "key"], &[]),
    (
        "DataGrid",
        &["columns", "rows", "class", "key"],
        &["columns", "rows"],
    ),
];

enum Child {
    Node(Node),
    /// A literal or `{ expr }` string value, used as the (single) child of a
    /// `Text`/`Heading`/`Button` node.
    Text(Expr),
    /// A `{ expr }` that evaluates to a `UITree<Msg>` used directly as a child
    /// of a `Container`/`List` — the mechanism for composing sub-views and
    /// component functions (e.g. `{ my_component(props) }`).
    NodeExpr(Expr),
    /// `{if ...}` / `{for ...}` control flow producing zero or more children.
    Control(Control),
}

/// Control-flow constructs recognised inside `{ ... }` child blocks.
enum Control {
    If {
        cond: Expr,
        then_children: Vec<Child>,
        else_children: Vec<Child>,
    },
    For {
        pat: Pat,
        iter: Expr,
        body: Vec<Child>,
    },
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
                let text_ctx = matches!(tag.to_string().as_str(), "Heading" | "Text" | "Button");
                let children = parse_children(cur, CloseMode::Tag(&tag), text_ctx)?;
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

/// How a `parse_children` parse should terminate.
enum CloseMode<'a> {
    /// Stop at the matching `</Tag>` (children of a node element).
    Tag(&'a Ident),
    /// Stop at a `}` (children of a `{if}`/`{for}` block body).
    Brace,
}

fn parse_children(
    cur: &mut Cursor,
    close: CloseMode,
    text_context: bool,
) -> Result<Vec<Child>, Error> {
    let mut children = Vec::new();
    loop {
        match cur.peek() {
            None => {
                return Err(match &close {
                    CloseMode::Tag(t) => Error::new(
                        t.span(),
                        format!("missing closing `</{}>`", t),
                    ),
                    CloseMode::Brace => {
                        Error::new(Span::call_site(), "missing closing `}` in control-flow block")
                    }
                })
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '<' => {
                // Closing tag if the next token is `/`.
                if matches!(cur.peek_nth(1), Some(TokenTree::Punct(p2)) if p2.as_char() == '/') {
                    match &close {
                        CloseMode::Tag(t) => {
                            consume_closing(cur, t)?;
                            return Ok(children);
                        }
                        CloseMode::Brace => {
                            return Err(Error::new(
                                p.span(),
                                "unexpected closing tag inside a control-flow block",
                            ))
                        }
                    }
                }
                let node = parse_node(cur)?;
                children.push(Child::Node(node));
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '}' => {
                match &close {
                    CloseMode::Brace => {
                        cur.next();
                        return Ok(children);
                    }
                    CloseMode::Tag(_) => {
                        return Err(Error::new(p.span(), "unexpected `}`"))
                    }
                }
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
                let stream = g.stream();
                // A block whose first token is `if`/`for` is control flow;
                // otherwise it's an interpolation expression (`{ expr }`).
                let first = stream.clone().into_iter().next();
                if let Some(TokenTree::Ident(kw)) = &first {
                    if kw == "if" || kw == "for" {
                        let ctrl = parse_control(stream, g.span())?;
                        children.push(Child::Control(ctrl));
                        continue;
                    }
                }
                let expr = syn::parse2(stream)
                    .map_err(|e| Error::new(g.span(), format!("invalid expression: {e}")))?;
                if text_context {
                    children.push(Child::Text(expr));
                } else {
                    children.push(Child::NodeExpr(expr));
                }
            }
            Some(other) => {
                return Err(Error::new(
                    other.span(),
                    "expected a child node (`<Tag>`), control flow (`{if}`/`{for}`), or text (`\"...\"` / `{ expr }`)",
                ));
            }
        }
    }
}

/// Parses a `{if cond { ... } [else { ... } | else if ...]}` or
/// `{for pat in iter { ... }}` block into a [`Control`] tree.
fn parse_control(stream: TokenStream, span: Span) -> Result<Control, Error> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut cur = Cursor { toks: &toks, pos: 0 };

    let first = cur
        .peek()
        .ok_or_else(|| Error::new(span, "empty control-flow block"))?;
    let kw = match first {
        TokenTree::Ident(i) => i.to_string(),
        _ => return Err(Error::new(span, "control-flow block must start with `if` or `for`")),
    };
    cur.next();

    match kw.as_str() {
        "if" => {
            // Condition = tokens up to the first `{ ... }` block.
            let mut cond_toks: Vec<TokenTree> = Vec::new();
            let then_block = loop {
                match cur.next() {
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => break g,
                    Some(t) => cond_toks.push(t),
                    None => return Err(Error::new(span, "expected `{` block after `if` condition")),
                }
            };
            let cond: Expr = syn::parse2(cond_toks.into_iter().collect())
                .map_err(|e| Error::new(span, format!("invalid `if` condition: {e}")))?;
            let then_children = parse_block_children(&then_block, span)?;

            let else_children = if matches!(cur.peek(), Some(TokenTree::Ident(i)) if i == "else") {
                cur.next();
                match cur.peek() {
                    Some(TokenTree::Ident(i)) if i == "if" => {
                        // `else if` — parse the remaining tokens as a nested
                        // `if` and wrap it as the sole else child.
                        let rest: TokenStream = cur.toks[cur.pos..].iter().cloned().collect();
                        vec![Child::Control(parse_control(rest, span)?)]
                    }
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                        let g = g.clone();
                        cur.next();
                        parse_block_children(&g, span)?
                    }
                    _ => {
                        return Err(Error::new(
                            span,
                            "`else` must be followed by `{ ... }` or `if ...`",
                        ))
                    }
                }
            } else {
                Vec::new()
            };

            Ok(Control::If {
                cond,
                then_children,
                else_children,
            })
        }
        "for" => {
            // Pattern = tokens up to the `in` keyword.
            let mut pat_toks: Vec<TokenTree> = Vec::new();
            loop {
                match cur.peek() {
                    Some(TokenTree::Ident(i)) if i == "in" => {
                        cur.next();
                        break;
                    }
                    Some(_) => pat_toks.push(cur.next().unwrap()),
                    None => return Err(Error::new(span, "expected `in` in `for` loop")),
                }
            }
            let pat: Pat = syn::Pat::parse_single
                .parse2(pat_toks.into_iter().collect())
                .map_err(|e| Error::new(span, format!("invalid `for` pattern: {e}")))?;

            // Iterator = tokens up to the `{ ... }` block.
            let mut iter_toks: Vec<TokenTree> = Vec::new();
            let body_block = loop {
                match cur.next() {
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => break g,
                    Some(t) => iter_toks.push(t),
                    None => return Err(Error::new(span, "expected `{` block after `for` iterator")),
                }
            };
            let iter: Expr = syn::parse2(iter_toks.into_iter().collect())
                .map_err(|e| Error::new(span, format!("invalid `for` iterator: {e}")))?;
            let body = parse_block_children(&body_block, span)?;

            Ok(Control::For { pat, iter, body })
        }
        other => Err(Error::new(span, format!("unknown control-flow keyword `{other}`"))),
    }
}

/// Parses the children of a `{ ... }` control-flow body block.
fn parse_block_children(group: &proc_macro2::Group, span: Span) -> Result<Vec<Child>, Error> {
    let bt: Vec<TokenTree> = group.stream().into_iter().collect();
    let mut cur = Cursor { toks: &bt, pos: 0 };
    parse_children(&mut cur, CloseMode::Brace, false)
        .map_err(|e| Error::new(span, format!("in control-flow block: {e}")))
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
        _other => Err(Error::new(
            node.tag.span(),
            format!(
                "`<{}>` text must be a string literal or `{{ expr }}`, not a child node or control flow",
                node.tag
            ),
        )),
    }
}

/// A node is *static* when every text/attribute value is a literal (a string
/// or numeric literal, not a `{ expr }` interpolation) **and** every child is
/// itself static. Static subtrees never change between renders, so the codegen
/// can build them once and clone the cached instance — the "compile-time
/// codegen for static UITree subtrees" differentiator (see `todo.md` Phase 5).
fn node_is_static(node: &Node) -> bool {
    let values_static = node.attrs.iter().all(|(_, e)| is_literal_expr(e))
        && node.children.iter().all(|c| match c {
            Child::Text(e) => is_literal_expr(e),
            Child::Node(_) => true,
            // A runtime-built node or control-flow block is, by definition,
            // not statically known.
            Child::NodeExpr(_) => false,
            Child::Control(_) => false,
        });
    if !values_static {
        return false;
    }
    node.children.iter().all(|c| match c {
        Child::Node(n) => node_is_static(n),
        Child::Text(_) | Child::NodeExpr(_) | Child::Control(_) => true,
    })
}

/// A literal expression is one the macro can prove is constant at compile time:
/// a string literal or a numeric/char literal (anything in `Expr::Lit` whose
/// inner `Lit` is not a `bool`/negative-number ambiguity issue). Brace-group
/// expressions (`{ count.get() }`) are dynamic and return `false`.
fn is_literal_expr(e: &Expr) -> bool {
    matches!(e, Expr::Lit(_))
}

/// Validates attributes/children and emits a statement that builds this node
/// as a child of `parent` (a `&mut ContainerBuilder<Msg>`), including any
/// trailing `.class(..)`/`.key(..)` chain.
///
/// When the node is provably static (no `{ expr }` interpolation anywhere in it
/// or its subtree) and its parent is *not* already part of a cached static
/// subtree, it's built once via [`tpt_appfront_core::static_tree::static_node`] and
/// appended with [`tpt_appfront_core::ContainerBuilder::with`] — the
/// "compile-time codegen for static UITree subtrees" differentiator. Otherwise
/// the normal runtime builder calls are emitted, and `parent_static` is
/// threaded into the children so a dynamic container can still have
/// individually-cached static children.
fn gen_node_stmt(
    node: &Node,
    parent: &Ident,
    parent_static: bool,
    id: &mut usize,
) -> Result<TokenStream, Error> {
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

    // A static node under a non-static parent is hoisted into a per-node
    // cached `UITree` (built exactly once, cloned thereafter). Inside an
    // already-cached subtree we keep emitting plain builder calls — the parent
    // cache already covers the whole thing, so nesting caches is redundant.
    if node_is_static(node) && !parent_static {
        let sentinel = format_ident!("__appfront_static_{}", *id);
        *id += 1;
        let inner_parent = format_ident!("__appfront_b_{}", *id);
        *id += 1;
        // Recurse with `parent_static = true` so the inner subtree is built
        // inline (cached wholesale by this outer `static_node` call).
        let inner = gen_node_stmt(node, &inner_parent, true, id)?;
        return Ok(quote! {
            #parent.with({
                static #sentinel: u8 = 0;
                let __appfront_id = (&#sentinel as *const u8) as u64;
                tpt_appfront_core::static_tree::static_node(__appfront_id, || {
                    let mut #inner_parent = tpt_appfront_core::ContainerBuilder::new();
                    #inner
                    #inner_parent
                        .into_only_child()
                        .expect("static subtree must yield exactly one node")
                })#chain
            });
        });
    }

    match tag.as_str() {
        "Container" => {
            let child_param = format_ident!("__c{}", *id);
            *id += 1;
            let inner = gen_children(&node.children, &child_param, parent_static, id)?;
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
        "List" => {
            // `<List>` children are built onto the inner `ContainerBuilder`
            // that `c.list(...)` passes in — same as a nested `Container`, but
            // the node kind becomes `NodeKind::List` (rendered as `<ul>`/`<table>`
            // rows by backends that support it).
            let child_param = format_ident!("__c{}", *id);
            *id += 1;
            let inner = gen_children(&node.children, &child_param, parent_static, id)?;
            let body = if inner.is_empty() {
                quote! { let _ = & #child_param; }
            } else {
                quote! { #(#inner)* }
            };
            Ok(quote! {
                #parent.list(|#child_param| { #body })#chain;
            })
        }
        "DataGrid" => {
            let columns = attr_expr(node, "columns").unwrap();
            let rows = attr_expr(node, "rows").unwrap();
            if !node.children.is_empty() {
                return Err(Error::new(
                    node.tag.span(),
                    "`<DataGrid>` is self-closing and must not have children — supply `columns` and `rows` as `{ expr }`s",
                ));
            }
            Ok(quote! { #parent.data_grid(#columns, #rows)#chain; })
        }
        other => Err(Error::new(
            node.tag.span(),
            format!("unknown tag `<{}>`", other),
        )),
    }
}

/// Emits `.class(..)`/`.key(..)`/`.on_input(..)` for any of those attributes
/// present on a node. `.on_input` only appears on `<Input>` (it's not in any
/// other tag's allowed-attribute list), so it's only ever emitted there.
fn chain_suffix(attrs: &[(Ident, Expr)]) -> TokenStream {
    let mut out = TokenStream::new();
    for (name, expr) in attrs {
        match name.to_string().as_str() {
            "class" => out.extend(quote! { .class(#expr) }),
            "key" => out.extend(quote! { .key(#expr) }),
            "on_input" => out.extend(quote! { .on_input(#expr) }),
            _ => {}
        }
    }
    out
}

fn gen_children(
    children: &[Child],
    parent: &Ident,
    parent_static: bool,
    id: &mut usize,
) -> Result<Vec<TokenStream>, Error> {
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        match child {
            Child::Node(n) => out.push(gen_node_stmt(n, parent, parent_static, id)?),
            Child::Text(e) => out.push(quote! { #parent.text(#e); }),
            Child::NodeExpr(e) => out.push(quote! { #parent.with(#e); }),
            Child::Control(ctrl) => match ctrl {
                Control::If {
                    cond,
                    then_children,
                    else_children,
                } => {
                    let then_stmts = gen_children(then_children, parent, parent_static, id)?;
                    let else_stmts = gen_children(else_children, parent, parent_static, id)?;
                    out.push(quote! {
                        if (#cond) {
                            #(#then_stmts)*
                        } else {
                            #(#else_stmts)*
                        }
                    });
                }
                Control::For { pat, iter, body } => {
                    let body_stmts = gen_children(body, parent, parent_static, id)?;
                    out.push(quote! {
                        for #pat in (#iter) {
                            #(#body_stmts)*
                        }
                    });
                }
            },
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

    let root_is_static = node_is_static(&root);

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
    // `is_dynamic` is now set precisely by the macro (it was a heuristic flag
    // before): `false` when the entire `view!` is purely static, `true` when
    // any interpolation exists. Backends read it to skip hydration/listener
    // work on inert subtrees, and the codegen above consumes it to hoist
    // static subtrees into cached `UITree`s.
    let is_dynamic_lit = !root_is_static;
    root_stmts.extend(quote! { __ui.meta.is_dynamic = #is_dynamic_lit; });

    let mut id = 0usize;

    if root_is_static {
        let sentinel = format_ident!("__appfront_static_root_{}", id);
        id += 1;
        let root_param = format_ident!("__c{}", id);
        id += 1;
        let inner = gen_children(&root.children, &root_param, true, &mut id)?;
        let body = if inner.is_empty() {
            quote! { let _ = & #root_param; }
        } else {
            quote! { #(#inner)* }
        };
        return Ok(quote! {
            {
                static #sentinel: u8 = 0;
                let __appfront_id = (&#sentinel as *const u8) as u64;
                let mut __ui = tpt_appfront_core::static_tree::static_node(__appfront_id, || {
                    tpt_appfront_core::UITree::container(|#root_param| { #body })
                });
                #root_stmts
                __ui
            }
        });
    }

    let root_param = format_ident!("__c{}", id);
    id += 1;
    let inner = gen_children(&root.children, &root_param, false, &mut id)?;
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
