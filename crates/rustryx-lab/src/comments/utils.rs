//! The Comments family's pure helpers (upstream lab `Comments/utils.ts`): element ids,
//! Markdown serialization of images, draft appends, timeline ordering, and how a
//! press was made.

use rustryx_charts::format::date::date_parse;
use rustryx_theme::authoring::js::{is_js_whitespace, js_trim};

use super::types::{CommentTime, CommentUploadedImage};

/// JavaScript's `encodeURIComponent`: every UTF-8 byte except the unreserved
/// `A–Z a–z 0–9 - _ . ! ~ * ' ( )` as `%XX`.
pub fn encode_uri_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        let unreserved = byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte);
        if unreserved {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Upstream `commentAnchorId`: the element id a comment's permalink points at.
pub fn comment_anchor_id(comment_id: &str) -> String {
    format!("comment-{}", encode_uri_component(comment_id))
}

/// Upstream `replyComposerId`: the element id of a thread's reply composer.
pub fn reply_composer_id(parent_id: &str) -> String {
    format!("{}-reply-composer", comment_anchor_id(parent_id))
}

/// Upstream `eventAnchorId`: the element id of an activity event row.
pub fn event_anchor_id(event_id: &str) -> String {
    format!("comments-event-{}", encode_uri_component(event_id))
}

/// Upstream `safeImageAlt`: alt text that cannot break out of Markdown's image
/// brackets; `fallback` (already localized) stands in for an image with none.
pub fn safe_image_alt(alt: &str, fallback: &str) -> String {
    // /[[\]\r\n]+/g → ' '
    let mut replaced = String::with_capacity(alt.len());
    let mut in_run = false;
    for c in alt.chars() {
        if matches!(c, '[' | ']' | '\r' | '\n') {
            if !in_run {
                replaced.push(' ');
                in_run = true;
            }
        } else {
            replaced.push(c);
            in_run = false;
        }
    }
    let trimmed = js_trim(&replaced);
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Upstream `safeMarkdownImageUrl`: a URL that cannot break out of Markdown's image
/// parentheses.
pub fn safe_markdown_image_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for c in js_trim(url).chars() {
        match c {
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            c if is_js_whitespace(c) => {
                out.push_str(&encode_uri_component(c.encode_utf8(&mut [0; 4])))
            }
            c => out.push(c),
        }
    }
    out
}

/// Upstream `imagesToMarkdown`: uploaded images as Markdown, one paragraph each.
/// `fallback_alt` (already localized) names an image uploaded without alt text.
pub fn images_to_markdown(images: &[CommentUploadedImage], fallback_alt: &str) -> String {
    images
        .iter()
        .map(|image| {
            format!(
                "![{}]({})",
                safe_image_alt(&image.alt, fallback_alt),
                safe_markdown_image_url(&image.url)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// JavaScript's `trimEnd`.
fn js_trim_end(text: &str) -> &str {
    text.trim_end_matches(is_js_whitespace)
}

/// Upstream `appendDraftBlock`: appends a block (an image, say) to a draft as its own
/// paragraph.
pub fn append_draft_block(value: &str, block: &str) -> String {
    let draft = js_trim_end(value);
    if draft.is_empty() {
        block.to_string()
    } else {
        format!("{draft}\n\n{block}")
    }
}

/// Upstream `appendDraftInline`: appends inline content (an emoji, say) to a draft,
/// spaced from its text.
pub fn append_draft_inline(value: &str, content: &str) -> String {
    let ends_spaced = value.chars().next_back().is_some_and(is_js_whitespace);
    let starts_spaced = content.chars().next().is_some_and(is_js_whitespace);
    if value.is_empty() || ends_spaced || starts_spaced {
        format!("{value}{content}")
    } else {
        format!("{value} {content}")
    }
}

/// Upstream `timelineTime`: a `created_at` value as epoch milliseconds; seconds (a
/// number below 10^12) are scaled up, and an unreadable string is 0.
pub fn timeline_time(value: &CommentTime) -> f64 {
    match value {
        CommentTime::Number(n) => {
            if *n < 1_000_000_000_000. {
                n * 1_000.
            } else {
                *n
            }
        }
        CommentTime::Text(text) => {
            let parsed = date_parse(text);
            if parsed.is_nan() { 0. } else { parsed }
        }
    }
}

/// Upstream `ActivationSource`: a pointer press opens a thread into its reply
/// composer; a key press keeps focus where it is, so the same key can close it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationSource {
    Pointer,
    Keyboard,
}

/// Upstream `activationSourceOf`: keyboard and assistive-technology clicks carry no
/// click count.
pub fn activation_source_of(event: &gpui::ClickEvent) -> ActivationSource {
    if event.is_keyboard() {
        ActivationSource::Keyboard
    } else {
        ActivationSource::Pointer
    }
}
