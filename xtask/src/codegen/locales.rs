//! `crates/rustryx-i18n/src/generated/catalogs.rs` from `spec/locales/*.json`:
//! every message key, English for each, and the shipped translations as
//! sparse (key index, message) pairs, so lookups are binary searches over
//! static data.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::{HEADER, read_json};

fn messages(catalog: &Value) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for (key, entry) in catalog.as_object().context("a catalog is an object")? {
        let message = match entry {
            Value::String(message) => message.clone(),
            entry => entry["defaultMessage"]
                .as_str()
                .with_context(|| format!("{key} has no defaultMessage"))?
                .to_string(),
        };
        out.insert(key.clone(), message);
    }
    Ok(out)
}

pub fn render(spec: &Path) -> Result<String> {
    let dir = spec.join("locales");
    let english = messages(&read_json(&dir.join("en.json"))?)?;
    let keys: Vec<&String> = english.keys().collect();
    if keys.len() > u16::MAX as usize {
        bail!("too many message keys for u16 indices");
    }

    let mut locales = BTreeMap::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        let Some(tag) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().is_some_and(|e| e == "json") && tag != "en" {
            locales.insert(tag.to_string(), messages(&read_json(&path)?)?);
        }
    }

    let mut out = format!(
        "{HEADER}/// Every message key Astryx's components use, sorted.\npub const KEYS: &[&str] = &[\n"
    );
    for key in &keys {
        out.push_str(&format!("    {key:?},\n"));
    }
    out.push_str("];\n\n/// English for every key, in `KEYS` order: the last fallback.\npub const ENGLISH: &[&str] = &[\n");
    for message in english.values() {
        out.push_str(&format!("    {message:?},\n"));
    }
    out.push_str(
        "];\n\n/// The shipped translations by locale tag: (index into `KEYS`, message),\n/// sorted by index. Keys a locale lacks fall back to English.\n#[cfg(feature = \"locales\")]\npub const SHIPPED: &[(&str, &[(u16, &str)])] = &[\n",
    );
    for (tag, catalog) in &locales {
        out.push_str(&format!("    ({tag:?}, &[\n"));
        for (index, key) in keys.iter().enumerate() {
            if let Some(message) = catalog.get(*key) {
                out.push_str(&format!("        ({index}, {message:?}),\n"));
            }
        }
        for key in catalog.keys() {
            if !english.contains_key(key) {
                bail!("{tag} has {key}, which English lacks");
            }
        }
        out.push_str("    ]),\n");
    }
    out.push_str(
        "];\n\n/// Built without the `locales` feature: English only.\n#[cfg(not(feature = \"locales\"))]\npub const SHIPPED: &[(&str, &[(u16, &str)])] = &[];\n",
    );
    Ok(out)
}
