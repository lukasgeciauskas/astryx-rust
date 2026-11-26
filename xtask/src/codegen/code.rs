//! CodeBlock's grammars and the syntax theme presets, from `spec/code/`, into
//! `crates/astryx-syntax/src/generated/`. Each rule's JS regex is translated into the
//! `fancy-regex` dialect: flags become an inline group, and the ASCII meanings JS gives
//! `\w` and `\d` are spelled out (Rust's are Unicode). `\b` stays Unicode:
//! `fancy-regex` has no ASCII boundary, so it differs from JS only beside a non-ASCII letter.

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// The regex-syntax metacharacters an escape keeps; other escaped punctuation is literal.
const META: &str = r".+*?()|[]{}^$#&-~\";

/// Translates a JS regex source and its flags.
pub fn translate(source: &str, flags: &str) -> Result<String> {
    let mut out = String::new();
    for flag in flags.chars() {
        match flag {
            'i' => out.push_str("(?i)"),
            'm' => out.push_str("(?m)"),
            's' => out.push_str("(?s)"),
            'u' | 'd' => {}
            other => bail!("unsupported regex flag {other} in /{source}/"),
        }
    }
    let mut chars = source.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let n = chars.next().context("a trailing backslash")?;
                match n {
                    'w' if in_class => out.push_str("[:word:]"),
                    'w' => out.push_str("[[:word:]]"),
                    'W' if !in_class => out.push_str("[^[:word:]]"),
                    'd' if in_class => out.push_str("0-9"),
                    'd' => out.push_str("[0-9]"),
                    'D' if !in_class => out.push_str("[^0-9]"),
                    'u' => {
                        let hex: String = if chars.peek() == Some(&'{') {
                            chars.next();
                            chars.by_ref().take_while(|c| *c != '}').collect()
                        } else {
                            chars.by_ref().take(4).collect()
                        };
                        out.push_str(&format!(r"\x{{{hex}}}"));
                    }
                    'W' | 'D' | 'c' => bail!("unsupported escape \\{n} in /{source}/"),
                    n if n.is_ascii_punctuation() && !META.contains(n) => out.push(n),
                    n => {
                        out.push('\\');
                        out.push(n);
                    }
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
                if chars.peek() == Some(&'^') {
                    out.push('^');
                    chars.next();
                }
            }
            // JS allows a bare `[` in a class; Rust reads it as a nested class.
            '[' => out.push_str(r"\["),
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            c => out.push(c),
        }
    }
    Ok(out)
}

/// A raw string literal that holds `s`.
fn raw(s: &str) -> String {
    let mut hashes = 1;
    while s.contains(&format!("\"{}", "#".repeat(hashes))) {
        hashes += 1;
    }
    let h = "#".repeat(hashes);
    format!("r{h}\"{s}\"{h}")
}

fn strings(value: &Value) -> Result<String> {
    let items: Vec<String> = value
        .as_array()
        .context("a string list")?
        .iter()
        .map(|v| format!("{:?}", v.as_str().unwrap_or_default()))
        .collect();
    Ok(format!("&[{}]", items.join(", ")))
}

fn grammar(value: &Value) -> Result<String> {
    let mut patterns = Vec::new();
    for p in value["patterns"].as_array().context("patterns")? {
        let source = p["source"].as_str().context("source")?;
        let flags = p["flags"].as_str().unwrap_or("");
        patterns.push(format!(
            "({:?}, {})",
            p["type"].as_str().context("type")?,
            raw(&translate(source, flags)?)
        ));
    }
    Ok(format!(
        "GrammarDef {{ default_type: {:?}, patterns: &[{}] }}",
        value["defaultType"].as_str().context("defaultType")?,
        patterns.join(",\n")
    ))
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else if c == '-' {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    out
}

/// `generated/languages.rs`: the built-in grammars and the language modules.
pub fn render_languages(spec: &Value) -> Result<String> {
    let mut out =
        String::from("use crate::grammar::{BuiltinGrammar, GrammarDef, ModuleLanguage};\n\n");
    out.push_str("/// The built-in grammars, each with the identifiers that select it.\npub static BUILTINS: &[BuiltinGrammar] = &[\n");
    for b in spec["builtins"].as_array().context("builtins")? {
        out.push_str(&format!(
            "BuiltinGrammar {{ identifiers: {}, grammar: {} }},\n",
            strings(&b["identifiers"])?,
            grammar(b)?
        ));
    }
    out.push_str("];\n\n/// The language modules, upstream `CodeBlock/languages/*`.\npub static MODULES: &[ModuleLanguage] = &[\n");
    let modules = spec["modules"].as_array().context("modules")?;
    for m in modules {
        out.push_str(&format!(
            "ModuleLanguage {{ name: {:?}, identifiers: {}, file_extensions: {}, file_names: {}, grammar: {} }},\n",
            m["name"].as_str().context("name")?,
            strings(&m["identifiers"])?,
            strings(&m["fileExtensions"])?,
            strings(&m["fileNames"])?,
            grammar(m)?
        ));
    }
    out.push_str("];\n\n/// The language values, one per module.\n#[allow(clippy::module_inception)]\npub mod languages {\n    use crate::CodeLanguage;\n\n");
    for (i, m) in modules.iter().enumerate() {
        let export = m["export"].as_str().context("export")?;
        out.push_str(&format!(
            "    /// Upstream `@astryxdesign/core/CodeBlock/languages/{}`: {}.\n    pub fn {}() -> CodeLanguage {{\n        CodeLanguage::module(&super::MODULES[{i}])\n    }}\n\n",
            m["module"].as_str().unwrap_or_default(),
            m["name"].as_str().unwrap_or_default(),
            snake(export),
        ));
    }
    out.push_str("}\n");
    Ok(out)
}

fn color(value: &Value) -> Result<String> {
    Ok(match value {
        Value::String(s) => format!("SyntaxColor::Single(Cow::Borrowed({s:?}))"),
        Value::Array(pair) if pair.len() == 2 => format!(
            "SyntaxColor::LightDark(Cow::Borrowed({:?}), Cow::Borrowed({:?}))",
            pair[0].as_str().context("light")?,
            pair[1].as_str().context("dark")?
        ),
        other => bail!("a syntax color: {other}"),
    })
}

/// `generated/presets.rs`: the syntax theme presets.
pub fn render_presets(spec: &Value) -> Result<String> {
    let mut out = String::from(
        "use std::borrow::Cow;\n\nuse crate::theme::{SyntaxColor, SyntaxThemeDefinition, SyntaxThemeTokens};\n\n",
    );
    const SLOTS: [(&str, &str); 14] = [
        ("keyword", "keyword"),
        ("string", "string"),
        ("comment", "comment"),
        ("number", "number"),
        ("function", "function"),
        ("type", "r#type"),
        ("variable", "variable"),
        ("operator", "operator"),
        ("constant", "constant"),
        ("tag", "tag"),
        ("attribute", "attribute"),
        ("property", "property"),
        ("punctuation", "punctuation"),
        ("background", "background"),
    ];
    const DIFF: [(&str, &str); 5] = [
        ("added", "added"),
        ("removed", "removed"),
        ("diff-header", "diff_header"),
        ("added-marker", "added_marker"),
        ("removed-marker", "removed_marker"),
    ];
    let themes = spec["themes"].as_array().context("themes")?;
    for theme in themes {
        let name = theme["name"].as_str().context("name")?;
        let export = theme["export"].as_str().context("export")?;
        let tokens = &theme["tokens"];
        let mut fields = Vec::new();
        for (key, field) in SLOTS {
            fields.push(format!("{field}: {}", color(&tokens[key])?));
        }
        for (key, field) in DIFF {
            fields.push(match tokens.get(key) {
                Some(v) => format!("{field}: Some({})", color(v)?),
                None => format!("{field}: None"),
            });
        }
        out.push_str(&format!(
            "/// The `{name}` preset.\npub fn {}() -> SyntaxThemeDefinition {{\n    SyntaxThemeDefinition {{ name: Cow::Borrowed({name:?}), tokens: SyntaxThemeTokens {{ {} }} }}\n}}\n\n",
            snake(export),
            fields.join(", ")
        ));
    }
    let list = |key: &str| -> Result<String> {
        let names: Vec<String> = spec[key]
            .as_array()
            .context("preset list")?
            .iter()
            .filter_map(Value::as_str)
            .map(|n| {
                let theme = themes.iter().find(|t| t["name"].as_str() == Some(n));
                format!(
                    "{}()",
                    snake(theme.and_then(|t| t["export"].as_str()).unwrap_or(n))
                )
            })
            .collect();
        Ok(names.join(", "))
    };
    out.push_str(&format!(
        "/// Upstream `darkSyntaxPresets`.\npub fn dark_syntax_presets() -> Vec<SyntaxThemeDefinition> {{\n    vec![{}]\n}}\n\n/// Upstream `lightSyntaxPresets`.\npub fn light_syntax_presets() -> Vec<SyntaxThemeDefinition> {{\n    vec![{}]\n}}\n\n",
        list("dark")?,
        list("light")?
    ));
    let all: Vec<String> = themes
        .iter()
        .filter_map(|t| t["export"].as_str())
        .map(|e| format!("{}()", snake(e)))
        .collect();
    out.push_str(&format!(
        "/// Upstream `allSyntaxPresets`.\npub fn all_syntax_presets() -> Vec<SyntaxThemeDefinition> {{\n    vec![{}]\n}}\n",
        all.join(", ")
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::translate;

    #[test]
    fn js_classes_keep_their_ascii_meanings() {
        assert_eq!(translate(r"\b[\w$]+\b", "").unwrap(), r"\b[[:word:]$]+\b");
        assert_eq!(translate(r"\/\/[^\n]*", "m").unwrap(), r"(?m)//[^\n]*");
        assert_eq!(translate(r"[+\-*/]", "i").unwrap(), r"(?i)[+\-*/]");
    }
}
