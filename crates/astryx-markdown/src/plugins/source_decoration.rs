//! Source decorations, upstream `Markdown/plugins/sourceDecoration.ts`
//! (`createMarkdownSourceDecoration`, `getMarkdownSourceDecorations`): caller-supplied source
//! ranges recorded, as non-semantic data in a Core-owned versioned envelope, on the
//! top-level blocks they overlap. Ranges are UTF-8 byte offsets into the parsed source
//! (upstream: UTF-16 units). Decorations describe settled source, so they appear only on a
//! final parse.

use serde_json::{Map, Value, json};

use super::{PluginError, Transform, TransformContext};
use crate::ast::{Block, DataValue, NodeRef, Root};

const ENVELOPE_KEY: &str = "astryx:sourceDecorations";
const ENVELOPE_BRAND: &str = "astryx.markdown.sourceDecorations";
const ENVELOPE_VERSION: u64 = 1;

/// Upstream `MarkdownSourceDecoration`: one decoration recorded on a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDecoration {
    pub name: String,
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownSourceDecorationRange`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDecorationRange {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownSourceDecorationOptions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDecorationOptions {
    /// The decoration name every entry this helper records carries.
    pub name: String,
    /// In any order; recorded in start, end, payload order, duplicates once.
    pub ranges: Vec<SourceDecorationRange>,
}

struct NormalizedRange {
    start: usize,
    end: usize,
    entry: Value,
}

fn fail<T>(message: &str) -> Result<T, PluginError> {
    Err(PluginError::new(format!(
        "Markdown source decoration: {message}"
    )))
}

/// Upstream `normalizeRanges`.
fn normalize_ranges(
    name: &str,
    ranges: &[SourceDecorationRange],
) -> Result<Vec<NormalizedRange>, PluginError> {
    let mut normalized: Vec<(NormalizedRange, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for range in ranges {
        if range.end <= range.start {
            return fail("ranges must be ordered, non-empty, non-negative UTF-16 offsets");
        }
        let key = format!(
            "{}\u{0}{}\u{0}{}",
            range.start,
            range.end,
            range
                .data
                .as_ref()
                .map_or(String::new(), |data| data.to_string())
        );
        if !seen.insert(key.clone()) {
            continue;
        }
        let entry = match &range.data {
            None => json!({ "name": name }),
            Some(data) => json!({ "name": name, "data": data }),
        };
        normalized.push((
            NormalizedRange {
                start: range.start,
                end: range.end,
                entry,
            },
            key,
        ));
    }
    normalized.sort_by(|(left, left_key), (right, right_key)| {
        left.start
            .cmp(&right.start)
            .then(left.end.cmp(&right.end))
            .then(left_key.cmp(right_key))
    });
    Ok(normalized.into_iter().map(|(range, _)| range).collect())
}

/// Upstream `existingEntries`: the decoration entries on a node's data, `Ok(None)` for none,
/// `Err(())` for data this helper does not own.
fn existing_entries(data: Option<&DataValue>) -> Result<Option<&Vec<Value>>, ()> {
    let Some(data) = data else {
        return Ok(None);
    };
    let Value::Object(data) = data else {
        return Err(());
    };
    let Some(envelope) = data.get(ENVELOPE_KEY) else {
        return Ok(None);
    };
    match envelope {
        Value::Object(envelope)
            if envelope.get("kind").and_then(Value::as_str) == Some(ENVELOPE_BRAND)
                && envelope.get("version").and_then(Value::as_u64) == Some(ENVELOPE_VERSION) =>
        {
            match envelope.get("entries") {
                Some(Value::Array(entries))
                    if entries.iter().all(|entry| {
                        entry
                            .as_object()
                            .is_some_and(|entry| entry.get("name").is_some_and(Value::is_string))
                    }) =>
                {
                    Ok(Some(entries))
                }
                _ => Err(()),
            }
        }
        _ => Err(()),
    }
}

fn envelope(entries: Vec<Value>) -> Value {
    json!({ "kind": ENVELOPE_BRAND, "version": ENVELOPE_VERSION, "entries": entries })
}

fn block_data(block: &mut Block) -> Option<&mut Option<DataValue>> {
    match block {
        Block::Heading(node) => Some(&mut node.data),
        Block::Paragraph(node) => Some(&mut node.data),
        Block::Code(node) => Some(&mut node.data),
        Block::Math(node) => Some(&mut node.data),
        Block::Blockquote(node) => Some(&mut node.data),
        Block::List(node) => Some(&mut node.data),
        Block::Table(node) => Some(&mut node.data),
        Block::ThematicBreak(node) => Some(&mut node.data),
        Block::Image(node) => Some(&mut node.data),
        // Extension nodes belong to their plugin; their data is their identity.
        Block::Extension(_) => None,
    }
}

/// Upstream `decorateNode` / `mergeDecorations`. Returns whether the node changed.
fn decorate(
    block: &mut Block,
    entries: Vec<Value>,
    context: &TransformContext<'_>,
    reported: &mut bool,
) -> bool {
    let Some(data) = block_data(block) else {
        return false;
    };
    let current = match existing_entries(data.as_ref()) {
        Ok(current) => current.cloned(),
        Err(()) => {
            if !*reported {
                *reported = true;
                context.report(
                    "skipped a node whose data is not a Core-owned source-decoration envelope",
                );
            }
            return false;
        }
    };
    let merged = envelope(match current {
        None => entries,
        Some(mut current) => {
            current.extend(entries);
            current
        }
    });
    let mut object = match data.take() {
        Some(Value::Object(object)) => object,
        _ => Map::new(),
    };
    object.insert(ENVELOPE_KEY.to_string(), merged);
    *data = Some(Value::Object(object));
    true
}

/// Upstream `createMarkdownSourceDecoration`: a trusted transform recording each range on
/// the top-level blocks whose Core-authored span it overlaps, on a final parse only.
pub fn create_markdown_source_decoration(
    options: SourceDecorationOptions,
) -> Result<Transform, PluginError> {
    if crate::js::is_blank(&options.name) {
        return fail("name must be a non-empty string");
    }
    let ranges = normalize_ranges(&options.name, &options.ranges)?;
    let smallest_start = ranges.first().map(|range| range.start);
    let transform = Transform::new(move |document: &Root, context: &TransformContext<'_>| {
        if !context.is_final() || ranges.is_empty() {
            return Ok(None);
        }
        let source_len = context.source().len();
        let usable: Vec<&NormalizedRange> = ranges
            .iter()
            .filter(|range| range.start < source_len)
            .collect();
        if usable.is_empty() {
            return Ok(None);
        }
        let mut next = document.clone();
        let mut changed = false;
        let mut reported = false;
        for block in &mut next.children {
            let Some(position) = crate::plugins::node_position(NodeRef::from(&*block)) else {
                continue;
            };
            let (Some(start), Some(end)) = (position.start.offset, position.end.offset) else {
                continue;
            };
            if end < start {
                continue;
            }
            let entries: Vec<Value> = usable
                .iter()
                .filter(|range| range.start < end && range.end > start)
                .map(|range| range.entry.clone())
                .collect();
            if !entries.is_empty() {
                changed |= decorate(block, entries, context, &mut reported);
            }
        }
        Ok(changed.then_some(next))
    });
    Ok(transform
        .with_claim(move |source| smallest_start.is_some_and(|start| source.len() > start))
        .trusted())
}

/// Upstream `getMarkdownSourceDecorations`: the decorations recorded on `node`; none for an
/// undecorated node or data that is not a current Core-owned envelope.
pub fn get_markdown_source_decorations(node: NodeRef<'_>) -> Vec<SourceDecoration> {
    match existing_entries(node.data()) {
        Ok(Some(entries)) => entries
            .iter()
            .filter_map(|entry| {
                Some(SourceDecoration {
                    name: entry.get("name")?.as_str()?.to_string(),
                    data: entry.get("data").cloned(),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}
