//! Link and image destination safety, upstream `Markdown/url.ts`, with the shared
//! navigation rule it builds on (`utils/safeUrl.ts`, `sanitizeUrl`). The parser decodes a
//! destination before these checks.

use crate::js;

/// Upstream `normalizeUrl`: without the control characters browsers ignore, and trimmed.
fn normalize_url(url: &str) -> String {
    let kept: String = url
        .chars()
        .filter(|c| !matches!(c, '\u{0}'..='\u{1F}' | '\u{7F}'))
        .collect();
    js::trim(&kept).to_string()
}

/// Upstream `comparableUrl`: lowercase, without the spaces after the scheme's colon,
/// which a data URL's media type ignores (`data: text/html` is HTML).
fn comparable_url(normalized: &str) -> String {
    let lower = normalized.to_lowercase();
    // `^([a-z][a-z\d+.-]*:) +` becomes `$1`.
    let bytes = lower.as_bytes();
    if bytes.first().is_some_and(u8::is_ascii_lowercase) {
        let scheme_end = 1 + bytes[1..]
            .iter()
            .take_while(|&&b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'+' | b'.' | b'-')
            })
            .count();
        if bytes.get(scheme_end) == Some(&b':') {
            let spaces = bytes[scheme_end + 1..]
                .iter()
                .take_while(|&&b| b == b' ')
                .count();
            if spaces > 0 {
                return format!(
                    "{}{}",
                    &lower[..=scheme_end],
                    &lower[scheme_end + 1 + spaces..]
                );
            }
        }
    }
    lower
}

/// Upstream `sanitizeUrl`: the normalized URL, or `None` for a blocked scheme.
fn sanitize_url(url: &str) -> Option<String> {
    let normalized = normalize_url(url);
    let lower = comparable_url(&normalized);
    let blocked = lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("data:text/html");
    (!blocked).then_some(normalized)
}

/// Upstream `DATA_SCHEME_PATTERN` (`/^data:/i`): Markdown blocks every `data:` URL.
fn is_data_url(normalized: &str) -> bool {
    normalized
        .as_bytes()
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case(b"data:"))
}

/// Upstream `isSafeMarkdownParserUrl`: whether the parser keeps a link or image
/// destination. The shared navigation rule accepts it, and it is not a `data:` URL.
pub fn is_safe_markdown_parser_url(url: &str) -> bool {
    sanitize_url(url).is_some_and(|normalized| !is_data_url(&normalized))
}

/// Upstream `normalizedMarkdownDestination`: the destination a link or image node
/// stores, the shared rule's normalized form (control characters and outer whitespace
/// removed). A blocked destination comes back unchanged, for the safety check to reject.
pub fn normalized_markdown_destination(url: &str) -> String {
    sanitize_url(url).unwrap_or_else(|| url.to_string())
}

/// Upstream `sanitizeMarkdownLinkUrl`: a navigation URL that does not come from parsed
/// source, such as a plugin's base URL, checked with the parser's rule. The normalized
/// URL, or `None`.
pub fn sanitize_markdown_link_url(url: &str) -> Option<String> {
    sanitize_url(url).filter(|normalized| !normalized.is_empty() && !is_data_url(normalized))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_script_and_data_schemes() {
        assert!(is_safe_markdown_parser_url("https://example.com"));
        assert!(is_safe_markdown_parser_url("/relative"));
        assert!(!is_safe_markdown_parser_url(" JavaScript:alert(1)"));
        assert!(!is_safe_markdown_parser_url("java\u{0}script:alert(1)"));
        assert!(!is_safe_markdown_parser_url("data: text/html,x"));
        assert!(!is_safe_markdown_parser_url("DATA:image/png;base64,x"));
        assert!(!is_safe_markdown_parser_url("vbscript:x"));
    }

    #[test]
    fn normalizes_destinations() {
        assert_eq!(normalized_markdown_destination("  /a\u{7}b  "), "/ab");
        assert_eq!(
            normalized_markdown_destination("javascript:x"),
            "javascript:x"
        );
        assert_eq!(sanitize_markdown_link_url("  "), None);
        assert_eq!(sanitize_markdown_link_url("data:image/png,x"), None);
        assert_eq!(
            sanitize_markdown_link_url(" https://a.b/ ").as_deref(),
            Some("https://a.b/")
        );
    }
}
