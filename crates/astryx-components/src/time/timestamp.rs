//! Timestamp. Upstream: `Timestamp/`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use astryx_i18n::datetime::parse_iso;
use astryx_i18n::{InstantFormat, TimeZoneChoice, current as i18n};
use gpui::{
    App, ElementId, IntoElement, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window,
    div, prelude::*,
};

use crate::text::{Text, TextColor, TextSize, TextType, TextWeight};

/// Upstream `value`: unix seconds or an ISO 8601 string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimestampValue {
    Unix(i64),
    Iso(SharedString),
}

impl TimestampValue {
    fn unix_seconds(&self) -> Option<i64> {
        match self {
            TimestampValue::Unix(seconds) => Some(*seconds),
            TimestampValue::Iso(iso) => parse_iso(iso),
        }
    }
}

impl From<i64> for TimestampValue {
    fn from(seconds: i64) -> Self {
        TimestampValue::Unix(seconds)
    }
}

impl From<&'static str> for TimestampValue {
    fn from(iso: &'static str) -> Self {
        TimestampValue::Iso(iso.into())
    }
}

/// Upstream `format`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimestampFormat {
    /// "2 hours ago", "yesterday", "now".
    Relative,
    /// "2h ago".
    RelativeShort,
    /// Relative when recent, `DateTime` once older than the threshold.
    #[default]
    Auto,
    Date,
    DateLong,
    DateWeekday,
    DateTime,
    Time,
}

impl TimestampFormat {
    fn as_str(self) -> &'static str {
        match self {
            TimestampFormat::Relative => "relative",
            TimestampFormat::RelativeShort => "relative_short",
            TimestampFormat::Auto => "auto",
            TimestampFormat::Date => "date",
            TimestampFormat::DateLong => "date_long",
            TimestampFormat::DateWeekday => "date_weekday",
            TimestampFormat::DateTime => "date_time",
            TimestampFormat::Time => "time",
        }
    }
}

fn now_seconds() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => 0,
    }
}

/// How long until a relative time's text changes: each second under a
/// minute, then at each minute, hour or day boundary.
pub fn until_relative_change(ago: i64) -> Duration {
    let ago = ago.unsigned_abs();
    let step = if ago < 60 {
        1
    } else if ago < 3600 {
        60
    } else if ago < 86_400 {
        3600
    } else {
        86_400
    };
    Duration::from_secs(step - ago % step)
}

#[derive(Default)]
struct LiveState {
    scheduled: bool,
}

/// A formatted instant, upstream `Timestamp`.
#[derive(IntoElement)]
pub struct Timestamp {
    id: ElementId,
    value: TimestampValue,
    format: TimestampFormat,
    timezone: TimeZoneChoice,
    auto_threshold: i64,
    has_tooltip: bool,
    timezone_shown: bool,
    live: bool,
    text_type: TextType,
    size: Option<TextSize>,
    color: TextColor,
    weight: Option<TextWeight>,
    style: StyleRefinement,
}

impl Timestamp {
    /// Upstream `value`.
    pub fn new(id: impl Into<ElementId>, value: impl Into<TimestampValue>) -> Self {
        Timestamp {
            id: id.into(),
            value: value.into(),
            format: TimestampFormat::Auto,
            timezone: TimeZoneChoice::Local,
            auto_threshold: 604_800,
            has_tooltip: true,
            timezone_shown: false,
            live: false,
            text_type: TextType::Supporting,
            size: None,
            color: TextColor::Secondary,
            weight: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `format`.
        format: TimestampFormat,
        /// Upstream `autoThreshold`: seconds before `auto` switches to a date.
        auto_threshold: i64,
        /// Upstream `hasTooltip`. The hover card lands with HoverCard (M5).
        !has_tooltip,
        /// Upstream `isTimezoneShown`: the zone after `date_time` and `time`.
        !timezone_shown,
        /// Upstream `isLive`: relative text keeps itself current.
        !live,
        /// Upstream `type`.
        text_type: TextType,
        /// Upstream `size`.
        ?size: TextSize,
        /// Upstream `color`.
        color: TextColor,
        /// Upstream `weight`.
        ?weight: TextWeight,
    }

    /// Upstream `timezoneID`: an IANA zone, or `"local"`.
    pub fn timezone_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.timezone = if id == "local" {
            TimeZoneChoice::Local
        } else {
            TimeZoneChoice::Iana(id)
        };
        self
    }
}

impl Styled for Timestamp {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Timestamp {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let i18n = i18n(cx);
        let Some(seconds) = self.value.unix_seconds() else {
            return div().into_any_element();
        };
        let now = now_seconds();
        let ago = now - seconds;
        let effective = match self.format {
            TimestampFormat::Auto if ago.abs() < self.auto_threshold => TimestampFormat::Relative,
            TimestampFormat::Auto => TimestampFormat::DateTime,
            other => other,
        };
        let absolute =
            |format| i18n.format_instant(seconds, format, &self.timezone, self.timezone_shown);
        let relative = matches!(
            effective,
            TimestampFormat::Relative | TimestampFormat::RelativeShort
        );
        let text: SharedString = match effective {
            TimestampFormat::Relative => i18n.format_relative(ago, false),
            TimestampFormat::RelativeShort => i18n.format_relative(ago, true),
            TimestampFormat::Date => absolute(InstantFormat::Date),
            TimestampFormat::DateLong => absolute(InstantFormat::DateLong),
            TimestampFormat::DateWeekday => absolute(InstantFormat::DateWeekday),
            TimestampFormat::DateTime => absolute(InstantFormat::DateTime),
            TimestampFormat::Time => absolute(InstantFormat::Time),
            TimestampFormat::Auto => unreachable!("resolved above"),
        }
        .into();
        // A relative time's accessible name adds the full date with the
        // zone spelled out, upstream `@astryx.timestamp.relativeLabel`.
        let name: SharedString = if relative {
            let date =
                i18n.format_instant(seconds, InstantFormat::FullLongZone, &self.timezone, true);
            i18n.format(
                "@astryx.timestamp.relativeLabel",
                &[("relative", text.to_string().into()), ("date", date.into())],
            )
        } else {
            text.clone()
        };

        if relative && self.live {
            let state = window.use_keyed_state(self.id.clone(), cx, |_, _| LiveState::default());
            if !state.read(cx).scheduled {
                state.update(cx, |s, _| s.scheduled = true);
                let wait = until_relative_change(ago);
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
        }

        let mut text_el = Text::new(text).text_type(self.text_type).color(self.color);
        if let Some(size) = self.size {
            text_el = text_el.size(size);
        }
        if let Some(weight) = self.weight {
            text_el = text_el.weight(weight);
        }
        let theme = astryx_theme::current(window, cx);
        let mut el = div();
        theme.refine(
            el.style(),
            "timestamp",
            &[("format", effective.as_str())],
            None,
        );
        el.style().refine(&self.style);
        el.id(self.id)
            .role(Role::Time)
            .aria_label(name)
            .child(text_el)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_relative_times_refresh_at_unit_boundaries() {
        assert_eq!(until_relative_change(5), Duration::from_secs(1));
        assert_eq!(until_relative_change(125), Duration::from_secs(55));
        assert_eq!(until_relative_change(3600 + 30), Duration::from_secs(3570));
    }
}
