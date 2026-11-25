//! Timestamp's hover card, upstream `TimestampHoverCard`: the instant in
//! full, or in each of `tooltip_entries`' zones and formats, one line each,
//! with copy buttons on the copyable ones.

use std::time::Duration;

use astryx_i18n::{I18n, InstantFormat, TimeZoneChoice};
use astryx_primitives::{Politeness, announce};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ClipboardItem, ElementId, GridTrack, IntoElement, ParentElement, SharedString,
    Styled, Window, div,
};
use web_time::Instant;

use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconSize};
use crate::layout::VisuallyHidden;
use astryx_primitives::ElementSize;

/// How long a copy button shows that it copied.
const COPY_FEEDBACK: Duration = Duration::from_millis(1500);

/// One line of a Timestamp's hover card, upstream `TimestampTooltipEntry`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimestampEntry {
    /// An IANA zone, or `"local"` for the viewer's; else the Timestamp's
    /// zone.
    pub timezone_id: Option<String>,
    /// The format; the full style by default.
    pub format: Option<InstantFormat>,
    pub label: Option<SharedString>,
    /// Shows a copy button for the line.
    pub copyable: bool,
}

impl TimestampEntry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `timezoneID`.
    pub fn timezone_id(mut self, id: impl Into<String>) -> Self {
        self.timezone_id = Some(id.into());
        self
    }

    /// Upstream `format`.
    pub fn format(mut self, format: InstantFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Upstream `label`.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `isCopyable`.
    pub fn copyable(mut self, copyable: bool) -> Self {
        self.copyable = copyable;
        self
    }
}

/// A line as shown.
pub(crate) struct Line {
    pub label: Option<SharedString>,
    pub zone_name: SharedString,
    pub value: SharedString,
    pub copyable: bool,
}

fn resolve(id: Option<&str>) -> Option<TimeZoneChoice> {
    match id {
        None => None,
        Some(id) if id.eq_ignore_ascii_case("local") => Some(TimeZoneChoice::Local),
        Some(id) => Some(TimeZoneChoice::Iana(id.to_string())),
    }
}

/// Upstream `formatTooltipLines`; with no entries, the full time, copyable.
pub(crate) fn lines(
    i18n: &I18n,
    seconds: i64,
    entries: &[TimestampEntry],
    default_zone: &TimeZoneChoice,
) -> Vec<Line> {
    if entries.is_empty() {
        return vec![Line {
            label: None,
            zone_name: i18n
                .format_zone_name(seconds, &TimeZoneChoice::Local)
                .into(),
            value: i18n
                .format_instant(seconds, InstantFormat::Full, &TimeZoneChoice::Local, true)
                .into(),
            copyable: true,
        }];
    }
    let zones: Vec<TimeZoneChoice> = entries
        .iter()
        .map(|e| resolve(e.timezone_id.as_deref()).unwrap_or_else(|| default_zone.clone()))
        .collect();
    let key = |z: &TimeZoneChoice| match z {
        TimeZoneChoice::Local => "local".to_string(),
        TimeZoneChoice::Iana(id) => id.to_lowercase(),
    };
    let mut keys: Vec<String> = zones.iter().map(key).collect();
    keys.push(key(default_zone));
    keys.sort();
    keys.dedup();
    let multiple = keys.len() > 1;
    entries
        .iter()
        .zip(zones)
        .map(|(entry, zone)| {
            let format = entry.format.unwrap_or(InstantFormat::Full);
            let named = matches!(zone, TimeZoneChoice::Iana(_));
            let show_zone = match format {
                InstantFormat::Full | InstantFormat::FullLongZone => true,
                InstantFormat::DateTime | InstantFormat::Time => multiple || named,
                _ => false,
            };
            Line {
                label: entry.label.clone(),
                zone_name: i18n.format_zone_name(seconds, &zone).into(),
                value: i18n
                    .format_instant(seconds, format, &zone, show_zone)
                    .into(),
                copyable: entry.copyable,
            }
        })
        .collect()
}

/// The card's content: a grid of label, value and copy-action columns, each
/// present only when a line needs it.
pub(crate) fn card_content(
    id: &ElementId,
    lines: Vec<Line>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let label_column = lines
        .iter()
        .any(|l| l.label.as_ref().is_some_and(|l| !l.is_empty()));
    let action_column = lines.iter().any(|l| l.copyable);
    let mut columns = Vec::new();
    if label_column {
        columns.push(GridTrack::Auto);
    }
    columns.push(GridTrack::Fr(1.));
    if action_column {
        columns.push(GridTrack::Auto);
    }
    let mut grid = div()
        .grid()
        .grid_template_columns(columns)
        .items_center()
        .gap_x(t.spacing.s4)
        .gap_y(t.spacing.s2);
    for (i, line) in lines.into_iter().enumerate() {
        let hidden_zone = VisuallyHidden::new().child(line.zone_name.clone());
        if label_column {
            let label = div()
                .whitespace_nowrap()
                .text_size(t.text.supporting.size)
                .line_height(gpui::relative(t.text.supporting.leading))
                .text_color(t.color.text_secondary);
            grid = grid.child(match line.label.filter(|l| !l.is_empty()) {
                Some(text) => label.child(text).into_any_element(),
                None => label.relative().child(hidden_zone).into_any_element(),
            });
        }
        let mut value = div()
            .relative()
            .whitespace_nowrap()
            .text_size(t.text.body.size)
            .line_height(gpui::relative(t.text.body.leading))
            .text_color(t.color.text_primary)
            .child(line.value.clone());
        if !label_column {
            // The zone names the row for assistive technology.
            value = value.child(VisuallyHidden::new().child(line.zone_name));
        }
        grid = grid.child(value);
        if action_column {
            grid = grid.child(
                div().flex().justify_end().items_center().children(
                    line.copyable
                        .then(|| copy_button(id, i, line.value, window, cx)),
                ),
            );
        }
    }
    grid.into_any_element()
}

/// A line's copy button, upstream `CopyButton`: it copies the value and
/// shows a check for a moment.
fn copy_button(
    id: &ElementId,
    index: usize,
    value: SharedString,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let i18n = astryx_i18n::current(cx);
    let button_id = ElementId::NamedInteger(format!("{id}-copy").into(), index as u64);
    let copied_at = window.use_keyed_state(
        ElementId::NamedInteger(format!("{id}-copied").into(), index as u64),
        cx,
        |_, _| None::<Instant>,
    );
    let now = cx.background_executor().now();
    let copied = copied_at
        .read(cx)
        .is_some_and(|at| now.saturating_duration_since(at) < COPY_FEEDBACK);
    let (icon, tooltip, label) = if copied {
        let copied = i18n.t("@astryx.timestamp.copied");
        ("check", copied.clone(), copied)
    } else {
        (
            "copy",
            i18n.t("@astryx.timestamp.copy"),
            i18n.format(
                "@astryx.timestamp.copyValue",
                &[("value", value.to_string().into())],
            ),
        )
    };
    let announcement = i18n.t("@astryx.timestamp.copied");
    IconButton::new(button_id, label, Icon::new(icon).size(IconSize::Sm))
        .variant(ButtonVariant::Ghost)
        .size(ElementSize::Sm)
        .tooltip(tooltip)
        .on_click(move |_, window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
            let now = cx.background_executor().now();
            copied_at.update(cx, |at, _| *at = Some(now));
            announce(announcement.clone(), Politeness::Polite, window, cx);
            // Back to the copy icon once the feedback ends.
            let timer = cx.background_executor().timer(COPY_FEEDBACK);
            window
                .spawn(cx, async move |cx| {
                    timer.await;
                    cx.update(|window, _| window.refresh()).ok();
                })
                .detach();
            window.refresh();
        })
        .into_any_element()
}
