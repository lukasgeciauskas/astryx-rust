//! GFM autolinks, upstream `Markdown/parser.ts` ("GFM autolink post-pass", on with
//! `autolink: 'gfm'`): bare `http(s)://` and `www.` URLs, `<scheme:url>` and `<email>`,
//! and bare emails become links. It runs on a block's finished inline tree.

use std::sync::LazyLock;

use regex::Regex;

use crate::ast::{Link, Literal, Phrasing};
use crate::js;
use crate::url::{is_safe_markdown_parser_url, normalized_markdown_destination};

/// Compiles an autolink pattern, `{ws}` standing for JavaScript's `\s`.
fn autolink_regex(pattern: &str) -> Regex {
    Regex::new(&pattern.replace("{ws}", js::WHITESPACE_CLASS)).expect("autolink patterns are valid")
}

/// Upstream `ANGLE_URL_RE`: `<scheme:url>` (CommonMark §6.5).
static ANGLE_URL: LazyLock<Regex> =
    LazyLock::new(|| autolink_regex(r"<([a-zA-Z][a-zA-Z0-9+.-]*:[^<>{ws}]*)>"));

/// Upstream `ANGLE_EMAIL_RE`: `<user@host.tld>`.
static ANGLE_EMAIL: LazyLock<Regex> =
    LazyLock::new(|| autolink_regex(r"<([A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+)>"));

/// Upstream `URL_LITERAL_RE`: a bare http(s) URL up to whitespace or `<`; trailing
/// punctuation is peeled afterwards.
static URL_LITERAL: LazyLock<Regex> = LazyLock::new(|| autolink_regex(r"https?://[^{ws}<]+"));

/// Upstream `WWW_LITERAL_RE`: a bare `www.` URL; its link gets `http://` prepended.
static WWW_LITERAL: LazyLock<Regex> = LazyLock::new(|| autolink_regex(r"www\.[^{ws}<]+"));

/// Upstream `EMAIL_DOMAIN_RE` (sticky): a bare email's domain, with at least one dot,
/// matched just after its `@`.
static EMAIL_DOMAIN: LazyLock<Regex> =
    LazyLock::new(|| autolink_regex(r"^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+"));

/// Upstream `TRAILING_PUNCT_CHARS`: trailing sentence punctuation (GFM §6.9).
fn is_trailing_punctuation(byte: u8) -> bool {
    b"?!.,:*_~".contains(&byte)
}

/// Upstream `EMAIL_LOCAL_PART_CHARS`: what an email's local part may hold.
fn is_email_local_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"._%+-".contains(&byte)
}

/// Upstream `EMAIL_LOCAL_CHARS`: a local part's characters and `@`, which before a match
/// make it the tail of something bigger.
fn is_email_local(byte: u8) -> bool {
    is_email_local_part(byte) || byte == b'@'
}

/// Upstream `isUrlBoundaryChar` (with `URL_CONTINUATION_CHARS`): whether the byte before
/// a bare URL leaves it a token of its own (`xhttps`, `=https://` and `/https://` do not).
fn is_url_boundary(before: Option<u8>) -> bool {
    before.is_none_or(|byte| !(byte.is_ascii_alphanumeric() || byte == b'/' || byte == b'='))
}

/// Upstream `AutolinkMatch`.
struct AutolinkMatch {
    start: usize,
    end: usize,
    href: String,
    display: String,
}

/// Upstream `peelTrailingPunctAndParens`: a bare URL without trailing sentence
/// punctuation or unbalanced trailing `)` (GFM §6.9), so `(https://example.com)` ends
/// before the `)` but `https://example.com/Foo_(bar)` keeps it.
fn peel_trailing_punct_and_parens(url: &str) -> &str {
    let bytes = url.as_bytes();
    let open = bytes.iter().filter(|&&b| b == b'(').count();
    let mut close = bytes.iter().filter(|&&b| b == b')').count();
    let mut end = bytes.len();
    while end > 0 {
        let byte = bytes[end - 1];
        if is_trailing_punctuation(byte) {
            end -= 1;
        } else if byte == b')' && close > open {
            close -= 1;
            end -= 1;
        } else {
            break;
        }
    }
    &url[..end]
}

/// Whether the bytes of `text` just before `end` are `prefix`.
fn preceded_by(text: &str, end: usize, prefix: &str) -> bool {
    end >= prefix.len() && &text.as_bytes()[end - prefix.len()..end] == prefix.as_bytes()
}

/// Upstream `scanAutolinksInText`: the autolinks of one text node, by start; on overlap
/// the first wins, so `<https://x>` outranks the bare `https://x` inside it.
fn scan_autolinks_in_text(text: &str) -> Vec<AutolinkMatch> {
    let bytes = text.as_bytes();
    let mut matches = Vec::new();

    for captures in ANGLE_URL.captures_iter(text) {
        let (whole, url) = (captures.get(0).expect("a match has group 0"), &captures[1]);
        // Unsafe schemes (javascript:, vbscript:, data:) stay text.
        if !is_safe_markdown_parser_url(url) {
            continue;
        }
        matches.push(AutolinkMatch {
            start: whole.start(),
            end: whole.end(),
            href: url.to_string(),
            display: url.to_string(),
        });
    }

    for captures in ANGLE_EMAIL.captures_iter(text) {
        let (whole, email) = (captures.get(0).expect("a match has group 0"), &captures[1]);
        matches.push(AutolinkMatch {
            start: whole.start(),
            end: whole.end(),
            href: format!("mailto:{email}"),
            display: email.to_string(),
        });
    }

    for found in URL_LITERAL.find_iter(text) {
        let start = found.start();
        if !is_url_boundary(start.checked_sub(1).map(|i| bytes[i]))
            || preceded_by(text, start, "](")
        {
            continue;
        }
        let cleaned = peel_trailing_punct_and_parens(found.as_str());
        if !cleaned.is_empty() {
            matches.push(AutolinkMatch {
                start,
                end: start + cleaned.len(),
                href: cleaned.to_string(),
                display: cleaned.to_string(),
            });
        }
    }

    for found in WWW_LITERAL.find_iter(text) {
        let start = found.start();
        // Not inside an `https://www…` the previous pattern took.
        if !is_url_boundary(start.checked_sub(1).map(|i| bytes[i]))
            || preceded_by(text, start, "://")
        {
            continue;
        }
        let cleaned = peel_trailing_punct_and_parens(found.as_str());
        if !cleaned.is_empty() {
            matches.push(AutolinkMatch {
                start,
                end: start + cleaned.len(),
                href: format!("http://{cleaned}"),
                display: cleaned.to_string(),
            });
        }
    }

    // Bare emails, found from each `@`. A match, accepted or not, ends where the next one
    // may start.
    let mut consumed_to = 0;
    for at in text.match_indices('@').map(|(at, _)| at) {
        let mut start = at;
        while start > consumed_to && is_email_local_part(bytes[start - 1]) {
            start -= 1;
        }
        if start == at {
            continue;
        }
        let Some(domain) = EMAIL_DOMAIN.find(&text[at + 1..]) else {
            continue;
        };
        let end = at + 1 + domain.end();
        consumed_to = end;
        // Not the continuation of something bigger (`name@x.y@host`, `foo+bar@x`,
        // `mailto:user@host`).
        if start
            .checked_sub(1)
            .is_some_and(|i| is_email_local(bytes[i]))
            || &bytes[start.saturating_sub(7)..start] == b"mailto:"
        {
            continue;
        }
        let email = &text[start..end];
        matches.push(AutolinkMatch {
            start,
            end,
            href: format!("mailto:{email}"),
            display: email.to_string(),
        });
    }

    matches.sort_by_key(|m| m.start);
    let mut resolved: Vec<AutolinkMatch> = Vec::new();
    let mut last_end = 0;
    for m in matches {
        if m.start >= last_end {
            last_end = m.end;
            resolved.push(m);
        }
    }
    resolved
}

fn text_node(value: &str) -> Phrasing {
    Phrasing::Text(Literal {
        value: value.to_string(),
        position: None,
        data: None,
    })
}

/// Upstream `splitTextOnAutolinks`: a text node's content as text and link nodes.
fn split_text_on_autolinks(content: &str) -> Vec<Phrasing> {
    let matches = scan_autolinks_in_text(content);
    if matches.is_empty() {
        return vec![text_node(content)];
    }
    let mut out = Vec::new();
    let mut cursor = 0;
    for m in matches {
        if m.start > cursor {
            out.push(text_node(&content[cursor..m.start]));
        }
        out.push(Phrasing::Link(Link {
            url: normalized_markdown_destination(&m.href),
            children: vec![text_node(&m.display)],
            position: None,
            data: None,
        }));
        cursor = m.end;
    }
    if cursor < content.len() {
        out.push(text_node(&content[cursor..]));
    }
    out
}

/// Upstream `transformAutolinks`: bare URLs and emails in text nodes become links. It
/// descends into strong, emphasis and delete, never into links (links do not nest), code,
/// images, citations or breaks. Adjacent text it leaves is joined.
pub(super) fn transform_autolinks(nodes: Vec<Phrasing>) -> Vec<Phrasing> {
    let mut out: Vec<Phrasing> = Vec::new();
    for node in nodes {
        match node {
            Phrasing::Text(text) => {
                for segment in split_text_on_autolinks(&text.value) {
                    if let (Phrasing::Text(segment), Some(Phrasing::Text(last))) =
                        (&segment, out.last_mut())
                    {
                        last.value.push_str(&segment.value);
                    } else {
                        out.push(segment);
                    }
                }
            }
            Phrasing::Strong(mut parent) => {
                parent.children = transform_autolinks(parent.children);
                out.push(Phrasing::Strong(parent));
            }
            Phrasing::Emphasis(mut parent) => {
                parent.children = transform_autolinks(parent.children);
                out.push(Phrasing::Emphasis(parent));
            }
            Phrasing::Delete(mut parent) => {
                parent.children = transform_autolinks(parent.children);
                out.push(Phrasing::Delete(parent));
            }
            other => out.push(other),
        }
    }
    out
}
