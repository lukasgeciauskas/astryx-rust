//! Markdown tests, after its spec's FR items and upstream's `Markdown.test.tsx`.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_markdown::{ParseOptions, parse_markdown_ast};
use astryx_primitives::{LinkHandler, link_provider};
use gpui::{
    IntoElement, Modifiers, ParentElement, Pixels, Styled, TestAppContext, VisualTestContext, div,
    point, px,
};

use super::markdown::{code_fence, heading_ids, table_column_floors};
use super::*;
use crate::segmented::tests::host;
use crate::text::TextDisplay;

type Log = Rc<RefCell<Vec<String>>>;
type Headings = Rc<RefCell<Vec<(u8, Option<String>)>>>;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

/// A Markdown in a 900px column, its links reported to `log`.
fn page(
    cx: &mut TestAppContext,
    log: Log,
    build: impl Fn() -> Markdown + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        let log = log.clone();
        let handler = LinkHandler::new(move |navigation, _, _| {
            log.borrow_mut().push(format!(
                "open {} {}",
                navigation.href,
                navigation.target.as_deref().unwrap_or("-")
            ));
        });
        div()
            .w(px(900.))
            .child(link_provider(handler, build()))
            .into_any_element()
    });
    settle(cx);
    cx
}

fn blocks(source: &str) -> Vec<astryx_markdown::ast::Block> {
    parse_markdown_ast(source, &ParseOptions::default()).children
}

fn spacing_step(
    cx: &mut VisualTestContext,
    step: impl Fn(&astryx_theme::Tokens) -> gpui::AbsoluteLength,
) -> Pixels {
    cx.update(|window, app| {
        let theme = astryx_theme::current(window, app);
        step(theme.tokens()).to_pixels(window.rem_size())
    })
}

/// The space between two blocks' wrappers.
fn gap(cx: &mut VisualTestContext, above: &'static str, below: &'static str) -> Pixels {
    let a = cx.debug_bounds(above).unwrap();
    let b = cx.debug_bounds(below).unwrap();
    b.top() - a.bottom()
}

#[test]
fn root_headings_take_unique_slugs_and_quoted_ones_none() {
    let ids = heading_ids(&blocks(
        "# Setup\n\n## Setup\n\n> # Quoted\n\n# Hello, World!\n\n# !!!\n\n# !!!",
    ));
    assert_eq!(
        ids,
        ["setup", "setup-1", "hello-world", "section", "section-1"]
    );
}

#[test]
fn fr29_fences_pick_the_diff_layer_by_their_first_word() {
    let fence = |source: &str| match blocks(source).remove(0) {
        astryx_markdown::ast::Block::Code(code) => code_fence(&code),
        other => panic!("not code: {other:?}"),
    };
    assert_eq!(fence("```diff\n-a\n```"), ("plaintext".into(), true));
    assert_eq!(fence("```diff-tsx\n-a\n```"), ("tsx".into(), true));
    assert_eq!(fence("```ts\nconst a = 1;\n```"), ("ts".into(), false));
    assert_eq!(fence("```\nplain\n```"), ("plaintext".into(), false));
    assert_eq!(
        fence("```c++\nint a;\n```"),
        ("c".into(), false),
        "the leading word"
    );
}

#[test]
fn fr26_table_columns_floor_from_their_own_content() {
    let table = |source: &str| match blocks(source).remove(0) {
        astryx_markdown::ast::Block::Table(table) => table_column_floors(&table),
        other => panic!("not a table: {other:?}"),
    };
    let long = "x".repeat(120);
    assert_eq!(
        table(&format!(
            "| Key | Meaning | Note |\n| --- | --- | --- |\n| id | Stable identifier never reused | {long} |"
        )),
        [4, 15, 24]
    );
    assert_eq!(
        table(
            "| Component name | Accessibility status and remediation owner | X |\n| --- | --- | --- |\n| Button | Pass | 1 |"
        ),
        [14, 21, 4],
        "a header floor up to its cap"
    );
    assert_eq!(
        table("| Status | Owner |\n| --- | --- |"),
        [6, 5],
        "header-only"
    );
    assert_eq!(
        table("| A | B | C |\n| --- | --- | --- |\n|  | middle only |  |"),
        [4, 6, 4],
        "empty cells at the minimum"
    );
}

#[gpui::test]
fn fr1_block_display_renders_blocks_and_inline_display_none(cx: &mut TestAppContext) {
    {
        let cx = page(cx, Log::default(), || {
            Markdown::new("md", "# Title\n\nText")
        });
        assert!(cx.debug_bounds("md-block-0").is_some());
        assert!(cx.debug_bounds("md-block-1-paragraph").is_some());
    }
    let cx = page(cx, Log::default(), || {
        Markdown::new("inline", "**bold** text").display(TextDisplay::Inline)
    });
    assert!(
        cx.debug_bounds("inline-block-0").is_none(),
        "no block anatomy"
    );
}

#[gpui::test]
fn fr3_a_heading_renderer_takes_the_level_and_root_slug(cx: &mut TestAppContext) {
    let seen: Headings = Rc::default();
    let log = seen.clone();
    let _cx = page(cx, Log::default(), move || {
        let log = log.clone();
        Markdown::new("md", "# Alpha\n\n### Beta\n\n##### Deep\n\n> # Quoted")
            .heading_level_start(3)
            .components(MarkdownComponents::new().heading(move |props, _, _| {
                log.borrow_mut()
                    .push((props.level, props.id.map(|id| id.to_string())));
                div().children(props.children).into_any_element()
            }))
    });
    let seen = seen.borrow();
    assert_eq!(
        seen[..4],
        [
            (3, Some("alpha".into())),
            (5, Some("beta".into())),
            (6, Some("deep".into())),
            (3, None),
        ],
        "shifted, capped at 6; quoted headings have no id"
    );
}

#[gpui::test]
fn fr29_a_code_renderer_receives_the_leading_word_and_nothing_new(cx: &mut TestAppContext) {
    let seen: Log = Rc::default();
    let log = seen.clone();
    let _cx = page(cx, Log::default(), move || {
        let log = log.clone();
        Markdown::new(
            "md",
            "```diff\n-a\n```\n\n```diff-tsx\n+b\n```\n\n```\nplain\n```",
        )
        .components(MarkdownComponents::new().code(move |props, _, _| {
            log.borrow_mut()
                .push(format!("{}:{}", props.language, props.code));
            div().into_any_element()
        }))
    });
    assert_eq!(
        seen.borrow()[..3],
        ["diff:-a", "diff:+b", "plaintext:plain"]
    );
}

#[gpui::test]
fn links_open_their_destination_external_ones_in_a_new_tab(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = page(cx, log.clone(), || {
        Markdown::new("md", "[Docs](https://example.com/docs \"Title\")")
    });
    let at = cx.debug_bounds("md-block-0").unwrap().origin + point(px(8.), px(8.));
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
    assert_eq!(
        *log.borrow(),
        ["open https://example.com/docs _blank"],
        "the destination without its title"
    );
}

#[gpui::test]
fn on_link_click_runs_first_and_false_keeps_the_link_from_opening(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let clicks = log.clone();
    let cx = page(cx, log.clone(), move || {
        let clicks = clicks.clone();
        Markdown::new("md", "[Settings](/settings)").on_link_click(move |href, _, _, _| {
            clicks.borrow_mut().push(format!("click {href}"));
            false
        })
    });
    let at = cx.debug_bounds("md-block-0").unwrap().origin + point(px(8.), px(8.));
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
    assert_eq!(*log.borrow(), ["click /settings"]);
}

#[gpui::test]
fn unsafe_destinations_render_as_text(cx: &mut TestAppContext) {
    let seen: Log = Rc::default();
    let log = seen.clone();
    let _cx = page(cx, Log::default(), move || {
        let log = log.clone();
        Markdown::new(
            "md",
            "[bad](javascript:alert(1)) [good](https://example.com) ![x](data:text/html,y)",
        )
        .components(MarkdownComponents::new().link(move |props, _, _| {
            log.borrow_mut().push(props.href.to_string());
            div().children(props.children).into_any_element()
        }))
    });
    let mut seen = seen.borrow().clone();
    seen.dedup();
    assert_eq!(seen, ["https://example.com"], "only the safe link");
}

#[gpui::test]
fn autolinks_only_with_the_gfm_option(cx: &mut TestAppContext) {
    let render = |cx: &mut TestAppContext, autolink: bool| {
        let seen: Log = Rc::default();
        let log = seen.clone();
        let _cx = page(cx, Log::default(), move || {
            let log = log.clone();
            let md = Markdown::new(
                "md",
                "See https://example.com and www.example.org or me@example.com",
            )
            .components(MarkdownComponents::new().link(move |props, _, _| {
                log.borrow_mut().push(props.href.to_string());
                div().children(props.children).into_any_element()
            }));
            if autolink {
                md.autolink(MarkdownAutolink::Gfm)
            } else {
                md
            }
        });
        seen.borrow().clone()
    };
    assert!(render(cx, false).is_empty(), "plain text by default");
    let mut links = render(cx, true);
    links.dedup();
    assert_eq!(
        links[..3],
        [
            "https://example.com",
            "http://www.example.org",
            "mailto:me@example.com"
        ]
    );
}

#[gpui::test]
fn block_margins_collapse_and_follow_the_density(cx: &mut TestAppContext) {
    {
        let cx = page(cx, Log::default(), || {
            Markdown::new("md", "One\n\nTwo\n\n## Three\n\nFour")
        });
        let s3 = spacing_step(cx, |t| t.spacing.s3);
        let s6 = spacing_step(cx, |t| t.spacing.s6);
        assert_eq!(
            gap(cx, "md-block-0", "md-block-1"),
            s3,
            "paragraph to paragraph"
        );
        assert_eq!(
            gap(cx, "md-block-1", "md-block-2"),
            s6,
            "the larger margin wins"
        );
    }
    let cx = page(cx, Log::default(), || {
        Markdown::new("dense", "One\n\nTwo").density(MarkdownDensity::Compact)
    });
    let s1 = spacing_step(cx, |t| t.spacing.s1);
    assert_eq!(gap(cx, "dense-block-0", "dense-block-1"), s1);
}

#[gpui::test]
fn prose_is_capped_at_the_content_width(cx: &mut TestAppContext) {
    let width = |cx: &mut TestAppContext, md: fn() -> Markdown| {
        let cx = page(cx, Log::default(), md);
        cx.debug_bounds("md-block-0-paragraph").unwrap().size.width
    };
    assert_eq!(
        width(cx, || Markdown::new("md", "Text")),
        px(680.),
        "the 680px measure"
    );
    assert_eq!(
        width(cx, || Markdown::new("md", "Text").content_width(px(300.))),
        px(300.)
    );
    assert_eq!(
        width(cx, || Markdown::new("md", "Text")
            .content_width(gpui::Length::Auto)),
        px(900.),
        "no cap"
    );
    assert_eq!(
        width(cx, || Markdown::new("md", "Text").content_width(px(0.))),
        px(900.),
        "zero drops the cap too"
    );
}

#[gpui::test]
fn content_align_centres_capped_prose(cx: &mut TestAppContext) {
    let cx = page(cx, Log::default(), || {
        Markdown::new("md", "Text")
            .content_width(px(300.))
            .content_align(MarkdownContentAlign::Center)
    });
    let column = cx.debug_bounds("md-block-0").unwrap();
    let bounds = cx.debug_bounds("md-block-0-paragraph").unwrap();
    assert_eq!(
        bounds.left() - column.left(),
        px(300.),
        "centred in the 900px column"
    );
}

#[gpui::test]
fn lists_tasks_and_tables_render_their_parts(cx: &mut TestAppContext) {
    let cx = page(cx, Log::default(), || {
        Markdown::new(
            "md",
            "- one\n- [x] done\n  - nested\n\nBetween\n\n- [ ] a\n- [x] b\n\n| a | b |\n|---|:-:|\n| 1 | 2 |",
        )
    });
    // The first list mixes tasks and plain items and stays one List (FR23).
    assert!(cx.debug_bounds("md-block-0-item-0").is_some());
    assert!(
        cx.debug_bounds("md-block-0-item-1-0").is_some(),
        "a nested block"
    );
    assert!(
        cx.debug_bounds("md-block-2-item-1").is_some(),
        "a task list row"
    );
    assert!(cx.debug_bounds("md-block-3-table-header-0-1").is_some());
    assert!(cx.debug_bounds("md-block-3-table-body-0-1").is_some());
}

#[gpui::test]
fn code_blocks_fit_their_content_at_least_the_measure_wide(cx: &mut TestAppContext) {
    let cx = page(cx, Log::default(), || {
        Markdown::new("md", "```ts\nconst a = 1;\n```")
    });
    assert_eq!(
        cx.debug_bounds("md-block-0-code").unwrap().size.width,
        px(680.),
        "min(680px, 100%), not the whole column"
    );
}
