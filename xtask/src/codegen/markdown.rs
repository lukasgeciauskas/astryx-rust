//! Markdown's named character references, from `spec/markdown/character-references.json`,
//! into `crates/rustryx-markdown/src/generated/character_references.rs`: a table sorted by
//! name for binary search.

use anyhow::{Context, Result};
use serde_json::Value;

pub fn render_references(spec: &Value) -> Result<String> {
    let mut entries: Vec<(String, String)> = Vec::new();
    for entry in spec["references"].as_array().context("references")? {
        let name = entry[0].as_str().context("name")?.to_string();
        let text: String = entry[1]
            .as_array()
            .context("code points")?
            .iter()
            .filter_map(|v| v.as_u64())
            .filter_map(|c| char::from_u32(c as u32))
            .collect();
        entries.push((name, text));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.dedup_by(|a, b| a.0 == b.0);
    let mut out = String::from(
        "/// Every HTML named character reference ending in `;`, sorted by name.\npub static NAMED: &[(&str, &str)] = &[\n",
    );
    for (name, text) in entries {
        out.push_str(&format!("    ({name:?}, {text:?}),\n"));
    }
    out.push_str("];\n");
    Ok(out)
}
