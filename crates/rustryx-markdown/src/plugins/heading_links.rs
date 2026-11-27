//! Heading links, upstream `Markdown/plugins/headingLinks.ts`
//! (`createMarkdownHeadingLinks`, `projectMarkdownHeadingLinks`): an opt-in plugin that
//! gives every heading, at every depth, a stable id (optionally namespaced) and a
//! permalink URL, shared by Markdown and Markdown-derived Outline. The React copy-button
//! renderer is the UI layer's; this module is the plugin and its projection.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use super::diagnostics::Diagnostics;
use super::{
    EntryRole, MarkdownPlugin, PluginDefinition, PluginError, PreparedMarkdownPlugins, Transform,
    create_entry, extension_text,
};
use crate::ast::{NodeRef, NodeType, Root, markdown_ast_text_with, visit_markdown_nodes};
use crate::url::sanitize_markdown_link_url;

const PLUGIN_NAME: &str = "heading-links";

/// Upstream `MarkdownHeadingLinksOptions`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadingLinksOptions {
    /// A caller-owned namespace for the ids (`prefix--slug`); omitted, ids are bare.
    pub heading_id_prefix: Option<String>,
    /// A caller-owned URL before the generated `#fragment`; omitted, the permalink is the
    /// bare fragment. Any fragment it has is replaced.
    pub permalink_base_url: Option<String>,
}

/// Upstream `PortableMarkdownHeadingLinksConfig`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeadingLinksConfig {
    heading_id_prefix: Option<String>,
    permalink_base_url: String,
}

/// Upstream `MarkdownHeadingLinksProjection`: per heading, by its depth-first index among
/// the document's headings, its id, label and permalink URL.
pub(crate) struct HeadingLinksProjection {
    pub(crate) headings: Vec<(String, String, String)>,
}

fn fail<T>(message: &str) -> Result<T, PluginError> {
    Err(PluginError::new(format!(
        "Markdown heading links: {message}"
    )))
}

/// Upstream `isParsablePermalinkBaseUrl`: `new URL(url, 'https://astryx.invalid/')`.
fn is_parsable_permalink_base_url(url: &str) -> bool {
    url::Url::parse("https://astryx.invalid/")
        .and_then(|base| base.join(url))
        .is_ok()
}

/// Upstream `headingSlug`: NFKC, trimmed, lowercase, without quotes, every run of
/// characters but letters and numbers one `-`, none at either end.
pub(crate) fn heading_slug(value: &str) -> String {
    static NON_SLUG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[^\p{Letter}\p{Number}]+").expect("NON_SLUG is valid"));
    let normalized = icu_normalizer::ComposingNormalizerBorrowed::new_nfkc().normalize(value);
    let lower: String = crate::js::trim(&normalized)
        .to_lowercase()
        .chars()
        .filter(|&c| c != '\'' && c != '"')
        .collect();
    NON_SLUG
        .replace_all(&lower, "-")
        .trim_matches('-')
        .to_string()
}

/// Upstream `uniqueHeadingSlug`: reserves every id it emits, so a natural `-1` suffix
/// never collides with a generated one.
pub(crate) fn unique_heading_slug(base_slug: &str, counts: &mut HashMap<String, usize>) -> String {
    let fallback = if base_slug.is_empty() {
        "section"
    } else {
        base_slug
    };
    let mut count = counts.get(fallback).copied().unwrap_or(0);
    let mut candidate = if count == 0 {
        fallback.to_string()
    } else {
        format!("{fallback}-{count}")
    };
    while counts.contains_key(&candidate) {
        count += 1;
        candidate = format!("{fallback}-{count}");
    }
    counts.insert(fallback.to_string(), count + 1);
    counts.insert(candidate.clone(), 0);
    candidate
}

/// Upstream `createMarkdownHeadingLinks`: refuses a base URL that is unsafe or does not
/// parse.
pub fn create_markdown_heading_links(
    options: HeadingLinksOptions,
) -> Result<MarkdownPlugin, PluginError> {
    let raw_base_url = options.permalink_base_url.unwrap_or_default();
    let sanitized = if raw_base_url.is_empty() {
        Some(String::new())
    } else {
        sanitize_markdown_link_url(&raw_base_url)
    };
    let Some(sanitized) = sanitized.filter(|url| is_parsable_permalink_base_url(url)) else {
        return fail("permalinkBaseUrl must be a safe, parseable navigation URL");
    };
    let permalink_base_url = match sanitized.find('#') {
        Some(fragment) => sanitized[..fragment].to_string(),
        None => sanitized,
    };
    let config = HeadingLinksConfig {
        heading_id_prefix: options.heading_id_prefix,
        permalink_base_url,
    };
    create_entry(
        PluginDefinition {
            transform: Some(Transform::new(|_, _| Ok(None)).trusted()),
            ..PluginDefinition::new(PLUGIN_NAME)
        },
        EntryRole::HeadingLinks(std::rc::Rc::new(config)),
    )
}

/// Upstream `projectMarkdownHeadingLinks`: the module's all-depth identity, after the
/// transforms ran, or `None` when the plugin is not installed.
pub(crate) fn project_markdown_heading_links(
    root: &Root,
    plugins: Option<&PreparedMarkdownPlugins>,
    diagnostics: &Diagnostics,
) -> Option<HeadingLinksProjection> {
    let config = plugins?.heading_links_config()?;
    let mut counts = HashMap::new();
    let mut headings = Vec::new();
    visit_markdown_nodes(root, NodeType::Heading, &mut |node| {
        let NodeRef::Heading(heading) = node else {
            return;
        };
        let label = crate::js::trim(&markdown_ast_text_with(&heading.children, &|node| {
            extension_text(plugins, node, diagnostics)
        }))
        .to_string();
        let slug = unique_heading_slug(&heading_slug(&label), &mut counts);
        let id = match config.heading_id_prefix.as_deref() {
            Some(prefix) if !prefix.is_empty() => format!("{prefix}--{slug}"),
            _ => slug,
        };
        let permalink = format!("{}#{id}", config.permalink_base_url);
        headings.push((id, label, permalink));
    });
    Some(HeadingLinksProjection { headings })
}
