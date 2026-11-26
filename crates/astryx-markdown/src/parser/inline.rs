//! The inline parser, upstream `Markdown/parser.ts` ("Inline parser" and its helpers): one
//! pass over a block's text that builds code spans, math, citations, images, links,
//! reference links and images, plugins' inline extension syntax, escapes, character
//! references and breaks, leaving runs of `*` and `_` for [`resolve_emphasis`].

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

use super::autolink::transform_autolinks;
use super::emphasis::{DelimiterRun, delimiter_run, resolve_emphasis};
use super::extension::{ExtensionMatch, match_extension_syntax};
use super::inline_index::InlineIndex;
use super::link_definitions::normalize_link_label;
use super::{LinkDefinitions, ResolvedOptions};
use crate::ast::{
    Citation, ExtensionDisplay, Image, Link, Literal, Parent, Phrasing, Void, markdown_ast_text,
};
use crate::character_references::{
    decode_literal_text, is_ascii_punctuation, match_character_reference,
};
use crate::js;
use crate::url::{is_safe_markdown_parser_url, normalized_markdown_destination};

/// The deepest image descriptions are read as inline content (upstream
/// `MAX_IMAGE_ALT_DEPTH`). Reading one reads every image nested in it again; deeper
/// descriptions keep their decoded source.
const MAX_IMAGE_ALT_DEPTH: usize = 10;

/// Upstream `INLINE_DESTINATION_WITH_TITLE`: the content between an inline link's or
/// image's parentheses, a `<…>` destination or one without spaces, then an optional
/// `"…"`, `'…'` or `(…)` title after whitespace (CommonMark 0.31 §6.3).
static INLINE_DESTINATION_WITH_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    let pattern = r#"(?s)^[{ws}]*(?:<((?:[^<>\n\\]|\\.)*)>|([^{ws}<][^{ws}]*?))(?:[{ws}]+(?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\((?:[^()\\]|\\.)*\)))?[{ws}]*$"#;
    Regex::new(&pattern.replace("{ws}", js::WHITESPACE_CLASS))
        .expect("INLINE_DESTINATION_WITH_TITLE is valid")
});

/// Where inline text sits, which changes how a code span reads.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlineContext {
    Default,
    /// In a table cell, `\|` in a code span is a `|`.
    TableCell,
}

/// One item of the scan: a finished node, or a delimiter run (an index into the runs)
/// that stays its own text node until emphasis is resolved. The items live in one
/// short-lived Vec per inline parse, so the node is kept unboxed.
#[allow(clippy::large_enum_variant)]
pub(super) enum Item {
    Node(Phrasing),
    Run(usize),
}

fn text_node(value: &str) -> Phrasing {
    Phrasing::Text(Literal {
        value: value.to_string(),
        position: None,
        data: None,
    })
}

/// Upstream `appendInlineText`: adds `value` to the last text node, unless that is a
/// delimiter run.
fn append_inline_text(items: &mut Vec<Item>, value: &str) {
    if let Some(Item::Node(Phrasing::Text(last))) = items.last_mut() {
        last.value.push_str(value);
    } else {
        items.push(Item::Node(text_node(value)));
    }
}

/// Upstream `lineEndingLengthAt`: the length of the line ending at `index`: 1 for LF, 2
/// for CRLF (the parser keeps a CRLF line's `\r`), or 0.
fn line_ending_length_at(bytes: &[u8], index: usize) -> usize {
    match (bytes.get(index), bytes.get(index + 1)) {
        (Some(b'\n'), _) => 1,
        (Some(b'\r'), Some(b'\n')) => 2,
        _ => 0,
    }
}

/// Upstream `isEscaped`: whether an odd run of backslashes precedes `index`.
pub(super) fn is_escaped(bytes: &[u8], index: usize) -> bool {
    bytes[..index]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
        % 2
        == 1
}

/// Upstream `isInlineMathStart`: whitespace cannot hug the delimiters, a digit cannot sit
/// just before the opener, and `$$` is display math's.
pub(super) fn is_inline_math_start(text: &str, index: usize) -> bool {
    let bytes = text.as_bytes();
    let before = index.checked_sub(1).map(|i| bytes[i]);
    bytes[index] == b'$'
        && before != Some(b'$')
        && bytes.get(index + 1).is_some_and(|&next| next != b'$')
        && !js::char_at(text, index + 1).is_some_and(js::is_whitespace)
        && !before.is_some_and(|b| b.is_ascii_digit())
        && !is_escaped(bytes, index)
}

/// Upstream `findInlineMathEnd`: the `$` that closes `$…$` math opened at `start`, on
/// the same line. The first unescaped `$` ends the candidate: it closes it, or the span is
/// text, so "$20 and $30" stays prose. No digit may follow the closer.
pub(super) fn find_inline_math_end(text: &str, start: usize) -> Option<usize> {
    if !is_inline_math_start(text, start) {
        return None;
    }
    let bytes = text.as_bytes();
    for index in start + 1..bytes.len() {
        if bytes[index] == b'\n' {
            return None;
        }
        if bytes[index] != b'$' || is_escaped(bytes, index) {
            continue;
        }
        let refused = bytes[index - 1] == b'$'
            || bytes.get(index + 1) == Some(&b'$')
            || js::char_before(text, index).is_some_and(js::is_whitespace)
            || bytes.get(index + 1).is_some_and(u8::is_ascii_digit);
        return (!refused).then_some(index);
    }
    None
}

/// Upstream `matchFullwidthCitation`: a `【id】` citation at `i`, its source id and end.
fn match_fullwidth_citation<'t>(
    text: &'t str,
    i: usize,
    opts: &ResolvedOptions,
) -> Option<(&'t str, usize)> {
    let source_ids = opts.source_ids?;
    if !text[i..].starts_with('【') {
        return None;
    }
    let after = i + '【'.len_utf8();
    let close = after + text[after..].find('】')?;
    let id = &text[after..close];
    (!id.is_empty() && source_ids.contains(id)).then_some((id, close + '】'.len_utf8()))
}

/// Upstream `matchBracketCitation`: a `[id]` citation at `i`, never one followed by `(`
/// (a link).
fn match_bracket_citation<'t>(
    text: &'t str,
    i: usize,
    opts: &ResolvedOptions,
) -> Option<(&'t str, usize)> {
    let source_ids = opts.source_ids?;
    if text.as_bytes()[i] != b'[' {
        return None;
    }
    let close = i + 1 + text[i + 1..].find(']')?;
    if text.as_bytes().get(close + 1) == Some(&b'(') {
        return None;
    }
    let id = &text[i + 1..close];
    (!id.is_empty() && source_ids.contains(id)).then_some((id, close + 1))
}

/// Upstream `inlineDestination`: the destination of an inline link or image, without its
/// title. Content in any other shape is all destination.
fn inline_destination(content: &str) -> &str {
    INLINE_DESTINATION_WITH_TITLE
        .captures(content)
        .and_then(|captures| captures.get(1).or_else(|| captures.get(2)))
        .map_or(content, |destination| destination.as_str())
}

/// Upstream `decodeLinkDestination`: a destination with its backslash escapes and
/// character references decoded (CommonMark 0.31 §6.3): `\)` is `)`, `&amp;` is `&`, and
/// an escaped `\&` stays literal. Every URL safety check runs on this decoded value, so
/// an encoded `&#106;avascript:` is refused like the plain one.
pub(super) fn decode_link_destination(raw: &str) -> Cow<'_, str> {
    decode_literal_text(raw)
}

/// An inline link's or image's destination, decoded and normalized.
pub(super) fn destination_url(content: &str) -> String {
    normalized_markdown_destination(&decode_link_destination(inline_destination(content)))
}

/// Upstream `breaksAsLineEndings`: `nodes` with each hard break as the line ending it
/// stands for, so an alt text keeps the words on either side apart.
fn breaks_as_line_endings(nodes: Vec<Phrasing>) -> Vec<Phrasing> {
    nodes
        .into_iter()
        .map(|node| match node {
            Phrasing::Break(_) => text_node("\n"),
            Phrasing::Strong(parent) => Phrasing::Strong(Parent {
                children: breaks_as_line_endings(parent.children),
                ..parent
            }),
            Phrasing::Emphasis(parent) => Phrasing::Emphasis(Parent {
                children: breaks_as_line_endings(parent.children),
                ..parent
            }),
            Phrasing::Delete(parent) => Phrasing::Delete(Parent {
                children: breaks_as_line_endings(parent.children),
                ..parent
            }),
            Phrasing::Link(link) => Phrasing::Link(Link {
                children: breaks_as_line_endings(link.children),
                ..link
            }),
            other => other,
        })
        .collect()
}

/// Upstream `imageAlt`: an image's alt text, its description read as inline content and
/// taken as plain text (CommonMark 0.31 §6.4), so ``![`a]b` *c*](u)`` has the alt `a]b c`.
pub(super) fn image_alt(
    description: &str,
    opts: &ResolvedOptions,
    context: InlineContext,
) -> String {
    if opts.image_alt_depth >= MAX_IMAGE_ALT_DEPTH {
        return decode_literal_text(description).into_owned();
    }
    let deeper = ResolvedOptions {
        image_alt_depth: opts.image_alt_depth + 1,
        ..opts.protected()
    };
    markdown_ast_text(&breaks_as_line_endings(parse_inline_impl(
        description,
        &deeper,
        context,
    )))
}

/// The destination `label` defines, if it is safe.
fn defined_destination<'d>(link_defs: &'d LinkDefinitions, label: &str) -> Option<&'d String> {
    link_defs
        .get(&normalize_link_label(label))
        .filter(|href| is_safe_markdown_parser_url(href))
}

/// Upstream `matchReferenceLink`: a full (`[text][label]`), collapsed (`[text][]`) or
/// shortcut (`[text]`) reference link at `start` (its `[`), and the index past it.
fn match_reference_link(
    text: &str,
    start: usize,
    link_defs: &LinkDefinitions,
    opts: &ResolvedOptions,
    context: InlineContext,
    index: &mut InlineIndex,
) -> Option<(Phrasing, usize)> {
    let text_close = index.opener_close(start)?;
    let link_text = &text[start + 1..text_close];
    // Link text reads as the enclosing text does, without extension syntax.
    let link = |url: &String, end: usize| {
        let children = parse_inline_impl(link_text, &opts.protected(), context);
        (
            Phrasing::Link(Link {
                url: url.clone(),
                children,
                position: None,
                data: None,
            }),
            end,
        )
    };
    // Full or collapsed: a matching definition wins.
    if text.as_bytes().get(text_close + 1) == Some(&b'[')
        && let Some(label_close) = index.closing_bracket(text_close + 2)
    {
        // Only truly empty brackets are the collapsed form; `[ ]` is a full reference whose
        // normalized label is empty and matches nothing.
        let raw_label = &text[text_close + 2..label_close];
        let label = if raw_label.is_empty() {
            link_text
        } else {
            raw_label
        };
        if let Some(href) = defined_destination(link_defs, label) {
            return Some(link(href, label_close + 1));
        }
        // No match: back off to a shortcut `[text]`, leaving `[label]` to parse alone.
    }
    if js::is_blank(link_text) {
        return None;
    }
    defined_destination(link_defs, link_text).map(|href| link(href, text_close + 1))
}

/// Upstream `matchReferenceImage`: the image form of [`match_reference_link`]
/// (`![alt][label]`, `![alt][]`, `![alt]`) at `start` (its `!`).
fn match_reference_image(
    text: &str,
    start: usize,
    link_defs: &LinkDefinitions,
    index: &mut InlineIndex,
    opts: &ResolvedOptions,
    context: InlineContext,
) -> Option<(Phrasing, usize)> {
    let alt_close = index.opener_close(start + 1)?;
    let alt = &text[start + 2..alt_close];
    let image = |url: &String, end: usize| {
        let node = Image {
            url: url.clone(),
            alt: image_alt(alt, opts, context),
            position: None,
            data: None,
        };
        (Phrasing::Image(node), end)
    };
    if text.as_bytes().get(alt_close + 1) == Some(&b'[')
        && let Some(label_close) = index.closing_bracket(alt_close + 2)
    {
        let raw_label = &text[alt_close + 2..label_close];
        let label = if raw_label.is_empty() { alt } else { raw_label };
        if let Some(src) = defined_destination(link_defs, label) {
            return Some(image(src, label_close + 1));
        }
        // No match: back off to a shortcut `![alt]`.
    }
    if js::is_blank(alt) {
        return None;
    }
    defined_destination(link_defs, alt).map(|src| image(src, alt_close + 1))
}

/// Whether the plain-text scan stops at byte `index`: at a character something else may
/// start with.
fn ends_plain_text(bytes: &[u8], index: usize, math: bool) -> bool {
    match bytes[index] {
        b'*' | b'_' | b'~' | b'`' | b'[' | b'!' | b'\\' | b'\n' | b'&' => true,
        b'$' => math,
        _ => bytes[index..].starts_with("【".as_bytes()),
    }
}

/// Upstream `parseInlineEntry`: parses a block's inline text, then applies the GFM
/// autolink pass when it is on. Nested reads (link text, strikethrough) call
/// [`parse_inline_impl`] directly, so the pass runs once, on the outermost tree.
pub(crate) fn parse_inline_entry(
    text: &str,
    opts: &ResolvedOptions,
    context: InlineContext,
) -> Vec<Phrasing> {
    let nodes = parse_inline_impl(text, opts, context);
    if opts.autolink_gfm {
        transform_autolinks(nodes)
    } else {
        nodes
    }
}

/// Upstream `parseInlineImpl`.
fn parse_inline_impl(text: &str, opts: &ResolvedOptions, context: InlineContext) -> Vec<Phrasing> {
    let bytes = text.as_bytes();
    let mut items: Vec<Item> = Vec::new();
    // Runs of `*` and `_`, in order; each is also an item until resolve_emphasis pairs it.
    let mut runs: Vec<DelimiterRun> = Vec::new();
    let mut index = InlineIndex::new(text, opts.link_defs.as_deref());
    // Only a plugin that contributes inline syntax costs anything per position.
    let extension_starts = opts.inline_extension_starts();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        // Escape: a backslash escapes ASCII punctuation, and before a line ending it is a
        // hard break; before anything else it is a literal backslash.
        if byte == b'\\' && i + 1 < bytes.len() {
            let line_ending = line_ending_length_at(bytes, i + 1);
            if line_ending > 0 {
                items.push(Item::Node(Phrasing::Break(Void::default())));
                i += 1 + line_ending;
                continue;
            }
            if is_ascii_punctuation(bytes[i + 1].into()) {
                items.push(Item::Node(text_node(&text[i + 1..i + 2])));
                i += 2;
                continue;
            }
        }

        // Inline code: a backtick string closes at the next backtick string of the same
        // length; with none, it is text (CommonMark 0.31 §6.1).
        if byte == b'`' {
            let open_end = i + bytes[i..].iter().take_while(|&&b| b == b'`').count();
            let ticks = open_end - i;
            let Some(close) = index.backtick_closer(open_end, ticks) else {
                append_inline_text(&mut items, &text[i..open_end]);
                i = open_end;
                continue;
            };
            let code = &text[open_end..close];
            let value = match context {
                InlineContext::TableCell => code.replace("\\|", "|"),
                InlineContext::Default => code.to_string(),
            };
            items.push(Item::Node(Phrasing::InlineCode(Literal {
                value,
                position: None,
                data: None,
            })));
            i = close + ticks;
            continue;
        }

        // Inline math (opt-in; code takes precedence).
        if opts.math
            && byte == b'$'
            && let Some(close) = find_inline_math_end(text, i)
        {
            items.push(Item::Node(Phrasing::InlineMath(Literal {
                value: text[i + 1..close].to_string(),
                position: None,
                data: None,
            })));
            i = close + 1;
            continue;
        }

        // Citation: fullwidth 【id】.
        if let Some((source_id, end)) = match_fullwidth_citation(text, i, opts) {
            items.push(Item::Node(Phrasing::Citation(Citation {
                source_id: source_id.to_string(),
                position: None,
                data: None,
            })));
            i = end;
            continue;
        }

        // Image: ![alt](src).
        if byte == b'!'
            && bytes.get(i + 1) == Some(&b'[')
            && let Some(alt_close) = index.opener_close(i + 1)
            && bytes.get(alt_close + 1) == Some(&b'(')
            && let Some(src_close) = index.destination_close(alt_close + 1)
        {
            let src = destination_url(&text[alt_close + 2..src_close]);
            if is_safe_markdown_parser_url(&src) {
                let alt = image_alt(&text[i + 2..alt_close], opts, context);
                items.push(Item::Node(Phrasing::Image(Image {
                    url: src,
                    alt,
                    position: None,
                    data: None,
                })));
            } else {
                // An unsafe scheme is text.
                items.push(Item::Node(text_node(&text[i..=src_close])));
            }
            i = src_close + 1;
            continue;
        }

        // Reference image: ![alt][label], ![alt][] or ![alt].
        if let Some(link_defs) = opts.link_defs.as_deref()
            && byte == b'!'
            && bytes.get(i + 1) == Some(&b'[')
            && let Some((node, end)) =
                match_reference_image(text, i, link_defs, &mut index, opts, context)
        {
            items.push(Item::Node(node));
            i = end;
            continue;
        }

        // Citation: bracket [id] (before links, which need a `(` after the `]`).
        if let Some((source_id, end)) = match_bracket_citation(text, i, opts) {
            items.push(Item::Node(Phrasing::Citation(Citation {
                source_id: source_id.to_string(),
                position: None,
                data: None,
            })));
            i = end;
            continue;
        }

        // Link: [text](url).
        if byte == b'['
            && let Some(text_close) = index.opener_close(i)
            && bytes.get(text_close + 1) == Some(&b'(')
            && let Some(url_close) = index.destination_close(text_close + 1)
        {
            let href = destination_url(&text[text_close + 2..url_close]);
            if is_safe_markdown_parser_url(&href) {
                // Link text reads as the enclosing text does, without extension syntax.
                let children =
                    parse_inline_impl(&text[i + 1..text_close], &opts.protected(), context);
                items.push(Item::Node(Phrasing::Link(Link {
                    url: href,
                    children,
                    position: None,
                    data: None,
                })));
            } else {
                // An unsafe scheme is text.
                items.push(Item::Node(text_node(&text[i..=url_close])));
            }
            i = url_close + 1;
            continue;
        }

        // Reference link: [text][label], [text][] or [text].
        if let Some(link_defs) = opts.link_defs.as_deref()
            && byte == b'['
            && let Some((node, end)) =
                match_reference_link(text, i, link_defs, opts, context, &mut index)
        {
            items.push(Item::Node(node));
            i = end;
            continue;
        }

        // Emphasis and strong: a run of `*` or `_` is a delimiter run, paired with the
        // others once the whole text is read (CommonMark 0.31 §6.2).
        if byte == b'*' || byte == b'_' {
            let run_end = i + bytes[i..].iter().take_while(|&&b| b == byte).count();
            let run = delimiter_run(byte, text, i, run_end);
            if run.can_open || run.can_close {
                items.push(Item::Run(runs.len()));
                runs.push(run);
                i = run_end;
                continue;
            }
            // A run that can neither open nor close is text; a plugin that starts with it
            // may still read it below.
            if !extension_starts.is_some_and(|starts| starts.contains_key(&char::from(byte))) {
                append_inline_text(&mut items, &text[i..run_end]);
                i = run_end;
                continue;
            }
        }

        // Strikethrough: ~~.
        if byte == b'~'
            && bytes.get(i + 1) == Some(&b'~')
            && let Some(close) = text[i + 2..].find("~~").map(|at| i + 2 + at)
        {
            let children = parse_inline_impl(&text[i + 2..close], opts, context);
            items.push(Item::Node(Phrasing::Delete(Parent {
                children,
                position: None,
                data: None,
            })));
            i = close + 2;
            continue;
        }

        // Extension syntax: built-ins and protected contexts win.
        if extension_starts.is_some() {
            match match_extension_syntax(text, i, ExtensionDisplay::Inline, opts) {
                ExtensionMatch::Match { node, end } => {
                    items.push(Item::Node(Phrasing::Extension(*node)));
                    i = end;
                    continue;
                }
                ExtensionMatch::Defer => break,
                ExtensionMatch::None => {}
            }
        }

        // Character reference: the characters it names, as text.
        if byte == b'&'
            && let Some((value, end)) = match_character_reference(text, i)
        {
            append_inline_text(&mut items, &value);
            i = end;
            continue;
        }

        // Plain text, up to the next character something else may start with.
        let mut end = i + js::char_at(text, i).map_or(1, char::len_utf8);
        while end < bytes.len()
            && !ends_plain_text(bytes, end, opts.math)
            && !extension_starts.is_some_and(|starts| {
                text.is_char_boundary(end)
                    && js::char_at(text, end).is_some_and(|c| starts.contains_key(&c))
            })
        {
            end += 1;
        }
        let content = &text[i..end];

        // Two or more spaces before a line ending are a hard break. A CRLF line keeps its
        // `\r` in the text; it belongs to the line ending, not to the spaces.
        if bytes.get(end) == Some(&b'\n') {
            let line = content.strip_suffix('\r').unwrap_or(content);
            let spaces = line.bytes().rev().take_while(|&b| b == b' ').count();
            if spaces >= 2 {
                let trimmed = &line[..line.len() - spaces];
                if !trimmed.is_empty() {
                    append_inline_text(&mut items, trimmed);
                }
                items.push(Item::Node(Phrasing::Break(Void::default())));
                i = end + 1;
                continue;
            }
        }

        append_inline_text(&mut items, content);
        i = end;
    }

    if runs.is_empty() {
        items
            .into_iter()
            .map(|item| match item {
                Item::Node(node) => node,
                Item::Run(_) => unreachable!("no runs were read"),
            })
            .collect()
    } else {
        resolve_emphasis(items, runs)
    }
}
