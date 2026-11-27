//! Frontmatter, upstream `Markdown/plugins/frontmatter.ts` (`createMarkdownFrontmatter`): a
//! leading `---` block of unique `key: value` lines, decoded by the caller's typed decoder
//! into finite JSON-like metadata stored on the document (`root.data`), and removed from
//! the rendered content. An unfinished block yields no content while streaming; malformed
//! or unfinished final input stays ordinary Markdown.

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::rc::Rc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::{
    MarkdownPlugin, PluginDefinition, PluginError, Transform, TransformContext,
    create_markdown_plugin,
};
use crate::ast::Root;

const ENVELOPE_KEY: &str = "astryx:frontmatter";
const ENVELOPE_BRAND: &str = "astryx.markdown.frontmatter";
const ENVELOPE_VERSION: u64 = 1;

/// The validated `key: value` fields of a frontmatter block.
pub type FrontmatterFields = BTreeMap<String, String>;

/// A caller's decoder: typed metadata from the fields, or an error (the transform fails
/// and the document keeps its frontmatter as Markdown).
pub type FrontmatterDecoder<M> = dyn Fn(&FrontmatterFields) -> Result<M, String>;

/// Upstream `MarkdownFrontmatterParseResult`.
#[derive(Clone, Debug, PartialEq)]
pub enum FrontmatterParseResult<M> {
    None,
    /// An unfinished block while streaming.
    Defer,
    Match {
        /// Where the Markdown after the block starts.
        content_start: usize,
        metadata: M,
    },
}

/// Upstream `MarkdownFrontmatter`: the plugin, and typed access to what it reads.
pub struct MarkdownFrontmatter<M> {
    pub plugin: MarkdownPlugin,
    name: String,
    decode: Rc<FrontmatterDecoder<M>>,
    metadata: PhantomData<M>,
}

impl<M> Clone for MarkdownFrontmatter<M> {
    fn clone(&self) -> Self {
        MarkdownFrontmatter {
            plugin: self.plugin.clone(),
            name: self.name.clone(),
            decode: self.decode.clone(),
            metadata: PhantomData,
        }
    }
}

enum Read {
    None,
    Defer,
    Match {
        end: usize,
        fields: FrontmatterFields,
    },
}

/// Upstream `readFrontmatter`.
fn read_frontmatter(source: &str) -> Read {
    let opening = if source.starts_with("---\r\n") {
        5
    } else if source.starts_with("---\n") {
        4
    } else {
        return Read::None;
    };
    let body = &source[opening..];
    // `/(?:^|\r?\n)---(?=\r?\n|$)/`: the first `---` line; `fields_end` is where the
    // match starts, `close_end` where it ends.
    let closes_at = |at: usize| {
        body[at..].starts_with("---") && {
            let after = &body[at + 3..];
            after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n")
        }
    };
    let close = std::iter::once(0)
        .chain(body.match_indices('\n').map(|(at, _)| at + 1))
        .find(|&at| closes_at(at));
    let Some(dashes) = close else {
        return Read::Defer;
    };
    let fields_end = if dashes == 0 {
        0
    } else if body[..dashes - 1].ends_with('\r') {
        dashes - 2
    } else {
        dashes - 1
    };
    let mut fields = FrontmatterFields::new();
    for line in body[..fields_end].split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if crate::js::is_blank(line) {
            continue;
        }
        let Some(separator) = line.find(':') else {
            return Read::None;
        };
        let key = crate::js::trim(&line[..separator]);
        if separator == 0 || key.is_empty() || fields.contains_key(key) {
            return Read::None;
        }
        fields.insert(
            key.to_string(),
            crate::js::trim(&line[separator + 1..]).to_string(),
        );
    }
    let mut end = opening + dashes + 3;
    if source[end..].starts_with("\r\n") {
        end += 2;
    } else if source[end..].starts_with('\n') {
        end += 1;
    }
    Read::Match { end, fields }
}

/// The current envelope at `root.data[ENVELOPE_KEY]`, if any.
fn envelope_values(data: Option<&Value>) -> Option<&Map<String, Value>> {
    let envelope = data?.as_object()?.get(ENVELOPE_KEY)?.as_object()?;
    (envelope.get("brand")?.as_str()? == ENVELOPE_BRAND
        && envelope.get("version")?.as_u64()? == ENVELOPE_VERSION)
        .then(|| envelope.get("values")?.as_object())
        .flatten()
}

impl<M: Serialize + DeserializeOwned + 'static> MarkdownFrontmatter<M> {
    /// Upstream `parse`: the frontmatter at the start of `source`. A decoder error, or
    /// metadata that is not JSON-like data, is an error.
    pub fn parse(
        &self,
        source: &str,
        is_final: bool,
    ) -> Result<FrontmatterParseResult<M>, PluginError> {
        parse_with(&*self.decode, source, is_final).map(|parsed| match parsed {
            Parsed::None => FrontmatterParseResult::None,
            Parsed::Defer => FrontmatterParseResult::Defer,
            Parsed::Match {
                content_start,
                metadata,
                ..
            } => FrontmatterParseResult::Match {
                content_start,
                metadata,
            },
        })
    }

    /// Upstream `getMetadata`: the metadata this plugin stored on `document`.
    pub fn get_metadata(&self, document: &Root) -> Option<M> {
        let value = envelope_values(document.data.as_ref())?.get(&self.name)?;
        serde_json::from_value(value.clone()).ok()
    }
}

enum Parsed<M> {
    None,
    Defer,
    Match {
        content_start: usize,
        metadata: M,
        value: Value,
    },
}

fn parse_with<M: Serialize>(
    decode: &FrontmatterDecoder<M>,
    source: &str,
    is_final: bool,
) -> Result<Parsed<M>, PluginError> {
    match read_frontmatter(source) {
        Read::None => Ok(Parsed::None),
        Read::Defer if is_final => Ok(Parsed::None),
        Read::Defer => Ok(Parsed::Defer),
        Read::Match { end, fields } => {
            let metadata = decode(&fields).map_err(PluginError::new)?;
            let value = serde_json::to_value(&metadata).map_err(|_| {
                PluginError::new("Markdown frontmatter metadata must be finite JSON-like data")
            })?;
            Ok(Parsed::Match {
                content_start: end,
                metadata,
                value,
            })
        }
    }
}

/// Upstream `createMarkdownFrontmatter`: `name` is the plugin's name and the metadata's
/// namespace; `decode` turns the fields into typed metadata.
pub fn create_markdown_frontmatter<M: Serialize + DeserializeOwned + 'static>(
    name: impl Into<String>,
    decode: impl Fn(&FrontmatterFields) -> Result<M, String> + 'static,
) -> Result<MarkdownFrontmatter<M>, PluginError> {
    let name = name.into();
    if crate::js::is_blank(&name) {
        return Err(PluginError::new(
            "Markdown frontmatter name must be non-empty",
        ));
    }
    let decode: Rc<FrontmatterDecoder<M>> = Rc::new(decode);
    let transform_decode = decode.clone();
    let namespace = name.clone();
    let transform = Transform::new(move |document: &Root, context: &TransformContext<'_>| {
        let parsed = parse_with(&*transform_decode, context.source(), context.is_final())
            .map_err(|error| error.to_string())?;
        let (content_start, value) = match parsed {
            Parsed::None => return Ok(None),
            Parsed::Defer => {
                return Ok(Some(Root {
                    children: Vec::new(),
                    ..document.clone()
                }));
            }
            Parsed::Match {
                content_start,
                value,
                ..
            } => (content_start, value),
        };
        let mut values = envelope_values(document.data.as_ref())
            .cloned()
            .unwrap_or_default();
        values.insert(namespace.clone(), value);
        let mut data = match &document.data {
            Some(Value::Object(data)) => data.clone(),
            _ => Map::new(),
        };
        data.insert(
            ENVELOPE_KEY.to_string(),
            json!({ "brand": ENVELOPE_BRAND, "version": ENVELOPE_VERSION, "values": values }),
        );
        let children = document
            .children
            .iter()
            .filter(|node| {
                crate::plugins::node_position(crate::ast::NodeRef::from(*node))
                    .and_then(|position| position.end.offset)
                    .is_none_or(|end| end > content_start)
            })
            .cloned()
            .collect();
        Ok(Some(Root {
            children,
            data: Some(Value::Object(data)),
            ..document.clone()
        }))
    })
    .trusted();
    let plugin = create_markdown_plugin(PluginDefinition {
        transform: Some(transform),
        ..PluginDefinition::new(name.clone())
    })?;
    Ok(MarkdownFrontmatter {
        plugin,
        name,
        decode,
        metadata: PhantomData,
    })
}
