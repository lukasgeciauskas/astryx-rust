//! The parts the views share (upstream `shared.tsx`): the frame with its header and plugin
//! slots, the range titles, the event pills and list rows with their category colors,
//! the popover body, and the status announcement.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, FontWeight, HighlightStyle, Hsla, IntoElement, Role, SharedString,
    StyledText, Window, div, px,
};
use rustryx_components::status::Spinner;
use rustryx_components::text::HeadingLevel;
use rustryx_i18n::date::{PartKind, PlainDate};
use rustryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollRole, UseScrollableAreaOptions, use_scrollable_area,
};
use rustryx_primitives::{Politeness, announce};
use rustryx_theme::color::mix_srgb;
use rustryx_theme::{ThemeStyled, Tokens, current};

use super::context::ScheduleContextValue;
use super::format::{ScheduleFormat, get_event_category};
use super::types::{CalendarEvent, ScheduleEventColor, ScheduleHeaderContent};

/// The colors of an event's surface (upstream `eventSurfaceColorStyle` and
/// `eventPastSurfaceColorStyle`): its text, border and fill.
pub(crate) fn surface_colors(
    t: &Tokens,
    color: ScheduleEventColor,
    is_past: bool,
) -> (Hsla, Hsla, Hsla) {
    let c = &t.color;
    let (text, border) = match color {
        ScheduleEventColor::Red => (c.text_red, c.border_red),
        ScheduleEventColor::Orange => (c.text_orange, c.border_orange),
        ScheduleEventColor::Yellow => (c.text_yellow, c.border_yellow),
        ScheduleEventColor::Green => (c.text_green, c.border_green),
        ScheduleEventColor::Teal => (c.text_teal, c.border_teal),
        ScheduleEventColor::Cyan => (c.text_cyan, c.border_cyan),
        ScheduleEventColor::Blue => (c.text_blue, c.border_blue),
        ScheduleEventColor::Purple => (c.text_purple, c.border_purple),
        ScheduleEventColor::Pink => (c.text_pink, c.border_pink),
        ScheduleEventColor::Gray => (c.text_gray, c.border_gray),
    };
    let card = c.background_card;
    if !is_past {
        // `color-mix(in srgb, <border> 20%, <card>)`.
        return (text, border, mix_srgb(card, border, 0.2));
    }
    if color == ScheduleEventColor::Gray {
        return (c.text_secondary, c.border, mix_srgb(card, border, 0.1));
    }
    (
        mix_srgb(c.text_secondary, text, 0.52),
        mix_srgb(c.border, border, 0.48),
        mix_srgb(card, border, 0.1),
    )
}

/// A list row's dot (upstream `eventDotColorStyle`): the category's border color, the
/// secondary text color for gray.
pub(crate) fn dot_color(t: &Tokens, color: ScheduleEventColor) -> Hsla {
    match color {
        ScheduleEventColor::Gray => t.color.text_secondary,
        other => surface_colors(t, other, false).1,
    }
}

/// The line height of the supporting text style, px.
pub(crate) fn supporting_leading(t: &Tokens, rem: gpui::Pixels) -> gpui::Pixels {
    t.text.supporting.size.to_pixels(rem) * t.text.supporting.leading
}

/// An event pill's frame (upstream `styles.eventPill`).
fn pill_frame(t: &Tokens, colors: (Hsla, Hsla, Hsla), rem: gpui::Pixels) -> gpui::Div {
    let (text, border, fill) = colors;
    div()
        .flex()
        .items_center()
        .gap(t.spacing.s1)
        .min_w_0()
        .border_1()
        .border_color(border)
        .bg(fill)
        .text_color(text)
        .rounded(t.radius.inner)
        .py(t.spacing.s0_5)
        .px(t.spacing.s1_5)
        .font_stack(&t.font.family.body)
        .text_size(t.text.supporting.size)
        .line_height(supporting_leading(t, rem))
        .font_weight(t.font.weight.medium)
}

/// A pill with its time first, then its title (upstream `EventPill`); the time shows for a
/// timed event drawn on a day.
pub(crate) fn event_pill(
    context: &ScheduleContextValue,
    event: &CalendarEvent,
    day: Option<PlainDate>,
    is_past: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let category = get_event_category(event, &context.categories);
    let time = match (day, event.instants()) {
        (Some(_), Some((start, end))) => Some(context.format.event_time_range(
            start,
            end,
            &context.timezone_id,
        )),
        _ => None,
    };
    pill_frame(t, surface_colors(t, category.color, is_past), rem)
        .children(time.map(|time| div().flex_none().truncate().opacity(0.8).child(time)))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(t.font.weight.bold)
                .child(event.title.clone()),
        )
        .into_any_element()
}

/// A pill that leads with the event's title (upstream `TitleFirstPill`): its time follows on
/// the same line only when the whole title and the time fit, and otherwise the title shows
/// alone, ellipsized. The line wraps, so layout, not script, decides whether the time
/// shows (component:Schedule FR18, PR5).
pub(crate) fn title_first_pill(
    context: &ScheduleContextValue,
    event: &CalendarEvent,
    time_label: Option<String>,
    is_past: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let category = get_event_category(event, &context.categories);
    let line = div()
        .flex()
        .flex_wrap()
        .items_baseline()
        .gap_x(t.spacing.s1)
        .flex_grow_1()
        .min_w_0()
        .h(supporting_leading(t, rem))
        .overflow_hidden()
        .child(
            div()
                .min_w_0()
                .max_w_full()
                .truncate()
                .font_weight(t.font.weight.bold)
                .child(event.title.clone()),
        )
        .children(time_label.map(|time| {
            div()
                .flex_none()
                .whitespace_nowrap()
                .opacity(0.8)
                .child(time)
        }));
    pill_frame(t, surface_colors(t, category.color, is_past), rem)
        .child(line)
        .into_any_element()
}

/// A month chip's pill: the title, then a timed event's start (upstream
/// `MonthEventPill`, component:Schedule FR18).
pub(crate) fn month_event_pill(
    context: &ScheduleContextValue,
    event: &CalendarEvent,
    is_past: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let time = event
        .instants()
        .map(|(start, _)| context.format.time(start, &context.timezone_id));
    title_first_pill(context, event, time, is_past, window, cx)
}

/// One event in a list (upstream `ListEventRow`): a color dot, the time or "All day", and
/// the title. A past row is muted with the secondary text color and a faded dot.
pub(crate) fn list_event_row(
    context: &ScheduleContextValue,
    event: &CalendarEvent,
    is_past: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let category = get_event_category(event, &context.categories);
    let time = match event.instants() {
        None => "All day".to_string(),
        Some((start, end)) => context
            .format
            .event_time_range(start, end, &context.timezone_id),
    };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.s2)
        .min_w_0()
        .child(
            div().w(px(12.)).flex_none().child(
                div()
                    .size(px(10.))
                    .rounded_full()
                    .bg(dot_color(t, category.color))
                    .when(is_past, |d| d.opacity(0.5)),
            ),
        )
        .child(
            div()
                .w(px(144.))
                .flex_none()
                .font_stack(&t.font.family.body)
                .text_color(t.color.text_secondary)
                .text_size(t.text.supporting.size)
                .line_height(supporting_leading(t, rem))
                .whitespace_nowrap()
                .child(time),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_stack(&t.font.family.body)
                .text_color(if is_past {
                    t.color.text_secondary
                } else {
                    t.color.text_primary
                })
                .text_size(t.text.body.size)
                .line_height(t.text.body.size.to_pixels(rem) * t.text.body.leading)
                .font_weight(t.font.weight.medium)
                .child(event.title.clone()),
        )
        .into_any_element()
}

/// The month title's text with its month part bold (upstream `LocalizedMonthTitle`).
fn month_title_runs(
    format: &ScheduleFormat,
    date: PlainDate,
    text: &mut String,
    bold: &mut Vec<std::ops::Range<usize>>,
) {
    for part in format.month_title_parts(date) {
        let start = text.len();
        text.push_str(&part.value);
        if part.kind == PartKind::Month {
            bold.push(start..text.len());
        }
    }
}

/// A range's title, its months bold (upstream `ScheduleMonthTitle` and
/// `ScheduleRangeMonthTitle`): one month, or "May - June 2026".
pub(crate) fn range_month_title(
    format: &ScheduleFormat,
    start: PlainDate,
    end: PlainDate,
) -> (String, Vec<std::ops::Range<usize>>) {
    let (mut text, mut bold) = (String::new(), Vec::new());
    if format.is_same_calendar_month(start, end) {
        month_title_runs(format, start, &mut text, &mut bold);
    } else if format.is_same_calendar_year(start, end) {
        let month = format.month_name(start);
        bold.push(0..month.len());
        text.push_str(&month);
        text.push_str(" - ");
        month_title_runs(format, end, &mut text, &mut bold);
    } else {
        month_title_runs(format, start, &mut text, &mut bold);
        text.push_str(" - ");
        month_title_runs(format, end, &mut text, &mut bold);
    }
    (text, bold)
}

/// The heading level of a Schedule's sub-headings.
pub(crate) fn heading_level(level: u8) -> HeadingLevel {
    match level {
        2 => HeadingLevel::H2,
        4 => HeadingLevel::H4,
        5 => HeadingLevel::H5,
        6 => HeadingLevel::H6,
        _ => HeadingLevel::H3,
    }
}

/// Text in a sub-heading's style at the schedule's level, secondary (upstream's
/// `<Heading level color="secondary">` around painted content).
pub(crate) fn heading_text(t: &Tokens, level: u8) -> gpui::Div {
    let style = match level {
        2 => &t.text.heading_2,
        4 => &t.text.heading_4,
        5 => &t.text.heading_5,
        6 => &t.text.heading_6,
        _ => &t.text.heading_3,
    };
    div()
        .font_stack(&t.font.family.heading)
        .text_style(style)
        .text_color(t.color.text_secondary)
}

/// Announces `message` once per occasion (upstream `useStatusAnnouncement`): when it first shows,
/// and whenever it changes; a message that goes away and comes back is a new occasion.
pub(crate) fn announce_status(id: &str, message: Option<&str>, window: &mut Window, cx: &mut App) {
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-status-announcement").into()),
        cx,
        |_, _| None::<String>,
    );
    let said = state.read(cx).clone();
    match message {
        None | Some("") => {
            if said.is_some() {
                state.update(cx, |s, _| *s = None);
            }
        }
        Some(message) if said.as_deref() == Some(message) => {}
        Some(message) => {
            state.update(cx, |s, _| *s = Some(message.to_string()));
            announce(message.to_string(), Politeness::Polite, window, cx);
        }
    }
}

/// The frame every view renders in (upstream `ScheduleFrame`): a named section with the
/// header (plugins' start slot, the range title with its loading spinner, the end slot)
/// over the view's body.
#[allow(clippy::too_many_arguments)]
pub(crate) fn schedule_frame(
    context: &Rc<ScheduleContextValue>,
    title: (String, Vec<std::ops::Range<usize>>),
    title_label: String,
    is_loading: bool,
    body: Vec<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    // The ring is silent; the Schedule announces its own busy start.
    announce_status(
        &context.id,
        is_loading.then_some("Loading events"),
        window,
        cx,
    );
    let (text, bold) = title;
    let highlights = bold
        .into_iter()
        .map(|range| {
            (
                range,
                HighlightStyle {
                    font_weight: Some(FontWeight(t.font.weight.bold.0)),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let heading = div()
        .id(ElementId::Name(format!("{}-title", context.id).into()))
        .font_stack(&t.font.family.heading)
        .text_style(&t.text.heading_2)
        .text_color(t.color.text_primary)
        .role(Role::Heading)
        .aria_level(2)
        .aria_label(SharedString::from(text.clone()))
        .child(StyledText::new(text).with_highlights(highlights));
    let spinner = div()
        .flex()
        // Upstream hides the spinner from assistive technology always (`aria-hidden`).
        .inert(true)
        .when(!is_loading, |d| d.opacity(0.))
        .child(
            Spinner::new(ElementId::Name(format!("{}-loading", context.id).into()))
                .aria_label("Loading events"),
        );
    let initial = ScheduleHeaderContent {
        start_content: Vec::new(),
        center_content: vec![
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .child(heading)
                .child(spinner)
                .into_any_element(),
        ],
        end_content: Vec::new(),
    };
    let header = context
        .plugins
        .iter()
        .fold(initial, |content, plugin| plugin.render_header(content));
    let slot = |items: Vec<AnyElement>| div().flex().items_center().gap(px(8.)).children(items);
    let selector = format!("{}-frame", context.id);
    let mut frame = div()
        .id(ElementId::Name(format!("{}-frame", context.id).into()))
        .debug_selector(move || selector)
        .flex_1()
        .flex()
        .flex_col()
        .min_w_0()
        .min_h_0()
        .border_1()
        .border_color(t.color.border)
        .rounded(t.radius.container)
        .bg(t.color.background_card)
        .overflow_hidden()
        .role(Role::Region)
        .aria_label(SharedString::from(title_label));
    theme.refine(frame.style(), "schedule-frame", &[], None);
    frame
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s3)
                .py(t.spacing.s3)
                .px(t.spacing.s4)
                .border_b_1()
                .border_color(t.color.border)
                .child(
                    slot(header.start_content)
                        .flex_1()
                        .flex_basis(px(0.))
                        .min_w_0()
                        .justify_start(),
                )
                .child(
                    slot(header.center_content)
                        .flex_none()
                        .justify_center()
                        .min_w_0(),
                )
                .child(
                    slot(header.end_content)
                        .flex_1()
                        .flex_basis(px(0.))
                        .min_w_0()
                        .justify_end(),
                ),
        )
        .children(body)
        .into_any_element()
}

/// The content of a view's popover (upstream `SchedulePopoverBody`): only the content
/// scrolls, inside the surface, in a named group that joins the tab order only while it
/// overflows; a group, not a region, so no popover adds a landmark. It never grows past
/// the visible height, less the surface's padding.
pub(crate) fn popover_body(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    child: AnyElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id: SharedString = id.into();
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let area = use_scrollable_area(
        &ElementId::Name(format!("{id}-popover-body").into()),
        UseScrollableAreaOptions::new(
            ScrollAxis::Block,
            ScrollKeyboardAccess::Viewport {
                label: label.into(),
                role: ScrollRole::Group,
            },
        ),
        window,
        cx,
    );
    let cap = window.viewport_size().height
        - (t.spacing.s4.to_pixels(rem) + t.spacing.s3.to_pixels(rem)) * 2.;
    area.viewport(div().max_h(cap))
        .child(area.content(div()).child(child))
        .into_any_element()
}

/// A screen-reader-only node: no box, a role and a name (upstream's visually hidden text).
pub(crate) fn sr_only(
    id: impl Into<SharedString>,
    role: Role,
    label: impl Into<SharedString>,
) -> AnyElement {
    let id: SharedString = id.into();
    div()
        .id(ElementId::Name(id))
        .absolute()
        .size_0()
        .overflow_hidden()
        .role(role)
        .aria_label(label.into())
        .into_any_element()
}
