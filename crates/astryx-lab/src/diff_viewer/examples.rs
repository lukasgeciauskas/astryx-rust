//! The DiffViewer gallery (upstream's `Lab/DiffViewer` stories).

use std::rc::Rc;

use astryx_components::button::{Button, ButtonVariant};
use astryx_components::containers::Card;
use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Heading, HeadingLevel, Text, TextColor, TextType};
use gpui::prelude::*;
use gpui::{AnyElement, App, Window, div, px};

use super::{
    CreateDiffFileOptions, DiffViewer, DiffViewerChangeIndicator, DiffViewerFile,
    DiffViewerInlineHighlight, DiffViewerSide, DiffViewerViewMode, create_diff_file,
    parse_unified_diff,
};

const PATCH: &str = "diff --git a/src/cart/total.ts b/src/cart/total.ts\nindex 4c1a2b7..9e0f3d1 100644\n--- a/src/cart/total.ts\n+++ b/src/cart/total.ts\n@@ -1,16 +1,18 @@\n import type {CartItem} from './types';\n+import {roundCurrency} from '../money';\n \n /**\n- * Sum the price of every item in the cart.\n+ * Sum the price of every item in the cart, applying the discount once\n+ * after the subtotal so rounding errors do not compound per item.\n  */\n-export function cartTotal(items: CartItem[], discount = 0) {\n-  let total = 0;\n-  for (const item of items) {\n-    total += item.price * item.quantity * (1 - discount);\n-  }\n-  return total;\n+export function cartTotal(items: CartItem[], discount = 0): number {\n+  const subtotal = items.reduce(\n+    (sum, item) => sum + item.price * item.quantity,\n+    0,\n+  );\n+  return roundCurrency(subtotal * (1 - discount));\n }\n \n export function isEmpty(items: CartItem[]) {\n   return items.length === 0;\n }\n@@ -40,7 +42,7 @@ export function formatTotal(total: number, currency: string) {\n   const formatter = new Intl.NumberFormat(undefined, {\n     style: 'currency',\n     currency,\n-    maximumFractionDigits: 3,\n+    maximumFractionDigits: 2,\n   });\n   return formatter.format(total);\n }\ndiff --git a/src/money.ts b/src/money.ts\nnew file mode 100644\nindex 0000000..b3f1c2e\n--- /dev/null\n+++ b/src/money.ts\n@@ -0,0 +1,4 @@\n+/** Round to whole cents. */\n+export function roundCurrency(value: number): number {\n+  return Math.round(value * 100) / 100;\n+}\ndiff --git a/src/cart/legacy.ts b/src/cart/legacyTotals.ts\nsimilarity index 100%\nrename from src/cart/legacy.ts\nrename to src/cart/legacyTotals.ts\ndiff --git a/public/receipt.png b/public/receipt.png\nindex 1f2e3d4..5a6b7c8 100644\nBinary files a/public/receipt.png and b/public/receipt.png differ\n";

const LONG_LINE_PATCH: &str = "diff --git a/src/cart/discounts.ts b/src/cart/discounts.ts\n--- a/src/cart/discounts.ts\n+++ b/src/cart/discounts.ts\n@@ -1,4 +1,4 @@\n import type {CartItem} from './types';\n-export const discountFor = (item: CartItem, rules: ReadonlyArray<{sku: string; percent: number}>) => rules.find(rule => rule.sku === item.sku)?.percent ?? 0;\n+export const discountFor = (item: CartItem, rules: ReadonlyArray<{sku: string; percent: number; expiresAt?: number}>) => rules.find(rule => rule.sku === item.sku && (rule.expiresAt == null || rule.expiresAt > Date.now()))?.percent ?? 0;\n \n export const NO_DISCOUNT = 0;\n";

fn total_file() -> DiffViewerFile {
    parse_unified_diff(PATCH).remove(0)
}

fn story(title: &'static str, description: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(Heading::new(HeadingLevel::H3, title))
        .children((!description.is_empty()).then(|| {
            Text::new(description)
                .text_type(TextType::Supporting)
                .color(TextColor::Secondary)
        }))
        .child(div().max_w(px(1080.)).w_full().child(body))
        .into_any_element()
}

/// The DiffViewer examples: unified and split views, sideways scrolling, every display
/// option, wrapping, a whole patch, two texts, a large change, a Viewed toggle, review
/// comments, a long comment and a compact viewer.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let viewed = window.use_keyed_state("diff-viewed", cx, |_, _| false);
    let comments = window.use_keyed_state("diff-comments", cx, |_, _| {
        vec![(1u64, DiffViewerSide::New, 13u64)]
    });
    let split = window.use_keyed_state("diff-review-split", cx, |_, _| false);
    let is_viewed = *viewed.read(cx);
    let is_split = *split.read(cx);
    let flags = create_diff_file(&CreateDiffFileOptions {
        path: Some("config/flags.yaml".into()),
        old_value: [
            "flags:",
            "  checkout_v2: false",
            "  dark_mode: true",
            "  search_suggestions: false",
            "  # owner: payments",
            "  retries: 3",
        ]
        .join("\n"),
        new_value: [
            "flags:",
            "  checkout_v2: true",
            "  dark_mode: true",
            "  search_suggestions: false",
            "  # owner: payments",
            "  retries: 5",
            "  timeout_ms: 2000",
        ]
        .join("\n"),
        ..Default::default()
    });
    let schema = |v: &str| {
        std::iter::once("// Generated file. Do not edit.".to_string())
            .chain((0..6000).map(|i| format!("export const field{i} = '{v}-{i}';")))
            .chain(std::iter::once("export default {};".to_string()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let large = create_diff_file(&CreateDiffFileOptions {
        path: Some("src/generated/schema.ts".into()),
        old_value: schema("v1"),
        new_value: schema("v2"),
        ..Default::default()
    });
    let long_line = parse_unified_diff(LONG_LINE_PATCH).remove(0);
    let (set_viewed, toggle_split) = (viewed.clone(), split.clone());
    let (add_comment, comment_list) = (comments.clone(), comments.read(cx).clone());
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story("Default", "One file, unified view (the default).", DiffViewer::new("diff-default", total_file())))
        .child(story(
            "Split",
            "Old file on the left, new file on the right.",
            DiffViewer::new("diff-split", total_file()).view_mode(DiffViewerViewMode::Split),
        ))
        .child(story(
            "Split independent scroll",
            "Unwrapped split view with lines wider than their side: the split stays fixed, both sides scroll sideways together, and line numbers and change bars stay put.",
            div().max_w(px(960.)).child(
                DiffViewer::new("diff-long", long_line.clone())
                    .view_mode(DiffViewerViewMode::Split)
                    .change_indicator(DiffViewerChangeIndicator::Bars),
            ),
        ))
        .child(story(
            "Display options",
            "How changes are marked, whether rows are tinted, how finely changed text is highlighted, wrapping, and line numbers.",
            DiffViewer::new("diff-options", total_file())
                .view_mode(DiffViewerViewMode::Split)
                .change_indicator(DiffViewerChangeIndicator::Bars)
                .inline_highlight(DiffViewerInlineHighlight::Word),
        ))
        .child(story(
            "No background",
            "Bars and word highlighting on plain rows: no change tint.",
            DiffViewer::new("diff-no-tint", total_file()).change_indicator(DiffViewerChangeIndicator::Bars).change_background(false),
        ))
        .child(story(
            "Character highlight",
            "Character-level highlighting with classic +/\u{2212} glyphs.",
            DiffViewer::new("diff-char", total_file()).inline_highlight(DiffViewerInlineHighlight::Char),
        ))
        .child(story(
            "Wrapped",
            "Long lines wrap instead of scrolling.",
            div().max_w(px(720.)).child(DiffViewer::new("diff-wrapped", long_line).view_mode(DiffViewerViewMode::Split).wrapped(true)),
        ))
        .child(story(
            "Multi-file patch",
            "A whole patch: one DiffViewer per file, with added, renamed and binary files.",
            Stack::vertical().gap(SpacingStep::S3).children(
                parse_unified_diff(PATCH)
                    .into_iter()
                    .enumerate()
                    .map(|(i, file)| DiffViewer::new(gpui::ElementId::Name(format!("diff-patch-{i}").into()), file)),
            ),
        ))
        .child(story("From two texts", "Diff two versions of a text with create_diff_file.", DiffViewer::new("diff-texts", flags)))
        .child(story(
            "Large change",
            "A rewrite too large to compare line by line: the changed lines fold behind a labeled control until the reader shows them.",
            DiffViewer::new("diff-large", large).max_height(px(480.)),
        ))
        .child(story(
            "Viewed toggle",
            "File header actions: a Viewed control that also folds the file away.",
            DiffViewer::new("diff-viewed-file", total_file()).collapsed(is_viewed).header_end(
                Button::new("diff-viewed-button", if is_viewed { "Viewed \u{2713}" } else { "Mark viewed" })
                    .size(astryx_primitives::ElementSize::Sm)
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_, _, cx| {
                        set_viewed.update(cx, |v, cx| {
                            *v = !*v;
                            cx.notify();
                        })
                    }),
            ),
        ))
        .child(story(
            "Review comments",
            "Click a line number to add a comment under that line; anchors stay put when switching view modes.",
            Stack::vertical()
                .gap(SpacingStep::S3)
                .child(
                    Button::new("diff-review-mode", if is_split { "Unified view" } else { "Split view" })
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, _, cx| {
                            toggle_split.update(cx, |s, cx| {
                                *s = !*s;
                                cx.notify();
                            })
                        }),
                )
                .child({
                    let list = Rc::new(comment_list);
                    let resolve = comments.clone();
                    DiffViewer::new("diff-review", total_file())
                        .view_mode(if is_split { DiffViewerViewMode::Split } else { DiffViewerViewMode::Unified })
                        .on_line_number_click(move |r, _, cx| {
                            let (side, line) = (r.side, r.line_number);
                            add_comment.update(cx, |c, cx| {
                                let id = c.iter().map(|(id, _, _)| *id).max().unwrap_or(0) + 1;
                                c.push((id, side, line));
                                cx.notify();
                            })
                        })
                        .render_line_annotation(move |r, _, _| {
                            let here: Vec<u64> = list.iter().filter(|(_, s, l)| *s == r.side && *l == r.line_number).map(|(id, _, _)| *id).collect();
                            if here.is_empty() {
                                return None;
                            }
                            let side = r.side.as_str();
                            let line = r.line_number;
                            Some(
                                Stack::vertical()
                                    .gap(SpacingStep::S2)
                                    .children(here.into_iter().map(|id| {
                                        let resolve = resolve.clone();
                                        Card::new().child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.))
                                                .child(div().flex_1().child(Text::new(format!("Comment on {side} line {line}"))))
                                                .child(
                                                    Button::new(gpui::ElementId::Name(format!("diff-resolve-{id}").into()), "Resolve")
                                                        .variant(ButtonVariant::Ghost)
                                                        .size(astryx_primitives::ElementSize::Sm)
                                                        .on_click(move |_, _, cx| {
                                                            resolve.update(cx, |c, cx| {
                                                                c.retain(|(i, _, _)| *i != id);
                                                                cx.notify();
                                                            })
                                                        }),
                                                ),
                                        )
                                    }))
                                    .into_any_element(),
                            )
                        })
                }),
        ))
        .child(story(
            "Long comment",
            "A multi-sentence comment wraps within the width the code sets.",
            DiffViewer::new("diff-long-comment", total_file()).render_line_annotation(|r, _, _| {
                (r.side == DiffViewerSide::New && r.line_number == 13).then(|| {
                    Text::new("Rounding once after the subtotal fixes the drift we saw on large carts, but it changes totals for carts that relied on per-item rounding. Before this lands, check the receipt snapshots, confirm the discount still applies to gift cards, and add a test with a 33% discount on three items so the rounding rule is pinned. If any partner integrations read the per-item totals, they need a heads-up as well.")
                        .text_type(TextType::Body)
                        .into_any_element()
                })
            }),
        ))
        .child(story("Compact", "Without line numbers, capped height.", DiffViewer::new("diff-compact", total_file()).line_numbers(false).max_height(px(240.))))
        .into_any_element()
}
