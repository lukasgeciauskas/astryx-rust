//! The `rustryx` commands. Each returns `Err(message)` on a usage or parity
//! failure.

use std::collections::BTreeMap;

use rustryx_components::doc::{self, ComponentDoc, RustApi};
use rustryx_theme::{Appearance, Theme, Tokens};
use serde_json::Value;

use crate::manifest::{self, Entry};

type Result = std::result::Result<(), String>;

/// Each package's registry of ported components.
fn registries() -> [(&'static str, &'static [ComponentDoc]); 3] {
    [
        ("core", doc::REGISTRY),
        ("charts", rustryx_charts::doc::REGISTRY),
        ("lab", rustryx_lab::doc::REGISTRY),
    ]
}

/// The Rust doc of an upstream entry, from its package's registry.
fn find_doc(entry: &Entry) -> Option<&'static ComponentDoc> {
    if entry.package == "core" {
        return doc::find(entry.name());
    }
    registries()
        .into_iter()
        .filter(|(package, _)| *package == entry.package)
        .flat_map(|(_, docs)| docs.iter())
        .find(|d| d.name.eq_ignore_ascii_case(entry.name()))
}

const GUIDES: &[(&str, &str)] = &[
    ("gpui", include_str!("../../../docs/guide/gpui.md")),
    (
        "conventions",
        include_str!("../../../docs/guide/conventions.md"),
    ),
    (
        "customization",
        include_str!("../../../docs/guide/customization.md"),
    ),
    ("effects", include_str!("../../../docs/guide/effects.md")),
    ("porting", include_str!("../../../docs/guide/porting.md")),
];

pub fn component_list(all: bool) -> Result {
    let mut groups: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for entry in manifest::entries() {
        if !(all || entry.is_component()) {
            continue;
        }
        let group = entry.category().unwrap_or("(uncategorized)");
        let mark = if find_doc(entry).is_some() { "*" } else { "" };
        let name = if all && entry.package != "core" {
            format!("{}{mark} ({})", entry.name(), entry.package)
        } else {
            format!("{}{mark}", entry.name())
        };
        groups.entry(group).or_default().push(name);
    }
    let mut total = 0;
    for (group, mut names) in groups {
        names.sort();
        total += names.len();
        println!("{group}: {}", names.join(", "));
    }
    println!("\n{total} listed; * = ported. `rustryx component <Name>` for details.");
    Ok(())
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn truncate(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        format!("{}…", one_line.chars().take(max).collect::<String>())
    }
}

fn rust_api(api: &RustApi) -> String {
    match api {
        RustApi::Method(m) => format!(".{m}(..)"),
        RustApi::Argument(a) => format!("constructor argument `{a}`"),
        RustApi::Trait(t) => format!("via `{t}`"),
        RustApi::Waived(why) => format!("not ported: {why}"),
        RustApi::Pending(m, what) => format!(".{m}(..), pending: {what}"),
    }
}

pub fn component(name: &str, dense: bool, package: Option<&str>) -> Result {
    let found = manifest::find(name, package);
    let Some(entry) = found.first() else {
        let close: Vec<&str> = manifest::entries()
            .iter()
            .map(Entry::name)
            .filter(|n| n.to_lowercase().contains(&name.to_lowercase()))
            .take(8)
            .collect();
        return Err(if close.is_empty() {
            format!("no upstream doc named `{name}`; try `rustryx component --list`")
        } else {
            format!(
                "no upstream doc named `{name}`; did you mean {}?",
                close.join(", ")
            )
        });
    };
    let doc_json = &entry.doc;
    let rust = find_doc(entry);
    let status = match rust {
        Some(d) => format!("ported: {}", d.rust_type),
        None => "not ported yet".to_string(),
    };
    println!(
        "{} · {} · {} · {status}",
        entry.name(),
        entry.package,
        entry.category().unwrap_or("-")
    );
    if found.len() > 1 {
        let others: Vec<&str> = found[1..].iter().map(|e| e.package.as_str()).collect();
        println!("(also in: {}; pass --package)", others.join(", "));
    }

    let dense_doc = if dense { entry.dense.as_ref() } else { None };
    let summary = dense_doc
        .map(|d| text(&d["description"]).to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            doc_json["usage"]["description"]
                .as_str()
                .map(str::to_string)
        })
        .or_else(|| doc_json["description"].as_str().map(str::to_string))
        .unwrap_or_default();
    if !summary.is_empty() {
        println!(
            "{}",
            if dense {
                truncate(&summary, 160)
            } else {
                summary
            }
        );
    }

    print_props(doc_json, dense_doc, rust, dense);
    print_hook(doc_json, dense);

    let parts: Vec<&str> = manifest::entries()
        .iter()
        .filter(|e| text(&e.doc["subComponentOf"]).eq_ignore_ascii_case(entry.name()))
        .map(Entry::name)
        .collect();
    if !parts.is_empty() {
        println!("\nParts: {}", parts.join(", "));
    }

    let practices = dense_doc
        .and_then(|d| d["usage"]["bestPractices"].as_array())
        .or_else(|| doc_json["usage"]["bestPractices"].as_array());
    if let Some(practices) = practices {
        println!("\nGuidance:");
        for item in practices {
            let mark = if item["guidance"].as_bool().unwrap_or(true) {
                "+"
            } else {
                "-"
            };
            let line = text(&item["description"]);
            println!(
                "  {mark} {}",
                if dense {
                    truncate(line, 140)
                } else {
                    line.to_string()
                }
            );
        }
    }

    if let Some(rust) = rust {
        println!(
            "\nRust example ({}):\n{}",
            rust.rust_type,
            rust.example.trim_end()
        );
    }
    if !dense {
        println!("\nUpstream: ../Astryx/{}", entry.source);
    }
    Ok(())
}

fn print_props(
    doc_json: &Value,
    dense_doc: Option<&Value>,
    rust: Option<&ComponentDoc>,
    dense: bool,
) {
    let Some(props) = doc_json["props"].as_array().filter(|p| !p.is_empty()) else {
        return;
    };
    println!("\nProps ({}):", props.len());
    for prop in props {
        let name = text(&prop["name"]);
        let ty = text(&prop["type"]);
        let default = prop["default"]
            .as_str()
            .map(|d| format!(" = {d}"))
            .unwrap_or_default();
        let required = if prop["required"].as_bool().unwrap_or(false) {
            " (required)"
        } else {
            ""
        };
        let mapped = rust.map(|r| {
            r.props
                .iter()
                .find(|m| m.prop == name)
                .map(|m| rust_api(&m.rust))
                .unwrap_or_else(|| "UNMAPPED".to_string())
        });
        if dense {
            let description = dense_doc
                .and_then(|d| d["propDescriptions"][name].as_str())
                .unwrap_or_else(|| text(&prop["description"]));
            let rust = mapped.map(|m| format!(" → {m}")).unwrap_or_default();
            println!(
                "  {name}: {}{default}{required}{rust} — {}",
                truncate(ty, 70),
                truncate(description, 110)
            );
        } else {
            println!("  {name}: {ty}{default}{required}");
            let description = text(&prop["description"]);
            if !description.is_empty() {
                println!("      {description}");
            }
            if let Some(m) = mapped {
                println!("      Rust: {m}");
            }
        }
    }
}

fn print_hook(doc_json: &Value, dense: bool) {
    if let Some(params) = doc_json["params"].as_array().filter(|p| !p.is_empty()) {
        println!("\nParams:");
        for param in params {
            println!(
                "  {}: {} — {}",
                text(&param["name"]),
                text(&param["type"]),
                if dense {
                    truncate(text(&param["description"]), 110)
                } else {
                    text(&param["description"]).to_string()
                }
            );
        }
    }
    if !doc_json["returns"].is_null() {
        let returns = &doc_json["returns"];
        let ty = returns["type"]
            .as_str()
            .unwrap_or_else(|| returns.as_str().unwrap_or_default());
        println!("\nReturns: {ty}");
    }
}

fn find_theme(name: &str) -> std::result::Result<Theme, String> {
    if name == "default" {
        return Ok(Theme::default());
    }
    rustryx_themes::all()
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| format!("no theme `{name}`; see `rustryx themes`"))
}

pub fn tokens(filter: Option<&str>, theme: &str, mode: &str) -> Result {
    let appearance = match mode {
        "light" => Appearance::Light,
        "dark" => Appearance::Dark,
        other => return Err(format!("--mode is light or dark, not `{other}`")),
    };
    let theme = find_theme(theme)?;
    let tokens = theme.tokens(appearance);
    let mut shown = 0;
    for (css, rust) in Tokens::FIELDS {
        if filter.is_some_and(|f| !css.contains(f) && !rust.contains(f)) {
            continue;
        }
        let value = tokens.get(css).map(|v| v.to_string()).unwrap_or_default();
        println!("{css:<40} t.{rust:<36} {value}");
        shown += 1;
    }
    println!(
        "\n{shown} tokens · theme {} · {mode}. In code: `let t = rustryx_theme::current(window, cx); t.tokens().<field>`",
        theme.name
    );
    Ok(())
}

pub fn themes() -> Result {
    println!("default (core defaults, system fonts)");
    for theme in rustryx_themes::all() {
        println!("{} ({} font files)", theme.name, theme.fonts.len());
    }
    Ok(())
}

pub fn docs(topic: Option<&str>) -> Result {
    match topic {
        None => {
            for (name, body) in GUIDES {
                let title = body
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim_start_matches("# ");
                println!("{name:<14} {title}");
            }
            println!("\n`rustryx docs <topic>` prints one guide.");
            Ok(())
        }
        Some(topic) => match GUIDES.iter().find(|(name, _)| *name == topic) {
            Some((_, body)) => {
                print!("{body}");
                Ok(())
            }
            None => Err(format!("no guide `{topic}`; run `rustryx docs`")),
        },
    }
}

/// Problems with the registry: names upstream lacks, and upstream props a
/// registered component does not map.
pub fn registry_problems() -> Vec<String> {
    let mut problems = Vec::new();
    for (package, rust) in registries()
        .into_iter()
        .flat_map(|(package, docs)| docs.iter().map(move |d| (package, d)))
    {
        let Some(entry) = manifest::find(rust.name, Some(package)).into_iter().next() else {
            problems.push(format!(
                "{} is registered but has no upstream doc",
                rust.name
            ));
            continue;
        };
        for prop in entry.doc["props"].as_array().into_iter().flatten() {
            let name = text(&prop["name"]);
            if !rust.props.iter().any(|m| m.prop == name) {
                problems.push(format!(
                    "{}: upstream prop `{name}` is not mapped",
                    rust.name
                ));
            }
        }
    }
    for rust in doc::SPEC_ONLY {
        if !manifest::find(rust.name, None).is_empty() {
            problems.push(format!(
                "{} has an upstream doc now: move it from SPEC_ONLY to REGISTRY",
                rust.name
            ));
        }
    }
    problems
}

pub fn parity(missing: bool) -> Result {
    for package in ["core", "charts", "lab"] {
        let components: Vec<&Entry> = manifest::entries()
            .iter()
            .filter(|e| e.package == package && e.is_ported_unit())
            .collect();
        let ported = components.iter().filter(|e| find_doc(e).is_some()).count();
        println!("{ported}/{} {package} components ported", components.len());
        if missing {
            let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for entry in components.iter().filter(|e| find_doc(e).is_none()) {
                groups
                    .entry(entry.category().unwrap_or("-"))
                    .or_default()
                    .push(entry.name());
            }
            for (group, mut names) in groups {
                names.sort();
                println!("  {group}: {}", names.join(", "));
            }
        }
    }
    let problems = registry_problems();
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_lists_upstream_components() {
        let components = manifest::entries()
            .iter()
            .filter(|e| e.is_component())
            .count();
        assert!(
            components >= 100,
            "expected ~111 core components, got {components}"
        );
        assert!(!manifest::find("Button", None).is_empty());
    }

    #[test]
    fn registry_matches_upstream() {
        assert_eq!(registry_problems(), Vec::<String>::new());
    }

    #[test]
    fn every_theme_prints_every_token() {
        for theme in ["default", "neutral", "butter", "gothic"] {
            for mode in ["light", "dark"] {
                let theme = find_theme(theme).unwrap();
                let tokens = theme.tokens(if mode == "dark" {
                    Appearance::Dark
                } else {
                    Appearance::Light
                });
                for (css, _) in Tokens::FIELDS {
                    assert!(tokens.get(css).is_some(), "{css} readable");
                }
            }
        }
    }

    #[test]
    fn guides_exist() {
        for (name, body) in GUIDES {
            assert!(body.starts_with("# "), "{name} has a title");
        }
    }
}
