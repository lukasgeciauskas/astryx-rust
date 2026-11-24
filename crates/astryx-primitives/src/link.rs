//! Navigation for every component that takes an `href`, upstream
//! `LinkProvider` and `utils/safeUrl.ts`.
//!
//! Upstream swaps the anchor for a router's link component. GPUI has no DOM
//! or router, so the counterpart is a handler that receives the navigation.
//! The default opens absolute URLs with the platform (`App::open_url`); apps
//! with their own routes set one with [`set_link_handler`] or scope one with
//! [`link_provider`].
//!
//! Read the handler during render with [`use_link_handler`] and call it from
//! the click handler.

use std::rc::Rc;

use gpui::{App, Global, IntoElement, SharedString, Window};

use crate::{Provider, provide, use_context};

/// A navigation a link asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Navigation {
    /// The destination, already checked by [`sanitize_url`].
    pub href: SharedString,
    /// Upstream `target`: `_blank` asks for a new window or tab.
    pub target: Option<SharedString>,
}

impl Navigation {
    /// Whether the link asked for a new window, upstream `target="_blank"`.
    pub fn opens_new_window(&self) -> bool {
        self.target.as_deref() == Some("_blank")
    }

    /// Whether the destination is an absolute URL with a scheme, as opposed
    /// to an app route (`/settings`, `#section`).
    pub fn is_absolute(&self) -> bool {
        let href = self.href.as_ref();
        href.split_once(':').is_some_and(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        })
    }
}

/// Receives navigations from links, upstream `LinkProvider`'s `component`.
#[derive(Clone)]
pub struct LinkHandler(Rc<NavigateFn>);

type NavigateFn = dyn Fn(&Navigation, &mut Window, &mut App);

impl LinkHandler {
    /// A handler that calls `navigate`.
    pub fn new(navigate: impl Fn(&Navigation, &mut Window, &mut App) + 'static) -> Self {
        Self(Rc::new(navigate))
    }

    /// The default: absolute URLs open with the platform; app routes, which
    /// only an app's handler understands, are ignored.
    pub fn platform() -> Self {
        Self::new(|navigation, _, cx| {
            if navigation.is_absolute() {
                cx.open_url(&navigation.href);
            }
        })
    }

    /// Navigates to `href`, unless it is unsafe (see [`sanitize_url`]).
    pub fn open(
        &self,
        href: &str,
        target: Option<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(href) = sanitize_url(href) else {
            return;
        };
        (self.0)(
            &Navigation {
                href: href.into(),
                target,
            },
            window,
            cx,
        );
    }
}

struct GlobalLinkHandler(LinkHandler);

impl Global for GlobalLinkHandler {}

/// Sets the app's link handler.
pub fn set_link_handler(handler: LinkHandler, cx: &mut App) {
    cx.set_global(GlobalLinkHandler(handler));
}

/// Sets the link handler for a subtree, upstream `LinkProvider`.
#[track_caller]
pub fn link_provider(handler: LinkHandler, child: impl IntoElement) -> Provider<LinkHandler> {
    provide(handler, child)
}

/// The nearest [`link_provider`]'s handler, else the app's, else
/// [`LinkHandler::platform`]. Read it during render.
pub fn use_link_handler(cx: &App) -> LinkHandler {
    use_context::<LinkHandler>(cx)
        .or_else(|| cx.try_global::<GlobalLinkHandler>().map(|g| g.0.clone()))
        .unwrap_or_else(LinkHandler::platform)
}

/// The URL without control characters and surrounding space, or `None` for
/// a scheme that runs code (`javascript:`, `vbscript:`, `data:text/html`),
/// upstream `sanitizeUrl`.
pub fn sanitize_url(url: &str) -> Option<String> {
    let normalized: String = url
        .chars()
        .filter(|c| !matches!(*c as u32, 0x00..=0x1f | 0x7f))
        .collect::<String>()
        .trim()
        .to_string();
    let mut comparable = normalized.to_lowercase();
    // `javascript:  alert(1)`: spaces after the scheme do not hide it.
    if let Some((scheme, rest)) = comparable.split_once(':') {
        let scheme_ok = scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
        if scheme_ok {
            comparable = format!("{scheme}:{}", rest.trim_start_matches(' '));
        }
    }
    let unsafe_scheme = ["javascript:", "vbscript:", "data:text/html"]
        .iter()
        .any(|prefix| comparable.starts_with(prefix));
    (!unsafe_scheme).then_some(normalized)
}

/// Whether a URL is safe to navigate to, upstream `isSafeUrl`.
pub fn is_safe_url(url: &str) -> bool {
    sanitize_url(url).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_like_upstream() {
        assert_eq!(
            sanitize_url(" https://example.com \n").as_deref(),
            Some("https://example.com")
        );
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url("JavaScript:alert(1)"));
        assert!(!is_safe_url("java\u{0}script:alert(1)"));
        assert!(!is_safe_url("javascript:   alert(1)"));
        assert!(!is_safe_url("data:text/html,<script>"));
        assert!(is_safe_url("data:image/png;base64,AAAA"));
        assert!(is_safe_url("/settings"));
        assert!(is_safe_url("mailto:someone@example.com"));
    }

    #[test]
    fn absolute_urls_have_a_scheme() {
        let nav = |href: &'static str| Navigation {
            href: href.into(),
            target: None,
        };
        assert!(nav("https://example.com").is_absolute());
        assert!(nav("mailto:a@b.c").is_absolute());
        assert!(!nav("/settings").is_absolute());
        assert!(!nav("#top").is_absolute());
        assert!(!nav("?q=1:2").is_absolute());
    }
}
