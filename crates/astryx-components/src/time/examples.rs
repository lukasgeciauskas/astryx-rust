//! Time examples: the gallery's Time page and the doc example of Timer and
//! Timestamp.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::layout::{SpacingStep, Stack, StackAlign};
use crate::text::{Text, TextType};
use crate::time::{Timer, TimerFormat, Timestamp, TimestampFormat};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let row = |name: &'static str, value: AnyElement| {
        Stack::horizontal()
            .gap(SpacingStep::S3)
            .v_align(StackAlign::Center)
            .child(Text::new(name).text_type(TextType::Label))
            .child(value)
    };
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(label("Timestamp formats"))
        .child(row(
            "relative",
            Timestamp::new("rel", now - 2 * 3600)
                .format(TimestampFormat::Relative)
                .into_any_element(),
        ))
        .child(row(
            "relative_short",
            Timestamp::new("short", now - 2 * 3600)
                .format(TimestampFormat::RelativeShort)
                .into_any_element(),
        ))
        .child(row(
            "yesterday",
            Timestamp::new("yday", now - 86_400 - 60)
                .format(TimestampFormat::Relative)
                .into_any_element(),
        ))
        .child(row(
            "auto (recent)",
            Timestamp::new("auto", now - 90)
                .live(true)
                .into_any_element(),
        ))
        .child(row(
            "auto (old)",
            Timestamp::new("old", "2025-03-21T21:51:07Z").into_any_element(),
        ))
        .child(row(
            "date",
            Timestamp::new("date", "2025-03-21T21:51:07Z")
                .format(TimestampFormat::Date)
                .into_any_element(),
        ))
        .child(row(
            "date_long",
            Timestamp::new("long", "2025-03-21T21:51:07Z")
                .format(TimestampFormat::DateLong)
                .into_any_element(),
        ))
        .child(row(
            "date_weekday",
            Timestamp::new("wd", "2025-03-21T21:51:07Z")
                .format(TimestampFormat::DateWeekday)
                .into_any_element(),
        ))
        .child(row(
            "time, Los Angeles, zone shown",
            Timestamp::new("la", "2025-03-21T21:51:07Z")
                .format(TimestampFormat::Time)
                .timezone_id("America/Los_Angeles")
                .timezone_shown(true)
                .into_any_element(),
        ))
        .child(label("Timers"))
        .child(row(
            "elapsed",
            Timer::new("elapsed")
                .start_time((now - 185) * 1000)
                .into_any_element(),
        ))
        .child(row(
            "clock",
            Timer::new("clock")
                .start_time((now - 3729) * 1000)
                .format(TimerFormat::Clock)
                .into_any_element(),
        ))
        .into_any_element()
}
