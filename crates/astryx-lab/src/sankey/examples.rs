//! The SankeyChart gallery (upstream's `Lab/SankeyChart` stories).

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Heading, HeadingLevel, Text, TextColor};
use astryx_theme::authoring::js::{js_number_to_string, js_round, js_to_fixed};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use super::{
    Oklch, SankeyChart, SankeyColumn, SankeyGrid, SankeyLabel, SankeyLink, SankeyLinkColor,
    SankeyLinkDatum, SankeyNode, SankeyNodeDatum,
};

fn nodes(list: &[(&str, &str, f64, Option<Oklch>)]) -> Vec<SankeyNodeDatum> {
    list.iter()
        .map(|(id, label, value, color)| {
            let node = SankeyNodeDatum::new(*id, *label, *value);
            match color {
                Some(c) => node.color(*c),
                None => node,
            }
        })
        .collect()
}

fn links(list: &[(&str, &str, f64)]) -> Vec<SankeyLinkDatum> {
    list.iter()
        .map(|(s, t, v)| SankeyLinkDatum::new(*s, *t, *v))
        .collect()
}

fn columns(list: &[&[&str]]) -> Vec<SankeyColumn> {
    list.iter()
        .map(|ids| SankeyColumn::ids(ids.iter().copied()))
        .collect()
}

fn funnel_nodes() -> Vec<SankeyNodeDatum> {
    nodes(&[
        ("visitors", "Visitors", 52000., Some([0.65, 0.2, 270.])),
        ("signups", "Sign Ups", 28000., Some([0.6, 0.17, 235.])),
        ("bounced", "Bounced", 24000., Some([0.55, 0.14, 350.])),
        ("activated", "Activated", 19500., Some([0.62, 0.16, 190.])),
        ("dormant", "Dormant", 8500., Some([0.55, 0.13, 50.])),
        ("subscribed", "Subscribed", 12400., Some([0.64, 0.18, 155.])),
        ("churned", "Churned", 7100., Some([0.54, 0.15, 20.])),
    ])
}

fn funnel_links() -> Vec<SankeyLinkDatum> {
    links(&[
        ("visitors", "signups", 28000.),
        ("visitors", "bounced", 24000.),
        ("signups", "activated", 19500.),
        ("signups", "dormant", 8500.),
        ("activated", "subscribed", 12400.),
        ("activated", "churned", 7100.),
    ])
}

fn funnel_columns() -> Vec<SankeyColumn> {
    columns(&[
        &["visitors"],
        &["signups", "bounced"],
        &["activated", "dormant"],
        &["subscribed", "churned"],
    ])
}

fn funnel(id: &'static str, height: f64) -> SankeyChart {
    SankeyChart::new(id, funnel_nodes(), funnel_links())
        .columns(funnel_columns())
        .height(height)
}

fn story(title: &'static str, caption: &'static str, chart: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(Heading::new(HeadingLevel::H3, title))
        .child(Text::new(caption).color(TextColor::Secondary))
        .child(chart)
        .into_any_element()
}

fn revenue() -> SankeyChart {
    let n = nodes(&[
        ("organic", "Organic", 18000., Some([0.62, 0.17, 155.])),
        ("paid", "Paid Ads", 22000., Some([0.6, 0.18, 240.])),
        ("referral", "Referral", 12000., Some([0.58, 0.15, 40.])),
        ("direct", "Direct", 8000., Some([0.55, 0.14, 300.])),
        ("web", "Web", 32000., Some([0.61, 0.16, 210.])),
        ("mobile", "Mobile", 20000., Some([0.59, 0.16, 170.])),
        ("other", "Other", 8000., Some([0.5, 0.11, 320.])),
        ("sub", "Subscription", 38000., Some([0.64, 0.19, 260.])),
        ("onetime", "One-time", 14000., Some([0.58, 0.15, 50.])),
        ("enterprise", "Enterprise", 8000., Some([0.56, 0.13, 190.])),
    ]);
    let l = links(&[
        ("organic", "web", 12000.),
        ("organic", "mobile", 6000.),
        ("paid", "web", 14000.),
        ("paid", "mobile", 8000.),
        ("referral", "web", 4000.),
        ("referral", "mobile", 6000.),
        ("referral", "other", 2000.),
        ("direct", "web", 2000.),
        ("direct", "other", 6000.),
        ("web", "sub", 22000.),
        ("web", "onetime", 6000.),
        ("web", "enterprise", 4000.),
        ("mobile", "sub", 14000.),
        ("mobile", "onetime", 4000.),
        ("mobile", "enterprise", 2000.),
        ("other", "sub", 2000.),
        ("other", "onetime", 4000.),
        ("other", "enterprise", 2000.),
    ]);
    SankeyChart::new("sankey-revenue", n, l)
        .columns(columns(&[
            &["organic", "paid", "referral", "direct"],
            &["web", "mobile", "other"],
            &["sub", "onetime", "enterprise"],
        ]))
        .height(420.)
        .child(SankeyGrid::new())
        .child(SankeyLink::new().opacity(0.65).tension(0.55))
        .child(SankeyNode::new())
        .child(SankeyLabel::new())
}

fn auto_columns() -> SankeyChart {
    let n = nodes(&[
        ("a", "Source A", 100., None),
        ("b", "Source B", 80., None),
        ("mid", "Middle", 180., None),
        ("out1", "Output 1", 120., None),
        ("out2", "Output 2", 60., None),
    ]);
    let l = links(&[
        ("a", "mid", 100.),
        ("b", "mid", 80.),
        ("mid", "out1", 120.),
        ("mid", "out2", 60.),
    ]);
    SankeyChart::new("sankey-auto", n, l)
        .height(280.)
        .child(SankeyLink::new().tension(0.6))
        .child(SankeyNode::new())
        .child(SankeyLabel::new().show_percent(false))
}

fn many_columns(cx: &App, window: &Window) -> impl IntoElement {
    const DROP: Option<Oklch> = Some([0.5, 0.12, 350.]);
    let n = nodes(&[
        ("awareness", "Awareness", 100000., None),
        ("interest", "Interest", 68000., None),
        ("dropped1", "Dropped", 32000., DROP),
        ("consideration", "Consideration", 45000., None),
        ("dropped2", "Distracted", 23000., DROP),
        ("intent", "Intent", 32000., None),
        ("dropped3", "Abandoned", 13000., DROP),
        ("evaluation", "Evaluation", 24000., None),
        ("dropped4", "Lost", 8000., DROP),
        ("purchase", "Purchase", 18000., Some([0.64, 0.18, 155.])),
        ("dropped5", "Rejected", 6000., DROP),
    ]);
    let l = links(&[
        ("awareness", "interest", 68000.),
        ("awareness", "dropped1", 32000.),
        ("interest", "consideration", 45000.),
        ("interest", "dropped2", 23000.),
        ("consideration", "intent", 32000.),
        ("consideration", "dropped3", 13000.),
        ("intent", "evaluation", 24000.),
        ("intent", "dropped4", 8000.),
        ("evaluation", "purchase", 18000.),
        ("evaluation", "dropped5", 6000.),
    ]);
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    div()
        .max_w(px(600.))
        .border_1()
        .border_color(t.color.border)
        .rounded(px(12.))
        .overflow_hidden()
        .child(
            SankeyChart::new("sankey-many", n, l)
                .columns(columns(&[
                    &["awareness"],
                    &["interest", "dropped1"],
                    &["consideration", "dropped2"],
                    &["intent", "dropped3"],
                    &["evaluation", "dropped4"],
                    &["purchase", "dropped5"],
                ]))
                .height(360.)
                .min_column_width(160.)
                .child(SankeyGrid::new())
                .child(SankeyLink::new())
                .child(SankeyNode::new())
                .child(SankeyLabel::new()),
        )
}

fn business() -> SankeyChart {
    // OKLCH approximations of Astryx data tokens: the categorical blue #0171E3 is about
    // oklch(0.55 0.19 255), blue-3 #2694FE oklch(0.65 0.17 250), gray-4 #5D6C7B
    // oklch(0.50 0.02 240) and gray-3 #AFB9C4 oklch(0.77 0.02 240).
    const GRAY: Option<Oklch> = Some([0.5, 0.02, 240.]);
    let n = nodes(&[
        ("visitors", "Visitors", 84200., Some([0.55, 0.19, 255.])),
        ("signups", "Signed Up", 42100., Some([0.58, 0.18, 255.])),
        ("bounce", "Bounced", 42100., GRAY),
        ("onboarded", "Onboarded", 28700., Some([0.61, 0.17, 252.])),
        ("stalled", "Stalled", 13400., GRAY),
        ("active", "Active Users", 21500., Some([0.64, 0.16, 250.])),
        ("inactive", "Inactive", 7200., GRAY),
        ("paying", "Paying", 15200., Some([0.67, 0.15, 248.])),
        ("free", "Free Tier", 6300., GRAY),
    ]);
    let l = links(&[
        ("visitors", "signups", 42100.),
        ("visitors", "bounce", 42100.),
        ("signups", "onboarded", 28700.),
        ("signups", "stalled", 13400.),
        ("onboarded", "active", 21500.),
        ("onboarded", "inactive", 7200.),
        ("active", "paying", 15200.),
        ("active", "free", 6300.),
    ]);
    SankeyChart::new("sankey-business", n, l)
        .columns(columns(&[
            &["visitors"],
            &["signups", "bounce"],
            &["onboarded", "stalled"],
            &["active", "inactive"],
            &["paying", "free"],
        ]))
        .height(380.)
        .child(SankeyGrid::new())
        .child(SankeyLink::new().opacity(0.6))
        .child(SankeyNode::new())
        .child(SankeyLabel::new())
}

fn wide_bars() -> SankeyChart {
    let n = nodes(&[
        ("single", "Single", 48., Some([0.62, 0.16, 30.])),
        ("married", "Married", 35., Some([0.58, 0.14, 180.])),
        ("divorced", "Divorced", 17., Some([0.55, 0.12, 300.])),
        ("male", "Male", 55., Some([0.57, 0.15, 240.])),
        ("female", "Female", 45., Some([0.6, 0.16, 340.])),
        ("happy", "Happy", 62., Some([0.64, 0.18, 150.])),
        ("unhappy", "Unhappy", 38., Some([0.52, 0.14, 25.])),
    ]);
    let l = links(&[
        ("single", "male", 26.),
        ("single", "female", 22.),
        ("married", "male", 20.),
        ("married", "female", 15.),
        ("divorced", "male", 9.),
        ("divorced", "female", 8.),
        ("male", "happy", 34.),
        ("male", "unhappy", 21.),
        ("female", "happy", 28.),
        ("female", "unhappy", 17.),
    ]);
    SankeyChart::new("sankey-wide", n, l)
        .columns([
            SankeyColumn::labeled("Relationship", ["single", "married", "divorced"]),
            SankeyColumn::labeled("Gender", ["male", "female"]),
            SankeyColumn::labeled("Outcome", ["happy", "unhappy"]),
        ])
        .height(380.)
        .node_gap(8.)
        .child(SankeyGrid::new())
        .child(SankeyLink::new().opacity(0.5).tension(0.5))
        .child(SankeyNode::new())
        .child(SankeyLabel::new())
}

/// Upstream's `formatBudget`: values in thousands of dollars.
fn format_budget(value: f64) -> String {
    if value >= 1_000_000_000. {
        return format!("${}T", js_to_fixed(value / 1_000_000_000., 1));
    }
    if value >= 1_000_000. {
        return format!("${}B", js_number_to_string(js_round(value / 1_000_000.)));
    }
    if value >= 1_000. {
        return format!("${}M", js_number_to_string(js_round(value / 1_000.)));
    }
    format!("${}", js_number_to_string(value))
}

fn budget() -> SankeyChart {
    let n = nodes(&[
        (
            "income_tax",
            "Income Tax",
            2118406000.,
            Some([0.55, 0.19, 255.]),
        ),
        (
            "payroll_tax",
            "Payroll Tax",
            1336808000.,
            Some([0.58, 0.17, 240.]),
        ),
        ("corp_tax", "Corp Tax", 511220000., Some([0.6, 0.16, 220.])),
        (
            "excise_tax",
            "Excise Tax",
            119883000.,
            Some([0.56, 0.14, 200.]),
        ),
        (
            "misc_revenue",
            "Misc Revenue",
            96615000.,
            Some([0.54, 0.12, 280.]),
        ),
        ("customs", "Customs", 47878000., Some([0.52, 0.11, 180.])),
        ("proposals", "Proposals", 45000000., Some([0.5, 0.1, 300.])),
        (
            "estate_tax",
            "Estate Tax",
            38543000.,
            Some([0.48, 0.09, 160.]),
        ),
        (
            "general_fund",
            "General Fund",
            2812308000.,
            Some([0.62, 0.17, 150.]),
        ),
        (
            "social_security",
            "Social Security",
            968357000.,
            Some([0.6, 0.16, 170.]),
        ),
        ("hhs", "HHS", 309881000., Some([0.58, 0.15, 130.])),
        ("treasury", "Treasury", 75173000., Some([0.56, 0.13, 190.])),
        ("labor", "Labor", 57839000., Some([0.54, 0.12, 40.])),
        ("transport", "Transport", 57056000., Some([0.52, 0.11, 60.])),
        (
            "agriculture",
            "Agriculture",
            11566000.,
            Some([0.5, 0.1, 100.]),
        ),
        ("fcc", "FCC", 10049000., Some([0.48, 0.09, 260.])),
        (
            "rail_retire",
            "Rail Retire",
            7098000.,
            Some([0.5, 0.08, 320.]),
        ),
        ("opm", "OPM", 5026000., Some([0.48, 0.08, 340.])),
    ]);
    let l = links(&[
        ("income_tax", "general_fund", 2118406000.),
        ("payroll_tax", "social_security", 968357000.),
        ("corp_tax", "general_fund", 511220000.),
        ("payroll_tax", "hhs", 298488000.),
        ("misc_revenue", "treasury", 75173000.),
        ("excise_tax", "general_fund", 62827000.),
        ("payroll_tax", "labor", 57839000.),
        ("excise_tax", "transport", 57056000.),
        ("proposals", "general_fund", 45000000.),
        ("estate_tax", "general_fund", 38543000.),
        ("customs", "general_fund", 36312000.),
        ("customs", "agriculture", 11566000.),
        ("misc_revenue", "hhs", 11393000.),
        ("misc_revenue", "fcc", 10049000.),
        ("payroll_tax", "rail_retire", 7098000.),
        ("payroll_tax", "opm", 5026000.),
    ]);
    SankeyChart::new("sankey-budget", n, l)
        .columns([
            SankeyColumn::labeled(
                "Revenue Source",
                [
                    "income_tax",
                    "payroll_tax",
                    "corp_tax",
                    "excise_tax",
                    "misc_revenue",
                    "customs",
                    "proposals",
                    "estate_tax",
                ],
            ),
            SankeyColumn::labeled(
                "Receiving Agency",
                [
                    "general_fund",
                    "social_security",
                    "hhs",
                    "treasury",
                    "labor",
                    "transport",
                    "agriculture",
                    "fcc",
                    "rail_retire",
                    "opm",
                ],
            ),
        ])
        .height(480.)
        .child(SankeyGrid::new())
        .child(SankeyLink::new().opacity(0.6).tension(0.5))
        .child(SankeyNode::new())
        .child(SankeyLabel::new().format_value(format_budget))
}

fn colored(id: &'static str, color: SankeyLinkColor) -> SankeyChart {
    funnel(id, 340.)
        .child(SankeyGrid::new())
        .child(SankeyLink::new().color(color))
        .child(SankeyNode::new())
        .child(SankeyLabel::new())
}

/// The SankeyChart examples: upstream's stories, from the conversion funnel to the
/// leaned gradients.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story(
            "Conversion Funnel",
            "User journey · Last 30 days",
            funnel("sankey-funnel", 360.)
                .child(SankeyGrid::new())
                .child(SankeyLink::new())
                .child(SankeyNode::new())
                .child(SankeyLabel::new()),
        ))
        .child(story(
            "Revenue Sources",
            "Channel attribution · March 2026",
            revenue(),
        ))
        .child(story(
            "Auto Column Detection",
            "No explicit columns — topologically sorted",
            auto_columns(),
        ))
        .child(story(
            "Deep Funnel (6 stages)",
            "Scrolls horizontally when columns exceed container width",
            many_columns(cx, window),
        ))
        .child(story(
            "Acquisition Funnel",
            "Blue = progression · Gray = drop-off",
            business(),
        ))
        .child(story(
            "Survey Flow",
            "Wider node bars with rotated text labels",
            wide_bars(),
        ))
        .child(story(
            "US Federal Budget FY2020",
            "Revenue sources → receiving agencies (vega-datasets/budget.json)",
            budget(),
        ))
        .child(story(
            "Petroleum Flow",
            "Monochrome — flat color, no gradients",
            funnel("sankey-mono", 340.)
                .node_color("#1a1a1e")
                .child(
                    SankeyLink::new()
                        .color(SankeyLinkColor::Flat("#1a1a1e".into()))
                        .opacity(0.75),
                )
                .child(SankeyNode::new().glow(false))
                .child(SankeyLabel::new()),
        ))
        .child(story(
            "Source-Colored Links",
            "Each ribbon matches its source node",
            colored("sankey-source", SankeyLinkColor::Source),
        ))
        .child(story(
            "Target-Colored Links",
            "Each ribbon matches its destination node",
            colored("sankey-target", SankeyLinkColor::Target),
        ))
        .child(story(
            "Source-Leaned Gradient",
            "bias=0.2 — holds source color, transitions near target",
            colored("sankey-lean-source", SankeyLinkColor::Biased(0.2)),
        ))
        .child(story(
            "Target-Leaned Gradient",
            "bias=0.8 — transitions early, holds target color",
            colored("sankey-lean-target", SankeyLinkColor::Biased(0.8)),
        ))
        .into_any_element()
}
