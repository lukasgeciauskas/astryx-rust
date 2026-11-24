//! The typed token surface: one Rust field per shared token, named by one
//! rule (the Swift generator's G6 rule, in snake_case).
//!
//! | Token | Field |
//! |---|---|
//! | `--color-text-primary` | `color.text_primary` |
//! | `--spacing-0-5` | `spacing.s0_5` |
//! | `--size-element-md` | `size.element_md` |
//! | `--font-size-2xl` | `font.size.s2xl` |
//! | `--text-heading-1-size` | `text.heading_1.size` |
//!
//! Private tokens (`--_*`) have no field.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use super::literal;

pub enum Node {
    Group(BTreeMap<String, Node>),
    Leaf { token: String, kind: String },
}

const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// A field name: `-` becomes `_`, a leading digit takes `s`, a keyword
/// becomes a raw identifier.
pub fn ident(part: &str) -> String {
    let snake = part.replace('-', "_");
    let snake = if snake.starts_with(|c: char| c.is_ascii_digit()) {
        format!("s{snake}")
    } else {
        snake
    };
    if KEYWORDS.contains(&snake.as_str()) {
        format!("r#{snake}")
    } else {
        snake
    }
}

/// The field path of a shared token, or `None` for a private one.
pub fn token_path(token: &str) -> Result<Option<Vec<String>>> {
    let name = token.strip_prefix("--").context("token without --")?;
    if name.starts_with('_') {
        return Ok(None);
    }
    let path = if let Some(rest) = name.strip_prefix("font-family-") {
        vec!["font".into(), "family".into(), ident(rest)]
    } else if let Some(rest) = name.strip_prefix("font-size-") {
        vec!["font".into(), "size".into(), ident(rest)]
    } else if let Some(rest) = name.strip_prefix("font-weight-") {
        vec!["font".into(), "weight".into(), ident(rest)]
    } else if let Some(rest) = name.strip_prefix("text-") {
        let (style, part) = rest.rsplit_once('-').context("text token without a part")?;
        if !matches!(part, "size" | "weight" | "leading") {
            bail!("text token {token} has part {part}");
        }
        vec!["text".into(), ident(style), part.into()]
    } else if let Some(rest) = name.strip_prefix("spacing-") {
        vec!["spacing".into(), format!("s{}", rest.replace('-', "_"))]
    } else {
        let (group, rest) = name.split_once('-').context("token without a group")?;
        match group {
            "color" | "size" | "border" | "focus" | "radius" | "shadow" | "duration" | "ease" => {
                vec![group.into(), ident(rest)]
            }
            _ => bail!("token {token} is in no known group"),
        }
    };
    Ok(Some(path))
}

/// Builds the surface from core's default tokens in one mode.
pub fn surface(tokens: &Map<String, Value>) -> Result<Node> {
    let mut root = BTreeMap::new();
    for (token, value) in tokens {
        let Some(path) = token_path(token)? else {
            continue;
        };
        let kind = literal::kind(value)?.to_string();
        insert(
            &mut root,
            &path,
            Node::Leaf {
                token: token.clone(),
                kind,
            },
        )
        .with_context(|| format!("placing {token}"))?;
    }
    if let Some(Node::Group(text)) = root.get("text") {
        for (style, node) in text {
            let Node::Group(parts) = node else {
                bail!("text.{style} is not a style")
            };
            for part in ["size", "weight", "leading"] {
                if !parts.contains_key(part) {
                    bail!("text style {style} has no {part}");
                }
            }
        }
    }
    Ok(Node::Group(root))
}

fn insert(group: &mut BTreeMap<String, Node>, path: &[String], leaf: Node) -> Result<()> {
    let (head, rest) = path.split_first().context("empty path")?;
    if rest.is_empty() {
        if group.insert(head.clone(), leaf).is_some() {
            bail!("two tokens map to field {head}");
        }
        return Ok(());
    }
    let child = group
        .entry(head.clone())
        .or_insert_with(|| Node::Group(BTreeMap::new()));
    match child {
        Node::Group(children) => insert(children, rest, leaf),
        Node::Leaf { .. } => bail!("{head} is both a token and a group"),
    }
}

fn struct_name(path: &[String]) -> String {
    if path.is_empty() {
        return "Tokens".into();
    }
    let mut name: String = path
        .iter()
        .map(|p| {
            let p = p.trim_start_matches("r#");
            p.split('_')
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                        .unwrap_or_default()
                })
                .collect::<String>()
        })
        .collect();
    name.push_str("Tokens");
    name
}

fn is_text_style(path: &[String]) -> bool {
    path.len() == 2 && path[0] == "text"
}

/// Every leaf with its Rust access path (`color.text_primary`).
pub fn leaves(node: &Node) -> Vec<(String, String, String)> {
    fn walk(node: &Node, path: &mut Vec<String>, out: &mut Vec<(String, String, String)>) {
        match node {
            Node::Leaf { token, kind } => out.push((token.clone(), path.join("."), kind.clone())),
            Node::Group(children) => {
                for (name, child) in children {
                    path.push(name.clone());
                    walk(child, path, out);
                    path.pop();
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(node, &mut Vec::new(), &mut out);
    out.sort();
    out
}

/// The struct definitions plus `Tokens::set`, `Tokens::get` and `NAMES`.
pub fn render_types(root: &Node) -> Result<String> {
    let mut out = String::new();
    render_struct(root, &mut Vec::new(), &mut out)?;

    let leaves = leaves(root);
    out.push_str("impl Tokens {\n");
    out.push_str("    /// The CSS custom-property name of every shared token, sorted.\n");
    out.push_str("    pub const NAMES: &'static [&'static str] = &[\n");
    for (token, _, _) in &leaves {
        out.push_str(&format!("        {token:?},\n"));
    }
    out.push_str("    ];\n\n");
    out.push_str("    /// Sets a token by its CSS name, as a theme override does.\n");
    out.push_str(
        "    pub fn set(&mut self, name: &str, value: TokenValue) -> Result<(), TokenError> {\n",
    );
    out.push_str("        match name {\n");
    for (token, path, kind) in &leaves {
        let variant = literal::variant(kind)?;
        out.push_str(&format!(
            "            {token:?} => match value {{ TokenValue::{variant}(v) => self.{path} = v, other => return Err(TokenError::wrong_kind(name, {variant:?}, &other)) }},\n"
        ));
    }
    out.push_str("            _ => return Err(TokenError::Unknown(name.to_string())),\n");
    out.push_str("        }\n        Ok(())\n    }\n\n");
    out.push_str("    /// Reads a token by its CSS name.\n");
    out.push_str("    pub fn get(&self, name: &str) -> Option<TokenValue> {\n");
    out.push_str("        Some(match name {\n");
    for (token, path, kind) in &leaves {
        let variant = literal::variant(kind)?;
        let clone = if matches!(kind.as_str(), "families" | "shadow" | "keyword") {
            ".clone()"
        } else {
            ""
        };
        out.push_str(&format!(
            "            {token:?} => TokenValue::{variant}(self.{path}{clone}),\n"
        ));
    }
    out.push_str("            _ => return None,\n        })\n    }\n}\n");
    Ok(out)
}

fn render_struct(node: &Node, path: &mut Vec<String>, out: &mut String) -> Result<()> {
    let Node::Group(children) = node else {
        return Ok(());
    };
    if is_text_style(path) {
        return Ok(());
    }
    for (name, child) in children {
        if matches!(child, Node::Group(_)) {
            path.push(name.clone());
            render_struct(child, path, out)?;
            path.pop();
        }
    }
    let doc = if path.is_empty() {
        "Every shared design token of one theme in one mode.".to_string()
    } else {
        format!("The `{}` tokens.", path.join("."))
    };
    out.push_str(&format!(
        "/// {doc}\n#[derive(Clone, Debug)]\npub struct {} {{\n",
        struct_name(path)
    ));
    for (name, child) in children {
        let ty = match child {
            Node::Leaf { token, kind } => {
                out.push_str(&format!("    /// `{token}`\n"));
                literal::rust_type(kind)?.to_string()
            }
            Node::Group(_) => {
                path.push(name.clone());
                let ty = if is_text_style(path) {
                    "TextStyleTokens".to_string()
                } else {
                    struct_name(path)
                };
                path.pop();
                ty
            }
        };
        out.push_str(&format!("    pub {name}: {ty},\n"));
    }
    out.push_str("}\n\n");
    Ok(())
}

/// One theme's tokens in one mode as a `Tokens { .. }` expression. A token
/// the theme lacks takes core's default.
pub fn render_value(
    root: &Node,
    tokens: &Map<String, Value>,
    fallback: &Map<String, Value>,
) -> Result<String> {
    fn walk(
        node: &Node,
        path: &mut Vec<String>,
        tokens: &Map<String, Value>,
        fallback: &Map<String, Value>,
    ) -> Result<String> {
        match node {
            Node::Leaf { token, kind } => {
                let value = tokens
                    .get(token)
                    .or_else(|| fallback.get(token))
                    .with_context(|| format!("no value for {token}"))?;
                let got = literal::kind(value)?;
                if got != kind {
                    bail!("{token} is a {got}, but the surface types it {kind}");
                }
                literal::expr(value)
            }
            Node::Group(children) => {
                let name = if is_text_style(path) {
                    "TextStyleTokens".to_string()
                } else {
                    struct_name(path)
                };
                let mut fields = Vec::new();
                for (field, child) in children {
                    path.push(field.clone());
                    fields.push(format!("{field}: {}", walk(child, path, tokens, fallback)?));
                    path.pop();
                }
                Ok(format!("{name} {{ {} }}", fields.join(", ")))
            }
        }
    }
    walk(root, &mut Vec::new(), tokens, fallback)
}
