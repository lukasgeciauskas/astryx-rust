//! useOutlineFromMarkdown, upstream `Outline/useOutlineFromMarkdown.ts`: a Markdown
//! string's root headings as Outline items, with Markdown's own parser, transforms and
//! heading ids, so every item targets the heading Markdown renders.

use std::rc::Rc;

use astryx_markdown::plugins::MarkdownPlugin;
use gpui::{App, ElementId, SharedString, Window};

use super::OutlineItem;

/// Upstream `ParseOutlineFromMarkdownOptions`.
#[derive(Clone)]
pub struct ParseOutlineFromMarkdownOptions {
    /// The plugin list given to Markdown, so transforms, labels and ids agree.
    pub plugins: Vec<MarkdownPlugin>,
    /// Whether the source is complete: false while Markdown streams.
    pub is_final: bool,
}

impl Default for ParseOutlineFromMarkdownOptions {
    fn default() -> Self {
        ParseOutlineFromMarkdownOptions {
            plugins: Vec::new(),
            is_final: true,
        }
    }
}

/// The items of the last parse, kept until the source or options change.
#[derive(Default)]
struct Memo {
    source: SharedString,
    plugins: Vec<MarkdownPlugin>,
    is_final: bool,
    items: Rc<Vec<OutlineItem>>,
}

/// Upstream `parseOutlineFromMarkdown`: the outline of `markdown`.
pub fn parse_outline_from_markdown(
    markdown: &str,
    options: &ParseOutlineFromMarkdownOptions,
) -> Vec<OutlineItem> {
    astryx_markdown::parse_outline_from_markdown(markdown, &options.plugins, options.is_final)
        .into_iter()
        .map(|item| OutlineItem::new(item.id, item.label, item.level))
        .collect()
}

/// Upstream `useOutlineFromMarkdown`: the outline of `markdown`, parsed again only when the
/// source, plugins or finality change. Pass Markdown the same source and plugins.
///
/// ```ignore
/// let items = use_outline_from_markdown("doc-outline", &source, Default::default(), window, cx);
/// Outline::new("doc-outline", items)
/// ```
pub fn use_outline_from_markdown(
    id: impl Into<ElementId>,
    markdown: &str,
    options: ParseOutlineFromMarkdownOptions,
    window: &mut Window,
    cx: &mut App,
) -> Vec<OutlineItem> {
    let id = id.into();
    let memo = window.use_keyed_state(
        ElementId::Name(format!("{id}-outline-from-markdown").into()),
        cx,
        |_, _| Memo::default(),
    );
    let fresh = {
        let m = memo.read(cx);
        m.source.as_ref() != markdown
            || m.is_final != options.is_final
            || m.plugins.len() != options.plugins.len()
            || m.plugins
                .iter()
                .zip(&options.plugins)
                .any(|(a, b)| !a.ptr_eq(b))
    };
    if fresh {
        let items = parse_outline_from_markdown(markdown, &options);
        memo.update(cx, |m, _| {
            m.source = markdown.to_string().into();
            m.plugins = options.plugins.clone();
            m.is_final = options.is_final;
            m.items = Rc::new(items);
        });
    }
    memo.read(cx).items.as_ref().clone()
}
