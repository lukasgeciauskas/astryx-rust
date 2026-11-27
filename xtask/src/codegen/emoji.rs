//! The bundled emoji set of lab's EmojiPicker, from `spec/emoji.json` (upstream's generated
//! `packages/lab/src/EmojiPicker/emojiData.ts`), into
//! `crates/astryx-lab/src/emoji_picker/generated/emoji_data.rs`: the CLDR groups in order,
//! each emoji as its character, CLDR name and keywords.

use anyhow::{Context, Result};
use serde_json::Value;

pub fn render(spec: &Value) -> Result<String> {
    let source = spec["source"].as_str().unwrap_or("emojibase-data");
    let mut out = String::from(
        "/// One emoji, upstream `EmojiDatum`: the character, its CLDR name, and its other CLDR\n/// keywords joined by spaces.\npub type EmojiDatum = (&'static str, &'static str, &'static str);\n\n/// A CLDR emoji group, in CLDR order (upstream `EmojiDatumGroup`).\n#[derive(Clone, Copy, Debug)]\npub struct EmojiDatumGroup {\n    /// The group's English heading, e.g. \"Smileys & emotion\".\n    pub name: &'static str,\n    /// The group's emoji, in CLDR order.\n    pub emojis: &'static [EmojiDatum],\n}\n\n",
    );
    out.push_str(&format!(
        "/// Every CLDR group but the component group, upstream `EMOJI_GROUPS`, from {source}.\npub static EMOJI_GROUPS: &[EmojiDatumGroup] = &[\n"
    ));
    for group in spec["groups"].as_array().context("emoji groups")? {
        let name = group["name"].as_str().context("group name")?;
        out.push_str(&format!(
            "    EmojiDatumGroup {{\n        name: {name:?},\n        emojis: &[\n"
        ));
        for emoji in group["emojis"].as_array().context("group emojis")? {
            let text = emoji[0].as_str().context("emoji")?;
            let label = emoji[1].as_str().context("emoji name")?;
            let keywords = emoji[2].as_str().context("emoji keywords")?;
            out.push_str(&format!(
                "            ({text:?}, {label:?}, {keywords:?}),\n"
            ));
        }
        out.push_str("        ],\n    },\n");
    }
    out.push_str("];\n");
    Ok(out)
}
