//! `ChatTypingIndicator`: an animated "X is typing…" hint above a chat composer (upstream
//! lab `ChatTypingIndicator`): three staggered bouncing dots with a grammar-aware label,
//! "Ana is typing…", "Ana and Ben are typing…" or "Ana and 2 others are typing…".
//!
//! The label is announced once per change through the shared polite channel
//! (`astryx_primitives::announce`), unless the indicator sits inside a live container
//! ([`astryx_primitives::provide_live_container`]: a chat log, a status) that delivers it
//! already (spec:AST-086).

use astryx_i18n::{Arg, I18n, current as i18n, use_locale};
use astryx_primitives::{Politeness, announce, in_live_container, prefers_reduced_motion};
use astryx_theme::{CubicBezier, ThemeStyled, current};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, IntoElement, Length, SharedString, StyleRefinement, Window, div, px,
};
use icu_list::ListFormatter;
use icu_list::options::{ListFormatterOptions, ListLength};
use writeable::Writeable as _;

/// The names joined as a long "and" list in `locale` (upstream's
/// `Intl.ListFormat(locale, {style: 'long', type: 'conjunction'})`).
pub fn long_conjunction(locale: &str, parts: &[&str]) -> String {
    let locale: icu_locale::Locale = locale.parse().unwrap_or(icu_locale::Locale::UNKNOWN);
    match ListFormatter::try_new_and(
        (&locale).into(),
        ListFormatterOptions::default().with_length(ListLength::Wide),
    ) {
        Ok(list) => list
            .format(parts.iter().copied())
            .write_to_string()
            .into_owned(),
        Err(_) => parts.join(", "),
    }
}

/// The status sentence for the people typing (upstream `typingLabel`): none without
/// names; three or more collapse to the first name and a translated overflow phrase,
/// joined by the locale's list format.
pub fn typing_label(names: &[SharedString], t: &I18n, locale: &str) -> Option<SharedString> {
    match names.len() {
        0 => None,
        1 => Some(t.format(
            "@astryx.chatTypingIndicator.one",
            &[("name", Arg::from(names[0].as_ref()))],
        )),
        n => {
            let others = (n > 2).then(|| {
                t.format(
                    "@astryx.chatTypingIndicator.others",
                    &[("count", Arg::from(n - 1))],
                )
            });
            let parts: Vec<&str> = match &others {
                Some(others) => vec![names[0].as_ref(), others.as_ref()],
                None => vec![names[0].as_ref(), names[1].as_ref()],
            };
            let joined = long_conjunction(locale, &parts);
            Some(t.format(
                "@astryx.chatTypingIndicator.many",
                &[("names", Arg::from(joined))],
            ))
        }
    }
}

/// One dot's bounce at `elapsed_ms` after the animation started (upstream's keyframes:
/// rest at 0 %, up 3 px and opaque at 30 %, back down at 60 %, resting to 100 %, each
/// stretch on CSS `ease-in-out`), delayed by `delay_ms`: its offset in px and opacity.
/// Before its delay a dot shows its own style, still and opaque.
pub fn dot_frame(elapsed_ms: f64, delay_ms: f64, period_ms: f64) -> (f64, f64) {
    if elapsed_ms < delay_ms || period_ms <= 0. {
        return (0., 1.);
    }
    let p = ((elapsed_ms - delay_ms) % period_ms) / period_ms;
    let ease = CubicBezier(0.42, 0., 0.58, 1.);
    let span = |from: f64, to: f64, a: f64, b: f64| -> f64 {
        let t = f64::from(ease.ease(((p - a) / (b - a)) as f32));
        from + (to - from) * t
    };
    if p < 0.3 {
        (span(0., -3., 0., 0.3), span(0.35, 1., 0., 0.3))
    } else if p < 0.6 {
        (span(-3., 0., 0.3, 0.6), span(1., 0.35, 0.3, 0.6))
    } else {
        (0., 0.35)
    }
}

/// The dots' stagger (upstream `dot2` and `dot3`'s `animationDelay`).
pub const DOT_DELAYS_MS: [f64; 3] = [0., 160., 320.];

/// The animated typing hint (upstream lab `ChatTypingIndicator`).
///
/// ```ignore
/// ChatTypingIndicator::new("typing").names(["Ana", "Ben"])
/// ```
#[derive(IntoElement)]
pub struct ChatTypingIndicator {
    id: ElementId,
    names: Vec<SharedString>,
    dots_column_width: Option<Length>,
    style: StyleRefinement,
}

impl ChatTypingIndicator {
    /// An indicator; `id` keeps its animation clock and what it announced.
    pub fn new(id: impl Into<ElementId>) -> Self {
        ChatTypingIndicator {
            id: id.into(),
            names: Vec::new(),
            dots_column_width: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `names`: the people typing; without them only the dots render.
    pub fn names(mut self, names: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.names = names.into_iter().map(Into::into).collect();
        self
    }

    /// The dots column's width (upstream's `--astryx-chat-typing-indicator-dots-column-width`
    /// custom property; its content's width by default), so a host can line the dots up
    /// under an avatar column as a comment row does.
    pub fn dots_column_width(mut self, width: impl Into<Length>) -> Self {
        self.dots_column_width = Some(width.into());
        self
    }
}

impl Styled for ChatTypingIndicator {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

// What an indicator drew, for tests: its label and its dots; and what it announced.
#[cfg(test)]
thread_local! {
    pub(crate) static TYPING: std::cell::RefCell<std::collections::HashMap<String, (Option<SharedString>, usize)>> = Default::default();
    pub(crate) static ANNOUNCED: std::cell::RefCell<Vec<SharedString>> = Default::default();
}

/// What an indicator keeps: when its animation started and the label it announced last.
struct IndicatorState {
    started: web_time::Instant,
    announced: Option<SharedString>,
}

impl RenderOnce for ChatTypingIndicator {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let locale = use_locale(cx);
        let label = typing_label(&self.names, &i18n(cx), &locale);
        let now = cx.background_executor().now();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-typing", self.id).into()),
            cx,
            move |_, _| IndicatorState {
                started: now,
                announced: None,
            },
        );
        // The label last announced, so a rerender does not repeat it; cleared when nobody
        // is typing, so the same people starting again is a new occasion (FR6).
        let inside_live = in_live_container(cx);
        let announce_now = state.update(cx, |s, _| match &label {
            None => {
                s.announced = None;
                None
            }
            Some(label) if s.announced.as_ref() == Some(label) => None,
            Some(label) => {
                s.announced = Some(label.clone());
                (!inside_live).then(|| label.clone())
            }
        });
        if let Some(text) = announce_now {
            #[cfg(test)]
            ANNOUNCED.with(|a| a.borrow_mut().push(text.clone()));
            announce(text, Politeness::Polite, window, cx);
        }
        #[cfg(test)]
        TYPING.with(|d| {
            d.borrow_mut()
                .insert(self.id.to_string(), (label.clone(), DOT_DELAYS_MS.len()))
        });
        let reduced = prefers_reduced_motion(cx);
        let elapsed = now
            .saturating_duration_since(state.read(cx).started)
            .as_secs_f64()
            * 1000.;
        let period = t.duration.slow.as_secs_f64() * 1000.;
        let dots: Vec<AnyElement> = DOT_DELAYS_MS
            .iter()
            .enumerate()
            .map(|(i, delay)| {
                // Under Reduce Motion the dots hold still.
                let (y, opacity) = if reduced {
                    (0., 1.)
                } else {
                    dot_frame(elapsed, *delay, period)
                };
                let selector = format!("{}-dot-{i}", self.id);
                div()
                    .debug_selector(move || selector)
                    .size(px(5.))
                    .rounded_full()
                    .bg(t.color.icon_secondary)
                    .translate_y(px(y as f32))
                    .opacity(opacity as f32)
                    .into_any_element()
            })
            .collect();
        if !reduced {
            window.request_animation_frame();
        }
        let mut root = div().flex().items_center().gap(t.spacing.s2).min_h(px(20.));
        theme.refine(root.style(), "chat-typing-indicator", &[], None);
        root.style().refine(&self.style);
        let selector = format!("{}-typing-indicator", self.id);
        root.debug_selector(move || selector)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .when_some(self.dots_column_width, |d, w| d.w(w))
                    .gap(t.spacing.s0_5)
                    .inert(true)
                    .children(dots),
            )
            .children(label.map(|l| {
                div()
                    .font_stack(&t.font.family.body)
                    .text_size(t.text.supporting.size)
                    .text_color(t.color.text_secondary)
                    .child(l)
            }))
    }
}
