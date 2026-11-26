//! CodeBlock tests, numbered after its spec.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    IntoElement, Modifiers, ParentElement, Styled, TestAppContext, VisualTestContext, div, px,
};

use super::*;
use crate::segmented::tests::host;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn page(
    cx: &mut TestAppContext,
    build: impl Fn() -> CodeBlock + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        div().w(px(700.)).child(build()).into_any_element()
    });
    settle(cx);
    cx
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

const TEN: &str = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";

#[gpui::test]
fn fr1_fr2_copy_writes_the_exact_code_once(cx: &mut TestAppContext) {
    let copies = Rc::new(RefCell::new(0));
    let cx = page(cx, {
        let copies = copies.clone();
        move || {
            let copies = copies.clone();
            CodeBlock::new("cb", "let a = 1;\n  let b = 2;\n")
                .language("rust")
                .on_copy(move |_, _| *copies.borrow_mut() += 1)
        }
    });
    click(cx, "cb-copy");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(
        copied.as_deref(),
        Some("let a = 1;\n  let b = 2;\n"),
        "the exact string"
    );
    assert_eq!(*copies.borrow(), 1);
}

#[gpui::test]
fn fr3_a_title_or_a_language_label_makes_the_header(cx: &mut TestAppContext) {
    let cx = page(cx, || CodeBlock::new("cb", "x").language("rust"));
    assert!(cx.debug_bounds("cb-header").is_some(), "a language label");
}

#[gpui::test]
fn fr3_plaintext_without_a_title_has_no_header_and_end_content_makes_none(cx: &mut TestAppContext) {
    let cx = page(cx, || {
        CodeBlock::new("cb", "x").end_content(div().child("tool"))
    });
    assert!(cx.debug_bounds("cb-header").is_none());
    assert!(
        cx.debug_bounds("cb-copy").is_some(),
        "headerless copy stays"
    );
}

#[gpui::test]
fn fr5_collapse_needs_the_threshold_and_toggles_the_body(cx: &mut TestAppContext) {
    let cx = page(cx, || {
        CodeBlock::new("cb", TEN).language("rust").collapsible(true)
    });
    let open = cx.debug_bounds("cb-region").unwrap().size.height;
    assert!(open > px(0.));
    click(cx, "cb-header");
    assert_eq!(
        cx.debug_bounds("cb-region").unwrap().size.height,
        px(0.),
        "collapsed"
    );
    click(cx, "cb-header");
    assert_eq!(
        cx.debug_bounds("cb-region").unwrap().size.height,
        open,
        "expanded again"
    );
}

#[gpui::test]
fn fr5_below_the_threshold_the_header_does_not_collapse(cx: &mut TestAppContext) {
    let cx = page(cx, || {
        CodeBlock::new("cb", "a\nb")
            .language("rust")
            .collapsible(true)
    });
    let open = cx.debug_bounds("cb-viewport").unwrap().size.height;
    click(cx, "cb-header");
    assert_eq!(cx.debug_bounds("cb-viewport").unwrap().size.height, open);
}

#[gpui::test]
fn fr14_line_numbers_are_file_coordinates_and_clamp_at_one(cx: &mut TestAppContext) {
    let cx = page(cx, || {
        CodeBlock::new("cb", "a\nb\nc")
            .line_numbers(true)
            .start_line_number(40)
            .highlight_lines([41, 99])
    });
    assert!(cx.debug_bounds("cb-line-40").is_some());
    assert!(cx.debug_bounds("cb-line-42").is_some());
    assert!(cx.debug_bounds("cb-line-1").is_none());
}

#[gpui::test]
fn fr14_a_start_below_one_clamps(cx: &mut TestAppContext) {
    let cx = page(cx, || {
        CodeBlock::new("cb", "a\nb")
            .line_numbers(true)
            .start_line_number(0)
    });
    assert!(cx.debug_bounds("cb-line-1").is_some());
    assert!(cx.debug_bounds("cb-line-0").is_none());
}

#[gpui::test]
fn fr11_a_headerless_blocks_controls_sit_beside_the_code_on_its_first_line(
    cx: &mut TestAppContext,
) {
    let cx = page(cx, || CodeBlock::new("cb", "npm install astryx\nmore"));
    let copy = cx.debug_bounds("cb-copy").unwrap();
    let viewport = cx.debug_bounds("cb-viewport").unwrap();
    let first = cx.debug_bounds("cb-line-1").unwrap();
    assert!(copy.left() >= viewport.right(), "never over the viewport");
    assert!(
        (copy.center().y - first.center().y).abs() < px(2.),
        "centred on the first line"
    );
}

#[gpui::test]
fn long_code_tokenizes_in_the_background(cx: &mut TestAppContext) {
    let code: String = (0..400).map(|i| format!("let v{i} = {i};\n")).collect();
    let cx = page(cx, move || {
        CodeBlock::new("cb", code.clone()).language("ts")
    });
    cx.executor().advance_clock(Duration::from_millis(50));
    settle(cx);
    assert!(cx.debug_bounds("cb-line-400").is_some());
}

#[gpui::test]
fn fr4_a_wrapped_block_wraps_its_long_lines(cx: &mut TestAppContext) {
    let long =
        "const message = 'a long line that has to wrap rather than scroll sideways at this width';";
    let cx = host(cx, move || {
        div()
            .w(px(300.))
            .child(
                CodeBlock::new("cb", long)
                    .language("js")
                    .wrapped(true)
                    .width(gpui::relative(1.)),
            )
            .into_any_element()
    });
    settle(cx);
    let line = cx.debug_bounds("cb-line-1").unwrap();
    let block = cx.debug_bounds("cb").unwrap();
    assert!(
        line.size.width <= block.size.width,
        "the line fits the block"
    );
    assert!(line.size.height > px(30.), "and takes several rows");
}

#[gpui::test]
fn fit_content_by_default_at_least_400px(cx: &mut TestAppContext) {
    {
        let cx = page(cx, || CodeBlock::new("cb", "x").language("rust"));
        assert_eq!(cx.debug_bounds("cb").unwrap().size.width, px(400.));
    }
    let cx = page(cx, || {
        CodeBlock::new("cb2", "x")
            .language("rust")
            .width(gpui::relative(1.))
    });
    assert_eq!(
        cx.debug_bounds("cb2").unwrap().size.width,
        px(700.),
        "an explicit width"
    );
}
