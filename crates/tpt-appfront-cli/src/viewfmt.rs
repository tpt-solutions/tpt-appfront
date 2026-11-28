//! `tpt-appfront fmt --views`: reformats the markup inside `view!` blocks.
//!
//! The formatter changes only element structure and indentation. Attribute
//! values and `{ ... }` children are Rust code, so they are copied verbatim.
//! Strings stay as written.
//!
//! It is conservative. A block it cannot parse with certainty (an unknown
//! construct, a bare attribute, an unbalanced brace) is left byte-for-byte
//! unchanged and counted as skipped. A `view!` that sits on a `//` comment line,
//! such as a doc example, is not touched. Only `view! { ... }` with braces is
//! formatted. The `( )` and `[ ]` forms are left alone.
//!
//! Output rules:
//! * An element whose children are all strings or expressions, on one line
//!   within 100 columns, stays on one line: `<Heading level={1u8}>"Title"</Heading>`.
//! * Otherwise the children go on their own lines, indented four spaces deeper.
//! * An opening tag wider than 100 columns puts each attribute on its own line.

const WIDTH: usize = 100;
const INDENT: usize = 4;

/// One parsed `view!` node.
#[derive(Debug, PartialEq)]
enum Node {
    Element {
        name: String,
        attrs: Vec<Attr>,
        children: Vec<Node>,
        self_closing: bool,
    },
    /// A string literal child, quotes included, as written.
    Text(String),
    /// A `{ ... }` child, braces included, as written.
    Expr(String),
}

/// `name={expr}` or `name="literal"`, with the value kept as written.
#[derive(Debug, PartialEq)]
struct Attr {
    name: String,
    value: String,
}

/// The result of formatting one source file.
#[derive(Debug, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    /// `view!` blocks rewritten (their text may already have been correct).
    pub blocks: usize,
    /// `view!` blocks left unchanged because they could not be parsed.
    pub skipped: usize,
}

/// Formats every `view! { ... }` block in `src`, leaving everything else as it is.
pub fn format_source(src: &str) -> Formatted {
    // Work on LF text, and write the file's dominant line ending back at the end,
    // so a CRLF file is not turned into mixed endings.
    let crlf_dominant = src.matches("\r\n").count() * 2 > src.matches('\n').count();
    let norm = src.replace("\r\n", "\n");
    let c: Vec<char> = norm.chars().collect();
    let mut out = String::with_capacity(norm.len());
    let mut copied = 0;
    let mut blocks = 0;
    let mut skipped = 0;
    let mut i = 0;
    while i < c.len() {
        if starts_with(&c, i, "view!") && (i == 0 || !is_ident(c[i - 1])) {
            let mut j = i + 5;
            while j < c.len() && c[j].is_whitespace() {
                j += 1;
            }
            if c.get(j) == Some(&'{') && !line_is_comment(&c, i) {
                match group_end(&c, j) {
                    Some(end) => {
                        let base = leading_spaces(&c, i);
                        match format_block(&c[j + 1..end - 1], base) {
                            Some(text) => {
                                out.extend(&c[copied..i]);
                                out.push_str(&text);
                                copied = end;
                                blocks += 1;
                            }
                            None => skipped += 1,
                        }
                        i = end;
                        continue;
                    }
                    None => skipped += 1,
                }
            }
        }
        i += 1;
    }
    out.extend(&c[copied..]);
    // Nothing changed: return the original bytes, endings included.
    if out == norm {
        return Formatted {
            text: src.to_string(),
            blocks,
            skipped,
        };
    }
    let text = if crlf_dominant {
        out.replace('\n', "\r\n")
    } else {
        out
    };
    Formatted {
        text,
        blocks,
        skipped,
    }
}

/// Formats the body of one `view!` block (the text between its braces) into
/// `view! { ... }` lines, with the block's line at indentation `base`. `None`
/// when the body cannot be parsed or is empty.
fn format_block(body: &[char], base: usize) -> Option<String> {
    let mut parser = Parser {
        chars: body.to_vec(),
        pos: 0,
    };
    let nodes = parser.parse_nodes(None).ok()?;
    if nodes.is_empty() {
        return None;
    }
    let mut lines = vec!["view! {".to_string()];
    for node in &nodes {
        lines.extend(render_node(node, base + INDENT));
    }
    lines.push(format!("{}}}", spaces(base)));
    Some(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

type ParseResult<T> = Result<T, ()>;

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn ident(&mut self) -> ParseResult<String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_ident(c)) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(());
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }

    /// The children of an element up to its closing tag (`close` is its name),
    /// or the top-level nodes of the block (`close` is `None`).
    fn parse_nodes(&mut self, close: Option<&str>) -> ParseResult<Vec<Node>> {
        let mut nodes = Vec::new();
        loop {
            self.skip_ws();
            let Some(c) = self.peek() else {
                return if close.is_none() { Ok(nodes) } else { Err(()) };
            };
            if starts_with(&self.chars, self.pos, "</") {
                let expected = close.ok_or(())?;
                self.pos += 2;
                self.skip_ws();
                let found = self.ident()?;
                self.skip_ws();
                if found != expected || self.peek() != Some('>') {
                    return Err(());
                }
                self.pos += 1;
                return Ok(nodes);
            }
            match c {
                '<' => nodes.push(self.element()?),
                '"' => {
                    let end = skip_string(&self.chars, self.pos).ok_or(())?;
                    nodes.push(Node::Text(self.slice(self.pos, end)));
                    self.pos = end;
                }
                '{' => {
                    let end = group_end(&self.chars, self.pos).ok_or(())?;
                    nodes.push(Node::Expr(self.slice(self.pos, end)));
                    self.pos = end;
                }
                _ => return Err(()),
            }
        }
    }

    fn element(&mut self) -> ParseResult<Node> {
        self.pos += 1; // '<'
        let name = self.ident()?;
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if starts_with(&self.chars, self.pos, "/>") {
                self.pos += 2;
                return Ok(Node::Element {
                    name,
                    attrs,
                    children: Vec::new(),
                    self_closing: true,
                });
            }
            match self.peek() {
                Some('>') => {
                    self.pos += 1;
                    let children = self.parse_nodes(Some(&name))?;
                    return Ok(Node::Element {
                        name,
                        attrs,
                        children,
                        self_closing: false,
                    });
                }
                Some(c) if c.is_alphabetic() || c == '_' => {
                    let attr_name = self.ident()?;
                    self.skip_ws();
                    if self.peek() != Some('=') {
                        // A bare attribute (no value): not something we format.
                        return Err(());
                    }
                    self.pos += 1;
                    self.skip_ws();
                    let start = self.pos;
                    let end = match self.peek() {
                        Some('{') => group_end(&self.chars, start).ok_or(())?,
                        Some('"') => skip_string(&self.chars, start).ok_or(())?,
                        _ => return Err(()),
                    };
                    attrs.push(Attr {
                        name: attr_name,
                        value: self.slice(start, end),
                    });
                    self.pos = end;
                }
                _ => return Err(()),
            }
        }
    }

    fn slice(&self, start: usize, end: usize) -> String {
        self.chars[start..end].iter().collect()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_node(node: &Node, indent: usize) -> Vec<String> {
    match node {
        Node::Element {
            name,
            attrs,
            children,
            self_closing,
        } => render_element(name, attrs, children, *self_closing, indent),
        Node::Text(raw) | Node::Expr(raw) => {
            let pad = spaces(indent);
            raw.split('\n')
                .enumerate()
                .map(|(n, line)| {
                    if n == 0 {
                        format!("{pad}{line}")
                    } else {
                        line.to_string()
                    }
                })
                .collect()
        }
    }
}

fn render_element(
    name: &str,
    attrs: &[Attr],
    children: &[Node],
    self_closing: bool,
    indent: usize,
) -> Vec<String> {
    let pad = spaces(indent);
    if self_closing {
        return open_tag(name, attrs, true, indent);
    }
    if children.is_empty() {
        let mut lines = open_tag(name, attrs, false, indent);
        let close = format!("</{name}>");
        if lines.len() == 1 {
            lines[0].push_str(&close);
        } else {
            lines.push(format!("{pad}{close}"));
        }
        return lines;
    }

    let inline_children = children.iter().all(|child| match child {
        Node::Element { .. } => false,
        Node::Text(raw) | Node::Expr(raw) => !raw.contains('\n'),
    });
    if inline_children {
        let open = open_tag(name, attrs, false, indent);
        if open.len() == 1 {
            let body: Vec<&str> = children
                .iter()
                .map(|child| match child {
                    Node::Text(raw) | Node::Expr(raw) => raw.as_str(),
                    Node::Element { .. } => unreachable!("checked above"),
                })
                .collect();
            let line = format!("{}{}</{name}>", open[0], body.join(" "));
            if line.chars().count() <= WIDTH {
                return vec![line];
            }
        }
    }

    let mut lines = open_tag(name, attrs, false, indent);
    for child in children {
        lines.extend(render_node(child, indent + INDENT));
    }
    lines.push(format!("{pad}</{name}>"));
    lines
}

/// The opening tag, as one line, or split over several lines when it would be
/// wider than [`WIDTH`] and has more than one attribute.
fn open_tag(name: &str, attrs: &[Attr], self_closing: bool, indent: usize) -> Vec<String> {
    let pad = spaces(indent);
    let closing = if self_closing { " />" } else { ">" };
    let attr_text: Vec<String> = attrs
        .iter()
        .map(|a| format!("{}={}", a.name, a.value))
        .collect();
    let single = if attr_text.is_empty() {
        format!("{pad}<{name}{closing}")
    } else {
        format!("{pad}<{name} {}{closing}", attr_text.join(" "))
    };
    if single.chars().count() <= WIDTH || attrs.len() <= 1 {
        return vec![single];
    }
    let mut lines = vec![format!("{pad}<{name}")];
    for text in attr_text {
        lines.push(format!("{}{text}", spaces(indent + INDENT)));
    }
    lines.push(format!("{pad}{}", closing.trim_start()));
    lines
}

// ---------------------------------------------------------------------------
// Scanning helpers (operate on chars; `i` is an index into `c`)
// ---------------------------------------------------------------------------

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

fn starts_with(c: &[char], i: usize, s: &str) -> bool {
    s.chars()
        .enumerate()
        .all(|(k, ch)| c.get(i + k) == Some(&ch))
}

/// The number of spaces at the start of the line that contains index `i`.
fn leading_spaces(c: &[char], i: usize) -> usize {
    let line_start = line_start(c, i);
    c[line_start..]
        .iter()
        .take_while(|ch| **ch == ' ')
        .count()
}

fn line_start(c: &[char], i: usize) -> usize {
    let mut s = i;
    while s > 0 && c[s - 1] != '\n' {
        s -= 1;
    }
    s
}

/// True when the line holding index `i` starts (after whitespace) with `//`.
fn line_is_comment(c: &[char], i: usize) -> bool {
    let start = line_start(c, i);
    let mut j = start;
    while j < c.len() && (c[j] == ' ' || c[j] == '\t') {
        j += 1;
    }
    starts_with(c, j, "//")
}

/// Index just past the string literal that starts at `i` (a `"`).
fn skip_string(c: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < c.len() {
        match c[j] {
            '\\' => j += 2,
            '"' => return Some(j + 1),
            _ => j += 1,
        }
    }
    None
}

/// Index just past a raw string (`r"..."` or `r#"..."#`) that starts at `i`.
fn skip_raw_string(c: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    let mut hashes = 0;
    while c.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if c.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    while j < c.len() {
        if c[j] == '"' && (0..hashes).all(|k| c.get(j + 1 + k) == Some(&'#')) {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    None
}

/// Index just past a character literal or lifetime that starts at `i` (a `'`).
fn skip_char_literal(c: &[char], i: usize) -> usize {
    if c.get(i + 1) == Some(&'\\') {
        // An escape such as '\n' or '\u{1F600}': find the closing quote.
        let limit = (i + 12).min(c.len());
        match c[(i + 2).min(limit)..limit].iter().position(|&ch| ch == '\'') {
            Some(offset) => i + 2 + offset + 1,
            None => i + 1,
        }
    } else if c.get(i + 2) == Some(&'\'') {
        i + 3
    } else {
        // A lifetime such as 'a: skip only the quote.
        i + 1
    }
}

/// Index just past the `}` that closes the `{` at `start`, skipping strings,
/// raw strings, character literals and comments inside. `None` when unbalanced.
fn group_end(c: &[char], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = start;
    while i < c.len() {
        match c[i] {
            '"' => i = skip_string(c, i)?,
            'r' if !(i > 0 && is_ident(c[i - 1]))
                && matches!(c.get(i + 1), Some('"') | Some('#')) =>
            {
                match skip_raw_string(c, i) {
                    Some(end) => i = end,
                    None => i += 1,
                }
            }
            '/' if c.get(i + 1) == Some(&'/') => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            }
            '/' if c.get(i + 1) == Some(&'*') => {
                let mut j = i + 2;
                while j + 1 < c.len() && !(c[j] == '*' && c[j + 1] == '/') {
                    j += 1;
                }
                i = j + 2;
            }
            '\'' => i = skip_char_literal(c, i),
            '{' => {
                depth += 1;
                i += 1;
            }
            '}' => {
                depth = depth.checked_sub(1)?;
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str) -> String {
        format_source(src).text
    }

    #[test]
    fn reindents_nested_elements_and_keeps_short_leaves_on_one_line() {
        let src = "fn ui() -> UITree<Msg> {\n    view! {\n<Container>\n<Heading level={1u8}>\"Counter\"</Heading>\n<Button on_click={Msg::Increment}>\"+1\"</Button>\n</Container>\n}\n}\n";
        let out = fmt(src);
        let expected = "fn ui() -> UITree<Msg> {\n    view! {\n        <Container>\n            <Heading level={1u8}>\"Counter\"</Heading>\n            <Button on_click={Msg::Increment}>\"+1\"</Button>\n        </Container>\n    }\n}\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn expressions_are_kept_verbatim() {
        let src = "let ui = view! {\n<Text>{ format!(\"{} items }}\", n) }</Text>\n<Button on_click={ move |_| { dispatch(Msg::Go) } }>\"Go\"</Button>\n};";
        let out = fmt(src);
        assert!(out.contains("<Text>{ format!(\"{} items }}\", n) }</Text>"), "{out}");
        assert!(out.contains("on_click={ move |_| { dispatch(Msg::Go) } }"), "{out}");
    }

    #[test]
    fn a_wide_opening_tag_wraps_one_attribute_per_line() {
        let src = "let ui = view! { <Input value={draft.get()} class=\"mb-2 w-full rounded border border-gray-300 px-3 py-2 focus:outline-none focus:ring\" on_input={Msg::SetDraft} /> };";
        let out = fmt(src);
        assert!(out.contains("\n    <Input\n        value={draft.get()}\n"), "{out}");
        assert!(out.contains("\n        on_input={Msg::SetDraft}\n    />"), "{out}");
    }

    #[test]
    fn formatting_is_idempotent() {
        let src = "let ui = view! {\n  <Container><Heading level={1u8}>\"A\"</Heading><List>{ rows() }</List></Container>\n};";
        let once = fmt(src);
        assert_eq!(fmt(&once), once);
    }

    #[test]
    fn unparseable_blocks_are_left_unchanged_and_counted() {
        let src = "let ui = view! {\n  <Input disabled>\"x\"</Input>\n};";
        let formatted = format_source(src);
        assert_eq!(formatted.text, src);
        assert_eq!(formatted.skipped, 1);
        assert_eq!(formatted.blocks, 0);
    }

    #[test]
    fn view_in_a_comment_line_is_not_touched() {
        let src = "//! let ui = view! {\n//!     <Text>\"x\"</Text>\n//! };\nfn f() {}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn non_view_code_is_untouched() {
        let src = "fn f() { let my_view!x = 1; }\nconst S: &str = \"view! { <Text>\";\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn crlf_files_keep_crlf_endings() {
        let src = "let ui = view! {\r\n<Container>\r\n<Text>\"a\"</Text>\r\n</Container>\r\n};\r\nfn f() {}\r\n";
        let out = format_source(src).text;
        assert!(!out.replace("\r\n", "").contains('\n'), "no bare LF: {out:?}");
        assert!(out.contains("    <Container>\r\n        <Text>\"a\"</Text>\r\n    </Container>"), "{out:?}");
    }

    #[test]
    fn already_formatted_crlf_file_is_returned_byte_for_byte() {
        let src = "let ui = view! {\r\n    <Text>\"a\"</Text>\r\n};\r\n";
        let formatted = format_source(src);
        assert_eq!(formatted.text, src);
        assert_eq!(formatted.blocks, 1);
    }

    #[test]
    fn braces_in_strings_and_chars_do_not_unbalance_the_block() {
        let src = "let ui = view! {\n<Text>{ if c == '}' { \"}\" } else { \"{\" } }</Text>\n};";
        let formatted = format_source(src);
        assert_eq!(formatted.blocks, 1, "{}", formatted.text);
        assert!(formatted.text.contains("'}' { \"}\" } else { \"{\" }"), "{}", formatted.text);
    }
}
