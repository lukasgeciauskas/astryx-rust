//! The LogStream gallery (upstream's `Lab/LogStream` stories).

use astryx_components::button::{Button, ButtonVariant};
use astryx_components::containers::Card;
use astryx_components::layout::{SpacingStep, Stack, StackAlign, StackItem, StackItemSize};
use astryx_components::status::{StatusDot, StatusDotVariant};
use astryx_components::text::{Heading, HeadingLevel, Text, TextColor, TextType};
use astryx_theme::ThemeStyled;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Styled, Window, div, px, relative,
};

use super::{LogEntry, LogStream, LogStreamLevel, LogStreamVariant};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(div().w(px(880.)).max_w_full().child(body))
        .into_any_element()
}

/// Upstream's `detailStyle` `<pre>`: code text at the stream's size, kept as written.
fn detail(text: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |window, cx| {
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        div()
            .font_stack(&t.font.family.code)
            .text_size(t.font.size.sm)
            .line_height(relative(1.7))
            .children(text.lines().map(|line| div().child(line.to_string())))
            .into_any_element()
    }
}

use LogStreamLevel::{Debug, Error, Info, Warn};

fn build_entries() -> Vec<LogEntry> {
    vec![
        LogEntry::new(
            "b-01",
            "12:04:16.002",
            Info,
            "Build machine: 4 cores, 8 GB RAM (iad1)",
        )
        .source("build"),
        LogEntry::new(
            "b-02",
            "12:04:16.089",
            Info,
            "Cloning github.com/acme/astryx-console (branch: main)",
        )
        .source("build"),
        LogEntry::new("b-03", "12:04:18.021", Info, "Install").source("stage"),
        LogEntry::new(
            "b-04",
            "12:04:18.144",
            Info,
            "$ pnpm install --frozen-lockfile",
        )
        .source("install"),
        LogEntry::new("b-05", "12:04:23.348", Info, "$ next build").source("build"),
        LogEntry::new("b-06", "12:04:38.207", Warn, "Compiled with warnings (1)")
            .source("build")
            .detail(detail(
                "./app/logs/page.tsx\n42:9 Warning: \"range\" is assigned a value but never used.",
            )),
        LogEntry::new(
            "b-07",
            "12:04:45.201",
            Info,
            "Uploading build outputs (23.4 MB)",
        )
        .source("deploy"),
        LogEntry::new("b-08", "12:04:50.004", Info, "Build completed in 34s").source("deploy"),
    ]
}

fn monitoring_entries() -> Vec<LogEntry> {
    vec![
        LogEntry::new("l-01", "14:02:08.114", Info, "GET /v1/projects 200 in 42ms").source("api-gateway"),
        LogEntry::new("l-02", "14:02:08.371", Debug, "token cache hit for key sess_7f31").source("auth"),
        LogEntry::new("l-03", "14:02:09.243", Warn, "upstream latency 1840ms exceeds 1500ms budget")
            .source("billing")
            .detail(detail("{\n  \"upstream\": \"payments.stripe\",\n  \"latencyMs\": 1840,\n  \"budgetMs\": 1500,\n  \"traceId\": \"tr_9c41b2\"\n}")),
        LogEntry::new("l-04", "14:02:10.037", Error, "charge failed: upstream returned 502")
            .source("billing")
            .detail(detail("{\n  \"error\": \"UpstreamBadGateway\",\n  \"attempt\": 1,\n  \"retryInMs\": 400,\n  \"invoice\": \"inv_20418\"\n}")),
        LogEntry::new("l-05", "14:02:11.305", Info, "charge succeeded for inv_20418 in 322ms").source("billing"),
        LogEntry::new("l-06", "14:02:13.078", Debug, "route table reloaded (37 routes)").source("api-gateway"),
    ]
}

/// Upstream's `liveScript`: the lines "Append line" adds, in order.
fn live_script(i: usize) -> Option<LogEntry> {
    let (timestamp, level, source, message) = [
        (
            "14:02:14.102",
            Info,
            "api-gateway",
            "GET /v1/projects 200 in 38ms",
        ),
        (
            "14:02:15.310",
            Debug,
            "auth",
            "token cache hit for key sess_9a02",
        ),
        (
            "14:02:17.708",
            Warn,
            "billing",
            "webhook delivery slow: 2210ms to partner.acme",
        ),
        (
            "14:02:20.131",
            Error,
            "worker",
            "job usage-rollup-0415 failed: table locked",
        ),
    ]
    .get(i)
    .copied()?;
    Some(LogEntry::new(format!("live-{i}"), timestamp, level, message).source(source))
}

/// The ControlledFollow story's state: how many live lines were appended, and pinning.
struct Live {
    appended: usize,
    following: bool,
}

/// The LogStream examples: a terminal build log, monitoring rows in a Card, and a
/// controlled live tail.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let live = window.use_keyed_state(ElementId::Name("log-stream-live".into()), cx, |_, _| Live {
        appended: 0,
        following: true,
    });
    let (appended, following) = {
        let l = live.read(cx);
        (l.appended, l.following)
    };
    let mut entries = monitoring_entries();
    entries.extend((0..appended).filter_map(live_script));
    let rows = entries.len();
    let can_append = live_script(appended).is_some();
    let (append, reset, follow) = (live.clone(), live.clone(), live.clone());

    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example(
            "Terminal build",
            Stack::vertical()
                .gap(SpacingStep::S2)
                .child(
                    Stack::horizontal()
                        .gap(SpacingStep::S2)
                        .v_align(StackAlign::Center)
                        .child(StatusDot::new(
                            "log-build-ready",
                            StatusDotVariant::Success,
                            "Ready",
                        ))
                        .child(Heading::new(HeadingLevel::H3, "Build logs")),
                )
                .child(
                    LogStream::new("log-build", build_entries())
                        .variant(LogStreamVariant::Terminal)
                        .max_height(px(360.))
                        .label("Build logs"),
                ),
        ))
        .child(example(
            "Monitoring rows",
            Card::new().padding(SpacingStep::S4).child(
                Stack::vertical()
                    .gap(SpacingStep::S3)
                    .child(
                        Stack::vertical()
                            .gap(SpacingStep::S0)
                            .child(Heading::new(HeadingLevel::H3, "Log results"))
                            .child(
                                Text::new(format!(
                                    "{} events indexed / env:prod / UTC",
                                    monitoring_entries().len()
                                ))
                                .text_type(TextType::Supporting)
                                .color(TextColor::Secondary),
                            ),
                    )
                    .child(
                        LogStream::new("log-monitoring", monitoring_entries())
                            .max_height(px(360.))
                            .label("Log results stream"),
                    ),
            ),
        ))
        .child(example(
            "Controlled follow",
            Stack::vertical()
                .gap(SpacingStep::S3)
                .child(
                    Stack::horizontal()
                        .gap(SpacingStep::S2)
                        .v_align(StackAlign::Center)
                        .child(
                            StatusDot::new(
                                "log-live-follow",
                                if following {
                                    StatusDotVariant::Success
                                } else {
                                    StatusDotVariant::Neutral
                                },
                                if following {
                                    "Following latest"
                                } else {
                                    "Not following"
                                },
                            )
                            .pulsing(following),
                        )
                        .child(
                            StackItem::new().size(StackItemSize::Fill).child(
                                Text::new(format!("{rows} rows"))
                                    .text_type(TextType::Supporting)
                                    .color(TextColor::Secondary),
                            ),
                        )
                        .child(
                            Button::new("log-live-append", "Append line")
                                .variant(ButtonVariant::Secondary)
                                .disabled(!can_append)
                                .on_click(move |_, window, cx| {
                                    append.update(cx, |l, _| {
                                        if live_script(l.appended).is_some() {
                                            l.appended += 1;
                                            l.following = true;
                                        }
                                    });
                                    window.refresh();
                                }),
                        )
                        .child(
                            Button::new("log-live-reset", "Reset")
                                .variant(ButtonVariant::Ghost)
                                .on_click(move |_, window, cx| {
                                    reset.update(cx, |l, _| {
                                        l.appended = 0;
                                        l.following = true;
                                    });
                                    window.refresh();
                                }),
                        ),
                )
                .child(
                    LogStream::new("log-live", entries)
                        .max_height(px(320.))
                        .following(following)
                        .on_follow_change(move |value, window, cx| {
                            follow.update(cx, |l, _| l.following = value);
                            window.refresh();
                        })
                        .label("Live log stream"),
                ),
        ))
        .into_any_element()
}
