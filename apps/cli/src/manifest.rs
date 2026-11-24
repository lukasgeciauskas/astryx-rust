//! Upstream's component and hook docs, embedded from `spec/manifest.json`.

use std::sync::OnceLock;

use serde_json::Value;

const MANIFEST: &str = include_str!("../../../spec/manifest.json");

/// One upstream `.doc.mjs` entry.
pub struct Entry {
    pub package: String,
    pub source: String,
    pub doc: Value,
    pub dense: Option<Value>,
}

impl Entry {
    pub fn name(&self) -> &str {
        self.doc["name"].as_str().unwrap_or_default()
    }

    pub fn category(&self) -> Option<&str> {
        self.doc["category"].as_str()
    }

    /// A top-level component: core, with a capitalized category (lowercase
    /// categories are hooks), and not a sub-part of another component.
    pub fn is_component(&self) -> bool {
        self.package == "core"
            && self
                .category()
                .is_some_and(|c| c.starts_with(char::is_uppercase))
            && self.doc["subComponentOf"].is_null()
            && !self.name().starts_with("use")
    }
}

pub fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let manifest: Value =
            serde_json::from_str(MANIFEST).expect("spec/manifest.json is valid JSON");
        manifest["entries"]
            .as_array()
            .expect("manifest has entries")
            .iter()
            .map(|e| Entry {
                package: e["package"].as_str().unwrap_or_default().to_string(),
                source: e["source"].as_str().unwrap_or_default().to_string(),
                doc: e["doc"].clone(),
                dense: (!e["dense"].is_null()).then(|| e["dense"].clone()),
            })
            .collect()
    })
}

/// Entries named `name` (case-insensitive), core first.
pub fn find(name: &str, package: Option<&str>) -> Vec<&'static Entry> {
    let mut found: Vec<&Entry> = entries()
        .iter()
        .filter(|e| e.name().eq_ignore_ascii_case(name))
        .filter(|e| package.is_none_or(|p| e.package == p))
        .collect();
    found.sort_by_key(|e| e.package != "core");
    found
}
