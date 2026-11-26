//! Heading slugs, upstream `Markdown/parser.ts` (`slugify`, `uniqueSlug`): the heading id
//! contract Markdown renders as `id`s and Outline derives its item ids from.

use std::collections::HashMap;

use crate::js;

/// Upstream `slugify`: heading text as a URL-safe slug, lowercase and hyphen-separated.
/// Quotes go; every other run of characters but `a-z` and `0-9` becomes one `-`, and
/// none opens or ends the slug.
pub fn slugify(value: &str) -> String {
    let lower = js::trim(value).to_lowercase();
    let mut slug = String::with_capacity(lower.len());
    let mut gap = false;
    for c in lower.chars().filter(|&c| c != '\'' && c != '"') {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if gap && !slug.is_empty() {
                slug.push('-');
            }
            gap = false;
            slug.push(c);
        } else {
            gap = true;
        }
    }
    slug
}

/// Upstream `uniqueSlug`: `base_slug` made unique with a numeric suffix (`setup`,
/// `setup-1`, …). An empty slug falls back to `section`. The caller owns `counts`, so one
/// document shares one numbering.
pub fn unique_slug(base_slug: &str, counts: &mut HashMap<String, usize>) -> String {
    let slug = if base_slug.is_empty() {
        "section"
    } else {
        base_slug
    };
    let count = counts.entry(slug.to_string()).or_insert(0);
    *count += 1;
    if *count == 1 {
        slug.to_string()
    } else {
        format!("{slug}-{}", *count - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_lowercase_and_hyphenated() {
        assert_eq!(slugify("  Hello, World! "), "hello-world");
        assert_eq!(slugify("It's \"quoted\""), "its-quoted");
        assert_eq!(slugify("--a--b--"), "a-b");
    }

    #[test]
    fn repeated_slugs_get_suffixes() {
        let mut counts = HashMap::new();
        assert_eq!(unique_slug("setup", &mut counts), "setup");
        assert_eq!(unique_slug("setup", &mut counts), "setup-1");
        assert_eq!(unique_slug("", &mut counts), "section");
        assert_eq!(unique_slug("section", &mut counts), "section-1");
    }
}
