//! Time: Timer and Timestamp. Upstream: `Timer/`, `Timestamp/`.

pub mod examples;
mod timestamp;

pub use timestamp::{Timestamp, TimestampFormat, TimestampValue};

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use astryx_i18n::{DurationUnit, current as i18n};
use astryx_theme::current;
use gpui::{
    App, ElementId, IntoElement, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window,
    prelude::*,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::text::{Text, TextColor, TextSize, TextType, TextWeight};

/// Upstream `Timer` `format`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimerFormat {
    /// Compact units: "45s", "3m 05s", "1h 02m".
    #[default]
    Elapsed,
    /// Stopwatch notation: "3:05", "1:02:09".
    Clock,
}

/// The text a timer shows `elapsed` after its start, upstream
/// `getPresentation`.
pub fn timer_text(elapsed: Duration, format: TimerFormat, i18n: &astryx_i18n::I18n) -> String {
    let seconds = elapsed.as_secs();
    match format {
        TimerFormat::Clock => {
            let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
            if h > 0 {
                format!("{h}:{m:02}:{s:02}")
            } else {
                format!("{m}:{s:02}")
            }
        }
        TimerFormat::Elapsed => {
            let info = i18n.info();
            if seconds < 60 {
                info.elapsed_duration(&[(DurationUnit::Second, seconds)])
            } else if seconds < 3600 {
                info.elapsed_duration(&[
                    (DurationUnit::Minute, seconds / 60),
                    (DurationUnit::Second, seconds % 60),
                ])
            } else {
                let minutes = seconds / 60;
                info.elapsed_duration(&[
                    (DurationUnit::Hour, minutes / 60),
                    (DurationUnit::Minute, minutes % 60),
                ])
            }
        }
    }
}

/// How long until the text changes, upstream `getMillisecondsUntilNextChange`:
/// each second, or each minute once an elapsed timer passes an hour.
pub fn until_next_change(elapsed: Duration, format: TimerFormat) -> Duration {
    let precision = if format == TimerFormat::Elapsed && elapsed >= Duration::from_secs(3600) {
        60_000
    } else {
        1_000
    };
    Duration::from_millis(precision - (elapsed.as_millis() as u64 % precision))
}

fn unix_ms(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}

struct TimerState {
    mounted_ms: i64,
    scheduled: bool,
}

/// Elapsed time since a start, updating itself, upstream `Timer`.
#[derive(IntoElement)]
pub struct Timer {
    id: ElementId,
    start_time: Option<i64>,
    format: TimerFormat,
    text_type: TextType,
    size: Option<TextSize>,
    color: TextColor,
    weight: Option<TextWeight>,
    style: StyleRefinement,
}

impl Timer {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Timer {
            id: id.into(),
            start_time: None,
            format: TimerFormat::Elapsed,
            text_type: TextType::Supporting,
            size: None,
            color: TextColor::Secondary,
            weight: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `startTime`: Unix milliseconds. Defaults to when the timer first draws.
        ?start_time: i64,
        /// Upstream `format`.
        format: TimerFormat,
        /// Upstream `type`.
        text_type: TextType,
        /// Upstream `size`.
        ?size: TextSize,
        /// Upstream `color`.
        color: TextColor,
        /// Upstream `weight`.
        ?weight: TextWeight,
    }
}

impl Styled for Timer {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Timer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let _theme = current(window, cx);
        let i18n = i18n(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| TimerState {
            mounted_ms: unix_ms(SystemTime::now()),
            scheduled: false,
        });
        let now = unix_ms(SystemTime::now());
        let start = self.start_time.unwrap_or(state.read(cx).mounted_ms);
        let elapsed = Duration::from_millis((now - start).max(0) as u64);
        let text: SharedString = timer_text(elapsed, self.format, &i18n).into();

        // One refresh at the next visible change, not every frame.
        if !state.read(cx).scheduled {
            state.update(cx, |s, _| s.scheduled = true);
            let wait = if now < start {
                Duration::from_millis((start - now) as u64 + 1000)
            } else {
                until_next_change(elapsed, self.format)
            };
            let state = state.clone();
            window
                .spawn(cx, async move |cx| {
                    cx.background_executor().timer(wait).await;
                    cx.update(|window, cx| {
                        state.update(cx, |s, _| s.scheduled = false);
                        window.refresh();
                    })
                    .ok();
                })
                .detach();
        }

        let mut text_el = Text::new(text.clone())
            .text_type(self.text_type)
            .color(self.color)
            .tabular_numbers(true);
        if let Some(size) = self.size {
            text_el = text_el.size(size);
        }
        if let Some(weight) = self.weight {
            text_el = text_el.weight(weight);
        }
        let mut el = gpui::div().id(self.id).role(Role::Timer).aria_label(text);
        el.style().refine(&self.style);
        el.child(text_el)
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TIMESTAMP_DOC: ComponentDoc = ComponentDoc {
    name: "Timestamp",
    rust_type: "astryx::components::time::Timestamp",
    props: &[
        p("value", RustApi::Argument("value")),
        p("format", RustApi::Method("format")),
        p("timezoneID", RustApi::Method("timezone_id")),
        p("autoThreshold", RustApi::Method("auto_threshold")),
        p(
            "hasTooltip",
            RustApi::Pending(
                "has_tooltip",
                "the copyable hover card lands with HoverCard in M5",
            ),
        ),
        p(
            "tooltipEntries",
            RustApi::Pending(
                "tooltip_entries",
                "the hover card lands with HoverCard in M5",
            ),
        ),
        p("isTimezoneShown", RustApi::Method("timezone_shown")),
        p("isLive", RustApi::Method("live")),
        p("type", RustApi::Method("text_type")),
        p("size", RustApi::Method("size")),
        p("color", RustApi::Method("color")),
        p("weight", RustApi::Method("weight")),
    ],
    example: include_str!("examples.rs"),
};

pub const TIMER_DOC: ComponentDoc = ComponentDoc {
    name: "Timer",
    rust_type: "astryx::components::time::Timer",
    props: &[
        p("startTime", RustApi::Method("start_time")),
        p("format", RustApi::Method("format")),
        p("type", RustApi::Method("text_type")),
        p("size", RustApi::Method("size")),
        p("color", RustApi::Method("color")),
        p("weight", RustApi::Method("weight")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_text_follows_upstream() {
        let en = astryx_i18n::english();
        let at = |s: u64, f| timer_text(Duration::from_secs(s), f, &en);
        assert_eq!(at(45, TimerFormat::Elapsed), "45s");
        assert_eq!(at(185, TimerFormat::Elapsed), "3m 05s");
        assert_eq!(at(3729, TimerFormat::Elapsed), "1h 02m");
        assert_eq!(at(185, TimerFormat::Clock), "3:05");
        assert_eq!(at(3729, TimerFormat::Clock), "1:02:09");
    }

    #[test]
    fn updates_every_second_then_every_minute() {
        assert_eq!(
            until_next_change(Duration::from_millis(1_250), TimerFormat::Elapsed),
            Duration::from_millis(750)
        );
        assert_eq!(
            until_next_change(Duration::from_secs(3_605), TimerFormat::Elapsed),
            Duration::from_secs(55)
        );
        assert_eq!(
            until_next_change(Duration::from_secs(3_605), TimerFormat::Clock),
            Duration::from_secs(1)
        );
    }
}
