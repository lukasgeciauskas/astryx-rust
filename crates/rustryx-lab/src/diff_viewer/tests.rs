//! Upstream lab `parseUnifiedDiff.test.ts`, `createDiffFile.test.ts`,
//! `inlineDiff.test.ts` and `DiffViewer.test.tsx`, the parts a GPUI window can drive.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Modifiers, Render, TestAppContext, VisualTestContext, Window, div, px,
};

use super::inline::{InlineDiffGranularity, diff_inline_ranges};
use super::*;

// diffInlineRanges.

fn changed<'a>(line: &'a str, ranges: &[std::ops::Range<usize>]) -> Vec<&'a str> {
    ranges.iter().map(|r| &line[r.clone()]).collect()
}

#[test]
fn reports_the_changed_words_on_each_side() {
    let (a, b) = (
        "const total = price * qty;",
        "const total = price * quantity;",
    );
    let r = diff_inline_ranges(a, b, InlineDiffGranularity::Word);
    assert_eq!(changed(a, &r.old), ["qty"]);
    assert_eq!(changed(b, &r.new), ["quantity"]);
}

#[test]
fn reports_a_pure_insertion_only_on_the_new_side() {
    let r = diff_inline_ranges("f(a, b)", "f(a, b, c)", InlineDiffGranularity::Word);
    assert!(r.old.is_empty());
    assert_eq!(changed("f(a, b, c)", &r.new), [", c"]);
}

#[test]
fn bridges_a_whitespace_gap_between_changed_words() {
    let (a, b) = ("call(alpha beta, x, y, z)", "call(gamma delta, x, y, z)");
    let r = diff_inline_ranges(a, b, InlineDiffGranularity::Word);
    assert_eq!(changed(a, &r.old), ["alpha beta"]);
    assert_eq!(changed(b, &r.new), ["gamma delta"]);
}

#[test]
fn compares_characters_in_char_mode_also_outside_the_bmp() {
    let r = diff_inline_ranges("color", "colour", InlineDiffGranularity::Char);
    assert!(r.old.is_empty());
    assert_eq!(changed("colour", &r.new), ["u"]);
    let r = diff_inline_ranges("a\u{1F600}b", "a\u{1F600}c", InlineDiffGranularity::Char);
    assert_eq!(changed("a\u{1F600}b", &r.old), ["b"]);
    assert_eq!(changed("a\u{1F600}c", &r.new), ["c"]);
}

#[test]
fn returns_nothing_for_unrelated_identical_empty_or_oversized_lines() {
    let none = inline::InlineDiffResult::default();
    assert_eq!(
        diff_inline_ranges("foo bar", "baz qux", InlineDiffGranularity::Word),
        none
    );
    assert_eq!(
        diff_inline_ranges(
            "let total = 0;",
            "const sum = items;",
            InlineDiffGranularity::Word
        ),
        none
    );
    assert_eq!(
        diff_inline_ranges("same", "same", InlineDiffGranularity::Word),
        none
    );
    assert_eq!(
        diff_inline_ranges("", "x", InlineDiffGranularity::Word),
        none
    );
    let long = "a".repeat(600);
    assert_eq!(
        diff_inline_ranges(&long, &format!("{long}b"), InlineDiffGranularity::Char),
        none
    );
}

#[test]
fn drops_whitespace_only_ranges_such_as_re_indentation() {
    let r = diff_inline_ranges(
        "  return value;",
        "    return value + 1;",
        InlineDiffGranularity::Word,
    );
    assert!(r.old.is_empty());
    assert_eq!(changed("    return value + 1;", &r.new), [" + 1"]);
}

// parseUnifiedDiff.

const GIT_PATCH: &str = "diff --git a/src/math.ts b/src/math.ts
index 3b18e51..a9c2f7d 100644
--- a/src/math.ts
+++ b/src/math.ts
@@ -1,5 +1,6 @@
 export function add(a: number, b: number) {
-  return a + b;
+  const sum = a + b;
+  return sum;
 }
 
 export const ZERO = 0;
@@ -10,3 +11,3 @@ export function sub(a: number, b: number) {
 // trailing
-const old = 1;
+const next = 1;
 // end
diff --git a/README.md b/README.md
new file mode 100644
index 0000000..e69de29
--- /dev/null
+++ b/README.md
@@ -0,0 +1,2 @@
+# Title
+Body
diff --git a/old/name.ts b/new/name.ts
similarity index 100%
rename from old/name.ts
rename to new/name.ts
diff --git a/logo.png b/logo.png
index 1111111..2222222 100644
Binary files a/logo.png and b/logo.png differ
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index e69de29..0000000
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
";

fn kinds(hunk: &DiffViewerHunk) -> Vec<(DiffViewerLineKind, Option<u64>, Option<u64>)> {
    hunk.lines
        .iter()
        .map(|l| (l.kind, l.old_line_number, l.new_line_number))
        .collect()
}

#[test]
fn splits_a_git_patch_into_one_file_per_diff_header() {
    let files = parse_unified_diff(GIT_PATCH);
    assert_eq!(
        files.iter().map(|f| f.status).collect::<Vec<_>>(),
        [
            DiffViewerFileStatus::Modified,
            DiffViewerFileStatus::Added,
            DiffViewerFileStatus::Renamed,
            DiffViewerFileStatus::Modified,
            DiffViewerFileStatus::Deleted
        ]
    );
}

#[test]
fn numbers_lines_from_the_hunk_header_on_each_side() {
    use DiffViewerLineKind::*;
    let math = &parse_unified_diff(GIT_PATCH)[0];
    assert_eq!(math.old_path.as_deref(), Some("src/math.ts"));
    let first = &math.hunks[0];
    assert_eq!(first.header, "@@ -1,5 +1,6 @@");
    assert_eq!(
        kinds(first),
        [
            (Context, Some(1), Some(1)),
            (Removed, Some(2), None),
            (Added, None, Some(2)),
            (Added, None, Some(3)),
            (Context, Some(3), Some(4)),
            (Context, Some(4), Some(5)),
            (Context, Some(5), Some(6))
        ]
    );
    assert_eq!(first.lines[1].content, "  return a + b;");
    // An empty context line keeps an empty content string.
    assert_eq!(first.lines[5].content, "");
    assert_eq!(
        math.hunks[1].header,
        "@@ -10,3 +11,3 @@ export function sub(a: number, b: number) {"
    );
    assert_eq!(math.hunks[1].lines[1].old_line_number, Some(11));
}

#[test]
fn reads_added_renamed_binary_and_deleted_files() {
    let files = parse_unified_diff(GIT_PATCH);
    assert_eq!(
        (files[1].old_path.clone(), files[1].new_path.as_deref()),
        (None, Some("README.md"))
    );
    assert_eq!(
        files[1].hunks[0]
            .lines
            .iter()
            .map(|l| l.new_line_number)
            .collect::<Vec<_>>(),
        [Some(1), Some(2)]
    );
    assert_eq!(
        (files[2].old_path.as_deref(), files[2].new_path.as_deref()),
        (Some("old/name.ts"), Some("new/name.ts"))
    );
    assert!(files[2].hunks.is_empty());
    assert!(files[3].is_binary && files[3].hunks.is_empty());
    assert_eq!(
        (files[4].old_path.as_deref(), files[4].new_path.clone()),
        (Some("gone.txt"), None)
    );
    assert_eq!(
        kinds(&files[4].hunks[0]),
        [(DiffViewerLineKind::Removed, Some(1), None)]
    );
}

#[test]
fn reads_classic_diff_u_output_and_marks_renames_only_from_rename_lines() {
    let files = parse_unified_diff(
        "--- a.txt\t2024-01-01 00:00:00.000000000 +0000\n+++ b.txt\t2024-01-02 00:00:00.000000000 +0000\n@@ -1 +1 @@\n-one\n+uno",
    );
    assert_eq!(files.len(), 1);
    assert_eq!(
        (
            files[0].old_path.as_deref(),
            files[0].new_path.as_deref(),
            files[0].status
        ),
        (Some("a.txt"), Some("b.txt"), DiffViewerFileStatus::Modified)
    );
}

#[test]
fn decodes_git_quoted_paths_and_reads_paths_with_spaces() {
    let files = parse_unified_diff(
        "diff --git \"a/caf\\303\\251.md\" \"b/caf\\303\\251.md\"\n--- \"a/caf\\303\\251.md\"\n+++ \"b/caf\\303\\251.md\"\n@@ -1 +1 @@\n-x\n+y\n",
    );
    assert_eq!(files[0].new_path.as_deref(), Some("caf\u{e9}.md"));
    let files =
        parse_unified_diff("diff --git a/my file.txt b/my file.txt\nnew file mode 100644\n");
    assert_eq!(files[0].new_path.as_deref(), Some("my file.txt"));
}

#[test]
fn reads_a_bare_hunk_skips_no_newline_markers_and_accepts_crlf() {
    let files =
        parse_unified_diff("@@ -5 +5 @@\r\n-five\r\n+FIVE\r\n\\ No newline at end of file\r\n");
    assert_eq!(files.len(), 1);
    assert_eq!(
        (files[0].old_path.clone(), files[0].new_path.clone()),
        (None, None)
    );
    assert_eq!(
        files[0].hunks[0]
            .lines
            .iter()
            .map(|l| l.content.to_string())
            .collect::<Vec<_>>(),
        ["five", "FIVE"]
    );
}

#[test]
fn returns_no_files_for_text_that_is_not_a_diff() {
    assert!(parse_unified_diff("just some text\nnothing here").is_empty());
    assert!(parse_unified_diff("").is_empty());
}

// createDiffFile.

fn text(lines: &[&str]) -> String {
    lines.join("\n")
}

fn diff(old: &str, new: &str) -> DiffViewerFile {
    create_diff_file(&CreateDiffFileOptions {
        old_value: old.into(),
        new_value: new.into(),
        path: Some("f.txt".into()),
        ..Default::default()
    })
}

#[test]
fn returns_no_hunks_for_identical_text() {
    assert!(diff("a\nb", "a\nb").hunks.is_empty());
}

#[test]
fn finds_a_minimal_change_with_numbered_lines_and_a_header() {
    use DiffViewerLineKind::*;
    let file = diff(
        &text(&["a", "b", "c", "d", "e"]),
        &text(&["a", "b", "X", "d", "e"]),
    );
    assert_eq!(file.hunks.len(), 1);
    let hunk = &file.hunks[0];
    assert_eq!(hunk.header, "@@ -1,5 +1,5 @@");
    assert_eq!(
        kinds(hunk),
        [
            (Context, Some(1), Some(1)),
            (Context, Some(2), Some(2)),
            (Removed, Some(3), None),
            (Added, None, Some(3)),
            (Context, Some(4), Some(4)),
            (Context, Some(5), Some(5))
        ]
    );
    assert!(!hunk.is_replaced_block);
}

#[test]
fn splits_distant_changes_into_separate_hunks_and_uses_the_default_context_for_nan() {
    let old: Vec<String> = (1..=20).map(|i| format!("l{i}")).collect();
    let mut new = old.clone();
    new[1] = "X".into();
    new[18] = "Y".into();
    let (old, new) = (old.join("\n"), new.join("\n"));
    assert_eq!(diff(&old, &new).hunks.len(), 2);
    let nan = create_diff_file(&CreateDiffFileOptions {
        old_value: old.clone(),
        new_value: new.clone(),
        context_lines: Some(f64::NAN),
        ..Default::default()
    });
    assert_eq!(nan.hunks, diff(&old, &new).hunks);
}

#[test]
fn marks_added_deleted_and_renamed_files_from_their_paths() {
    let base = || CreateDiffFileOptions {
        old_value: "a".into(),
        new_value: "b".into(),
        ..Default::default()
    };
    let added = create_diff_file(&CreateDiffFileOptions {
        old_path: Some(None),
        new_path: Some(Some("n".into())),
        ..base()
    });
    assert_eq!(
        (added.status, added.old_path),
        (DiffViewerFileStatus::Added, None)
    );
    let deleted = create_diff_file(&CreateDiffFileOptions {
        new_path: Some(None),
        old_path: Some(Some("o".into())),
        ..base()
    });
    assert_eq!(
        (deleted.status, deleted.new_path),
        (DiffViewerFileStatus::Deleted, None)
    );
    let renamed = create_diff_file(&CreateDiffFileOptions {
        old_path: Some(Some("a".into())),
        new_path: Some(Some("b".into())),
        ..base()
    });
    assert_eq!(renamed.status, DiffViewerFileStatus::Renamed);
}

#[test]
fn shows_a_rewrite_too_large_to_search_as_one_replaced_block() {
    let old: Vec<String> = (0..6000).map(|i| format!("old {i}")).collect();
    let new: Vec<String> = (0..6000).map(|i| format!("new {i}")).collect();
    let file = diff(
        &format!("head\n{}\ntail", old.join("\n")),
        &format!("head\n{}\ntail", new.join("\n")),
    );
    assert_eq!(file.hunks.len(), 1);
    assert!(file.hunks[0].is_replaced_block);
    let removed = file.hunks[0]
        .lines
        .iter()
        .filter(|l| l.kind == DiffViewerLineKind::Removed)
        .count();
    assert_eq!(removed, 6000);
}

#[test]
fn keeps_a_long_one_sided_change_minimal_not_a_replaced_block() {
    let added: Vec<String> = (0..12_000).map(|i| format!("n{i}")).collect();
    let file = diff("a\nb", &format!("a\n{}\nb", added.join("\n")));
    assert!(!file.hunks[0].is_replaced_block);
    assert_eq!(
        file.hunks[0]
            .lines
            .iter()
            .filter(|l| l.kind == DiffViewerLineKind::Added)
            .count(),
        12_000
    );
}

#[test]
fn finds_a_minimal_diff_for_texts_with_more_than_a_thousand_changes() {
    // Every other line changes: past the traced search's limit, the linear-space
    // search finds the same minimal script.
    let old: Vec<String> = (0..1500).map(|i| format!("k{i}")).collect();
    let new: Vec<String> = (0..1500)
        .map(|i| {
            if i % 2 == 0 {
                format!("k{i}")
            } else {
                format!("v{i}")
            }
        })
        .collect();
    let file = diff(&old.join("\n"), &new.join("\n"));
    let changes: usize = file
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.kind != DiffViewerLineKind::Context)
        .count();
    assert_eq!(changes, 1500);
    assert!(file.hunks.iter().all(|h| !h.is_replaced_block));
}

// In a window.

type BuildFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host(BuildFn);

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(900.)).h(px(700.)).child((self.0)(window, cx))
    }
}

fn host(
    cx: &mut TestAppContext,
    f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(f)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .is_some()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    settle(cx);
    let b = cx
        .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

fn math() -> DiffViewerFile {
    parse_unified_diff(GIT_PATCH).remove(0)
}

#[gpui::test]
fn renders_every_line_and_collapses_from_the_header(cx: &mut TestAppContext) {
    let changes: Rc<RefCell<Vec<bool>>> = Rc::default();
    let c = changes.clone();
    let cx = host(cx, move |_, _| {
        let c = c.clone();
        DiffViewer::new("dv", math())
            .on_collapsed_change(move |next, _, _| c.borrow_mut().push(next))
            .into_any_element()
    });
    assert!(shown(cx, "dv-code-new-2"));
    assert!(
        shown(cx, "dv-code-old-2"),
        "the removed line anchors to its old number"
    );
    click(cx, "dv-toggle");
    assert_eq!(*changes.borrow(), [true]);
    assert!(!shown(cx, "dv-code-new-2"));
    click(cx, "dv-toggle");
    assert!(shown(cx, "dv-code-new-2"));
}

#[gpui::test]
fn follows_a_controlled_collapsed_state(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        DiffViewer::new("dv", math())
            .collapsed(true)
            .into_any_element()
    });
    assert!(!shown(cx, "dv-code-new-2"));
    click(cx, "dv-toggle");
    assert!(!shown(cx, "dv-code-new-2"), "the owner keeps it collapsed");
}

#[gpui::test]
fn renders_annotation_rows_and_makes_line_numbers_report_their_line(cx: &mut TestAppContext) {
    type Clicks = Rc<RefCell<Vec<(DiffViewerSide, u64)>>>;
    let clicks: Clicks = Rc::default();
    let c = clicks.clone();
    let cx = host(cx, move |_, _| {
        let c = c.clone();
        DiffViewer::new("dv", math())
            .render_line_annotation(|r, _, _| {
                (r.side == DiffViewerSide::New && r.line_number == 3).then(|| {
                    div()
                        .debug_selector(|| "note-new-3".into())
                        .child("Looks good")
                        .into_any_element()
                })
            })
            .on_line_number_click(move |r, _, _| c.borrow_mut().push((r.side, r.line_number)))
            .into_any_element()
    });
    assert!(shown(cx, "note-new-3"));
    let note = cx.debug_bounds("note-new-3").unwrap();
    let line = cx.debug_bounds("dv-code-new-3").unwrap();
    assert!(
        note.top() >= line.bottom() - px(1.),
        "the note sits under its line"
    );
    click(cx, "dv-line-new-3");
    click(cx, "dv-line-old-2");
    assert_eq!(
        *clicks.borrow(),
        [(DiffViewerSide::New, 3), (DiffViewerSide::Old, 2)]
    );
    // A context line's old number is plain text, not a second button.
    assert!(!shown(cx, "dv-line-old-1"));
    assert!(shown(cx, "dv-line-new-1"));
}

#[gpui::test]
fn pairs_removed_and_added_lines_side_by_side_in_split_view(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        DiffViewer::new("dv", math())
            .view_mode(DiffViewerViewMode::Split)
            .into_any_element()
    });
    let old = cx.debug_bounds("dv-code-old-2").expect("the removed line");
    let new = cx
        .debug_bounds("dv-code-new-2")
        .expect("the added line that replaced it");
    assert!((old.top() - new.top()).abs() < px(1.), "on one row");
    assert!(old.right() <= new.left() + px(1.), "old on the left");
}

#[gpui::test]
fn shows_messages_for_binary_and_empty_files(cx: &mut TestAppContext) {
    let binary = parse_unified_diff(GIT_PATCH).remove(3);
    let cx = host(cx, move |_, _| {
        div()
            .child(DiffViewer::new("bin", binary.clone()))
            .child(
                DiffViewer::new("same", diff("a", "a"))
                    .header_end(div().debug_selector(|| "viewed".into()).child("Viewed")),
            )
            .into_any_element()
    });
    assert!(shown(cx, "bin"));
    assert!(!shown(cx, "bin-viewport"), "a binary file has no hunk area");
    assert!(shown(cx, "viewed"), "the headerEnd slot renders");
}

#[gpui::test]
fn folds_a_replaced_block_behind_a_labeled_control(cx: &mut TestAppContext) {
    let old: Vec<String> = (0..6000).map(|i| format!("old {i}")).collect();
    let new: Vec<String> = (0..6000).map(|i| format!("new {i}")).collect();
    let file = diff(
        &format!("head\n{}\ntail", old.join("\n")),
        &format!("head\n{}\ntail", new.join("\n")),
    );
    let cx = host(cx, move |_, _| {
        DiffViewer::new("dv", file.clone())
            .max_height(px(400.))
            .into_any_element()
    });
    assert!(shown(cx, "dv-fold-h0"));
    assert!(!shown(cx, "dv-code-old-2"), "the block's lines are folded");
    assert!(shown(cx, "dv-code-new-1"), "its context stays in view");
    click(cx, "dv-fold-h0");
    assert!(shown(cx, "dv-code-old-2"));
}

#[gpui::test]
fn folds_a_file_with_more_than_2000_changed_lines(cx: &mut TestAppContext) {
    let old: Vec<String> = (0..2100).map(|i| format!("o{i}")).collect();
    let new: Vec<String> = (0..2100)
        .map(|i| {
            if i % 2 == 0 {
                format!("o{i}")
            } else {
                format!("n{i}")
            }
        })
        .collect();
    let file = diff(&old.join("\n"), &new.join("\n"));
    let cx = host(cx, move |_, _| {
        DiffViewer::new("dv", file.clone())
            .max_height(px(300.))
            .into_any_element()
    });
    assert!(shown(cx, "dv-fold-file"));
    assert!(!shown(cx, "dv-viewport"));
    click(cx, "dv-fold-file");
    assert!(shown(cx, "dv-viewport"));
}

#[test]
fn infers_the_language_from_the_file_name() {
    let name = |p: &str| rustryx_syntax::language_key(&infer_language(Some(p), &[])).to_string();
    assert_eq!(name("src/app.tsx"), "tsx");
    assert_eq!(name("lib/x.RS"), "rust");
    assert_eq!(name("Makefile"), "plaintext");
    assert_eq!(
        name(".bashrc"),
        "plaintext",
        "a leading dot is not an extension"
    );
    assert_eq!(
        rustryx_syntax::language_key(&infer_language(None, &[])),
        "plaintext"
    );
}
