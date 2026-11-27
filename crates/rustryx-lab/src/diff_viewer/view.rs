//! The DiffViewer component (upstream lab `DiffViewer/DiffViewer.tsx`): a read-only,
//! syntax-highlighted diff of one file, unified or split, for code review surfaces.
//!
//! Unwrapped, upstream pins the line numbers and change indicators with sticky cells
//! while the code scrolls sideways, and split view's two panes scroll in lockstep.
//! GPUI has no sticky positioning, so each code cell clips its line and shifts it by
//! one sideways offset the viewer keeps (wheel and trackpad move it): the gutters stay
//! put, both sides move together, and every row stays as tall as its taller side.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ElementId, HighlightStyle, Hsla, KeyDownEvent, Length, Pixels, Role,
    ScrollEvent, SharedString, StyleRefinement, StyledText, Window, canvas, div, px,
};
use rustryx_components::button::{Button, ButtonVariant};
use rustryx_components::icon::{Icon, IconColor, IconSize};
use rustryx_components::syntax_theme::{SyntaxColors, syntax_colors, syntax_theme};
use rustryx_components::token::{Token, TokenColor};
use rustryx_i18n::Arg;
use rustryx_primitives::{ElementSize, control_focus, is_focus_visible};
use rustryx_syntax::{
    CodeLanguage, CodeLanguageInput, SyntaxThemeDefinition, SyntaxToken, TokenLine, tokenize,
};
use rustryx_theme::{ThemeStyled, current};

use super::inline::{InlineDiffGranularity, diff_inline_ranges};
use super::types::{
    DiffViewerFile, DiffViewerFileStatus, DiffViewerLine, DiffViewerLineKind, DiffViewerLineRef,
    DiffViewerSide,
};

/// Upstream `DiffViewerViewMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffViewerViewMode {
    /// One column, removed lines above their replacements (the default).
    #[default]
    Unified,
    /// The old file on the left, the new on the right, each removed line paired with
    /// the added line that replaced it.
    Split,
}

/// Upstream `DiffViewerChangeIndicator`: how changed lines are marked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffViewerChangeIndicator {
    /// `+` and `-` glyphs (the default).
    #[default]
    Classic,
    /// A solid bar for added lines, a dashed one for removed lines.
    Bars,
    None,
}

/// Upstream `DiffViewerInlineHighlight`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffViewerInlineHighlight {
    /// The changed words (the default).
    #[default]
    Word,
    Char,
    None,
}

/// Upstream `EXTENSION_LANGUAGES`.
fn extension_language(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "ts" | "mts" | "cts" => "ts",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "js",
        "jsx" => "jsx",
        "json" => "json",
        "html" | "htm" => "html",
        "xml" => "xml",
        "svg" => "svg",
        "css" => "css",
        "scss" => "scss",
        "less" => "less",
        "py" => "python",
        "sh" | "bash" => "bash",
        "zsh" => "zsh",
        "php" => "php",
        "hack" => "hack",
        "yaml" | "yml" => "yaml",
        "md" | "markdown" => "markdown",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
        "sql" => "sql",
        "rs" => "rust",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "go" => "go",
        _ => return None,
    })
}

/// Upstream `inferLanguage`: a listed language first (a later entry wins), then a
/// built-in by extension, else plain text.
pub fn infer_language(path: Option<&str>, code_languages: &[CodeLanguage]) -> CodeLanguageInput {
    let Some(path) = path else {
        return CodeLanguageInput::from("plaintext");
    };
    let base = &path[path.rfind('/').map_or(0, |i| i + 1)..];
    let extension = base
        .rfind('.')
        .filter(|dot| *dot > 0)
        .map(|dot| base[dot + 1..].to_lowercase());
    for language in code_languages.iter().rev() {
        if language.file_names.iter().any(|n| n == base)
            || extension
                .as_ref()
                .is_some_and(|e| language.file_extensions.iter().any(|x| x == e))
        {
            return CodeLanguageInput::Language(language.clone());
        }
    }
    CodeLanguageInput::from(
        extension
            .as_deref()
            .and_then(extension_language)
            .unwrap_or("plaintext"),
    )
}

/// Upstream `lineRef`: a removed line anchors to its old number, the others to their
/// new one.
pub fn line_ref(line: &DiffViewerLine, hunk_index: usize) -> Option<DiffViewerLineRef> {
    let (side, number) = match line.kind {
        DiffViewerLineKind::Removed => (DiffViewerSide::Old, line.old_line_number?),
        _ => (DiffViewerSide::New, line.new_line_number?),
    };
    Some(DiffViewerLineRef {
        side,
        line_number: number,
        line: line.clone(),
        hunk_index,
    })
}

/// One line as the viewer draws it.
#[derive(Clone)]
pub(crate) struct LineCell {
    pub line: DiffViewerLine,
    pub line_ref: DiffViewerLineRef,
    pub tokens: TokenLine,
    /// Changed byte ranges to highlight inside the line.
    pub emphasis: Vec<Range<usize>>,
}

/// Upstream `ReplacedBlockFold`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReplacedBlockFold {
    pub hunk_index: usize,
    /// The hunk cell the fold row goes before.
    pub at: usize,
    /// Removed plus added lines in the block.
    pub changed_lines: usize,
    pub expanded: bool,
}

pub(crate) struct HunkCells {
    pub cells: Vec<LineCell>,
    pub fold: Option<ReplacedBlockFold>,
}

/// Upstream `UnifiedRow` and `SplitRow`.
pub(crate) enum Row {
    Hunk(SharedString),
    Fold(ReplacedBlockFold),
    Line(LineCell),
    Pair(Option<LineCell>, Option<LineCell>),
}

/// Upstream `MAX_UNFOLDED_CHANGED_LINES`.
pub const MAX_UNFOLDED_CHANGED_LINES: usize = 2000;

/// Upstream `addInlineEmphasis`: each run of removed lines pairs with the added
/// lines after it, line by line, and each pair's changed ranges are marked.
fn add_inline_emphasis(cells: &mut [LineCell], granularity: InlineDiffGranularity) {
    let mut i = 0;
    while i < cells.len() {
        if cells[i].line.kind != DiffViewerLineKind::Removed {
            i += 1;
            continue;
        }
        let removed_start = i;
        while i < cells.len() && cells[i].line.kind == DiffViewerLineKind::Removed {
            i += 1;
        }
        let added_start = i;
        while i < cells.len() && cells[i].line.kind == DiffViewerLineKind::Added {
            i += 1;
        }
        let pairs = (added_start - removed_start).min(i - added_start);
        for r in 0..pairs {
            let ranges = diff_inline_ranges(
                &cells[removed_start + r].line.content,
                &cells[added_start + r].line.content,
                granularity,
            );
            cells[removed_start + r].emphasis = ranges.old;
            cells[added_start + r].emphasis = ranges.new;
        }
    }
}

/// Upstream `tokenizeRun`: a run's old and new sides each tokenized as continuous
/// text, each line taking the tokens of the side it renders from.
fn tokenize_run(lines: &[DiffViewerLine], language: &CodeLanguageInput) -> Vec<TokenLine> {
    if matches!(language, CodeLanguageInput::Name(name) if name == "plaintext") {
        return lines.iter().map(|_| Vec::new()).collect();
    }
    let side = |skip: DiffViewerLineKind| {
        lines
            .iter()
            .filter(|l| l.kind != skip)
            .map(|l| l.content.as_ref())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let old_tokens = tokenize(&side(DiffViewerLineKind::Added), language);
    let new_tokens = tokenize(&side(DiffViewerLineKind::Removed), language);
    let (mut old_index, mut new_index) = (0, 0);
    lines
        .iter()
        .map(|line| match line.kind {
            DiffViewerLineKind::Removed => {
                old_index += 1;
                old_tokens.get(old_index - 1).cloned().unwrap_or_default()
            }
            DiffViewerLineKind::Added => {
                new_index += 1;
                new_tokens.get(new_index - 1).cloned().unwrap_or_default()
            }
            DiffViewerLineKind::Context => {
                old_index += 1;
                new_index += 1;
                new_tokens.get(new_index - 1).cloned().unwrap_or_default()
            }
        })
        .collect()
}

/// Upstream `buildCells`: a folded replaced block keeps only the context around it,
/// and its changed lines are not tokenized.
pub(crate) fn build_cells(
    file: &DiffViewerFile,
    language: &CodeLanguageInput,
    inline: DiffViewerInlineHighlight,
    expanded: &HashSet<usize>,
) -> Vec<HunkCells> {
    file.hunks
        .iter()
        .enumerate()
        .map(|(hunk_index, hunk)| {
            let mut cells: Vec<LineCell> = Vec::new();
            let add_run = |lines: &[DiffViewerLine], cells: &mut Vec<LineCell>| {
                let tokens = tokenize_run(lines, language);
                for (line, tokens) in lines.iter().zip(tokens) {
                    if let Some(line_ref) = line_ref(line, hunk_index) {
                        cells.push(LineCell {
                            line: line.clone(),
                            line_ref,
                            tokens,
                            emphasis: Vec::new(),
                        });
                    }
                }
            };
            let change_start = if hunk.is_replaced_block {
                hunk.lines
                    .iter()
                    .position(|l| l.kind != DiffViewerLineKind::Context)
            } else {
                None
            };
            let Some(change_start) = change_start else {
                add_run(&hunk.lines, &mut cells);
                match inline {
                    DiffViewerInlineHighlight::Word => {
                        add_inline_emphasis(&mut cells, InlineDiffGranularity::Word)
                    }
                    DiffViewerInlineHighlight::Char => {
                        add_inline_emphasis(&mut cells, InlineDiffGranularity::Char)
                    }
                    DiffViewerInlineHighlight::None => {}
                }
                return HunkCells { cells, fold: None };
            };
            // A replaced block's removed and added lines are not edits of each other,
            // so they get no inline highlight.
            let is_expanded = expanded.contains(&hunk_index);
            let changed_lines = hunk
                .lines
                .iter()
                .filter(|l| l.kind != DiffViewerLineKind::Context)
                .count();
            if is_expanded {
                add_run(&hunk.lines, &mut cells);
                let at = cells
                    .iter()
                    .position(|c| c.line.kind != DiffViewerLineKind::Context)
                    .unwrap_or(cells.len());
                return HunkCells {
                    cells,
                    fold: Some(ReplacedBlockFold {
                        hunk_index,
                        at,
                        changed_lines,
                        expanded: true,
                    }),
                };
            }
            // Leading and trailing context are tokenized apart: the hidden lines between
            // them can open or close a comment or a string.
            add_run(&hunk.lines[..change_start], &mut cells);
            let at = cells.len();
            let rest = (change_start + changed_lines).min(hunk.lines.len());
            add_run(&hunk.lines[rest..], &mut cells);
            HunkCells {
                cells,
                fold: Some(ReplacedBlockFold {
                    hunk_index,
                    at,
                    changed_lines,
                    expanded: false,
                }),
            }
        })
        .collect()
}

/// Upstream `buildUnifiedRows`.
pub(crate) fn unified_rows(file: &DiffViewerFile, cells: &[HunkCells]) -> Vec<Row> {
    let mut rows = Vec::new();
    for (h, hunk) in file.hunks.iter().enumerate() {
        rows.push(Row::Hunk(hunk.header.clone()));
        let HunkCells { cells, fold } = &cells[h];
        for (i, cell) in cells.iter().enumerate() {
            if let Some(fold) = fold.filter(|f| f.at == i) {
                rows.push(Row::Fold(fold));
            }
            rows.push(Row::Line(cell.clone()));
        }
        if let Some(fold) = fold.filter(|f| f.at == cells.len()) {
            rows.push(Row::Fold(fold));
        }
    }
    rows
}

/// Upstream `buildSplitRows`: each run of removed lines pairs with the run of added
/// lines after it, row by row; context lines show on both sides of one row.
pub(crate) fn split_rows(file: &DiffViewerFile, cells: &[HunkCells]) -> Vec<Row> {
    let mut rows = Vec::new();
    for (h, hunk) in file.hunks.iter().enumerate() {
        rows.push(Row::Hunk(hunk.header.clone()));
        let HunkCells { cells, fold } = &cells[h];
        let mut i = 0;
        while i < cells.len() {
            if let Some(fold) = fold.filter(|f| f.at == i) {
                rows.push(Row::Fold(fold));
            }
            if cells[i].line.kind == DiffViewerLineKind::Context {
                rows.push(Row::Pair(Some(cells[i].clone()), Some(cells[i].clone())));
                i += 1;
                continue;
            }
            let mut removed = Vec::new();
            let mut added = Vec::new();
            while i < cells.len() && cells[i].line.kind == DiffViewerLineKind::Removed {
                removed.push(cells[i].clone());
                i += 1;
            }
            while i < cells.len() && cells[i].line.kind == DiffViewerLineKind::Added {
                added.push(cells[i].clone());
                i += 1;
            }
            for r in 0..removed.len().max(added.len()) {
                rows.push(Row::Pair(removed.get(r).cloned(), added.get(r).cloned()));
            }
        }
        if let Some(fold) = fold.filter(|f| f.at == cells.len()) {
            rows.push(Row::Fold(fold));
        }
    }
    rows
}

/// Upstream `buildPieces`: a line cut at every token and highlight boundary, each
/// piece with one token type and one changed state (byte offsets).
pub(crate) fn build_pieces(
    text: &str,
    tokens: &[SyntaxToken],
    emphasis: &[Range<usize>],
) -> Vec<(Range<usize>, Option<SharedString>, bool)> {
    let mut segments: Vec<(usize, usize, Option<SharedString>)> = Vec::new();
    let mut cursor = 0;
    for token in tokens {
        let start = token.start.max(cursor);
        let end = token.end.min(text.len());
        if start > cursor {
            segments.push((cursor, start, None));
        }
        if end > start {
            segments.push((start, end, Some(token.ty.to_string().into())));
        }
        cursor = cursor.max(end);
    }
    if cursor < text.len() {
        segments.push((cursor, text.len(), None));
    }
    let cuts: HashSet<usize> = emphasis.iter().flat_map(|r| [r.start, r.end]).collect();
    let changed = |offset: usize| emphasis.iter().any(|r| offset >= r.start && offset < r.end);
    let mut pieces = Vec::new();
    for (seg_start, seg_end, ty) in segments {
        let mut start = seg_start;
        for offset in seg_start + 1..seg_end {
            if cuts.contains(&offset) && text.is_char_boundary(offset) {
                pieces.push((start..offset, ty.clone(), changed(start)));
                start = offset;
            }
        }
        pieces.push((start..seg_end, ty, changed(start)));
    }
    pieces
}

/// Upstream `widestLine`: the line likely to render widest, tabs as two columns and
/// wide characters (from U+1100) as two.
pub fn widest_line<'a>(lines: impl IntoIterator<Item = &'a str>) -> &'a str {
    let mut widest = "";
    let mut widest_width: i64 = -1;
    for line in lines {
        let width: i64 = line
            .chars()
            .map(|c| {
                if c == '\t' || u32::from(c) >= 0x1100 {
                    2
                } else {
                    1
                }
            })
            .sum();
        if width > widest_width {
            widest = line;
            widest_width = width;
        }
    }
    widest
}

/// A press handler.
type PressFn = Rc<dyn Fn(&mut Window, &mut App)>;
type AnnotationFn = Rc<dyn Fn(&DiffViewerLineRef, &mut Window, &mut App) -> Option<AnyElement>>;
type LineClickFn = Rc<dyn Fn(&DiffViewerLineRef, &mut Window, &mut App)>;
type CollapsedFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Upstream `findAnnotatedHunks`: hunks with an annotation on a line a fold would
/// hide.
fn annotated_hunks(
    file: &DiffViewerFile,
    render: Option<&AnnotationFn>,
    window: &mut Window,
    cx: &mut App,
) -> HashSet<usize> {
    let mut out = HashSet::new();
    let Some(render) = render else { return out };
    for (h, hunk) in file.hunks.iter().enumerate() {
        let annotated = hunk.lines.iter().any(|line| {
            // A replaced block's fold keeps its context lines in view.
            if hunk.is_replaced_block && line.kind == DiffViewerLineKind::Context {
                return false;
            }
            line_ref(line, h).is_some_and(|r| render(&r, window, cx).is_some())
        });
        if annotated {
            out.insert(h);
        }
    }
    out
}

/// What the viewer keeps across frames.
#[derive(Default)]
struct ViewerState {
    collapsed: Option<bool>,
    /// The reader's file fold toggle, for the file it was made on.
    file_toggle: Option<(DiffViewerFile, bool)>,
    /// The reader's block toggles, for the file they were made on.
    block_toggles: Option<(DiffViewerFile, HashMap<usize, bool>)>,
    /// The sideways offset of unwrapped code, and its range.
    scroll_x: Pixels,
    max_x: Pixels,
}

/// Upstream lab `DiffViewer`: a read-only, syntax-highlighted diff of one file, in
/// unified or split view, with review-comment rows under any line and actionable line
/// numbers.
///
/// ```ignore
/// let file = parse_unified_diff(patch).remove(0);
/// DiffViewer::new("diff", file).view_mode(DiffViewerViewMode::Split)
/// ```
pub struct DiffViewer {
    id: ElementId,
    file: DiffViewerFile,
    view_mode: DiffViewerViewMode,
    language: Option<CodeLanguageInput>,
    code_languages: Vec<CodeLanguage>,
    line_numbers: bool,
    wrapped: bool,
    change_indicator: DiffViewerChangeIndicator,
    inline_highlight: DiffViewerInlineHighlight,
    change_background: bool,
    collapsible: bool,
    collapsed: Option<bool>,
    default_collapsed: bool,
    on_collapsed_change: Option<CollapsedFn>,
    header_end: Option<AnyElement>,
    render_line_annotation: Option<AnnotationFn>,
    on_line_number_click: Option<LineClickFn>,
    max_height: Option<Length>,
    syntax_theme: Option<SyntaxThemeDefinition>,
    style: StyleRefinement,
}

impl DiffViewer {
    /// Upstream `file`: paths, status, binary flag and hunks; build it with
    /// [`super::parse_unified_diff`] or [`super::create_diff_file`].
    pub fn new(id: impl Into<ElementId>, file: DiffViewerFile) -> Self {
        DiffViewer {
            id: id.into(),
            file,
            view_mode: DiffViewerViewMode::Unified,
            language: None,
            code_languages: Vec::new(),
            line_numbers: true,
            wrapped: false,
            change_indicator: DiffViewerChangeIndicator::Classic,
            inline_highlight: DiffViewerInlineHighlight::Word,
            change_background: true,
            collapsible: true,
            collapsed: None,
            default_collapsed: false,
            on_collapsed_change: None,
            header_end: None,
            render_line_annotation: None,
            on_line_number_click: None,
            max_height: None,
            syntax_theme: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `viewMode` (`Unified` by default).
    pub fn view_mode(mut self, mode: DiffViewerViewMode) -> Self {
        self.view_mode = mode;
        self
    }

    /// Upstream `language`: else one inferred from the file name.
    pub fn language(mut self, language: impl Into<CodeLanguageInput>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Upstream `codeLanguages`: languages the file name may infer; a later entry wins.
    pub fn code_languages(mut self, languages: impl IntoIterator<Item = CodeLanguage>) -> Self {
        self.code_languages = languages.into_iter().collect();
        self
    }

    /// Upstream `hasLineNumbers` (on by default).
    pub fn line_numbers(mut self, show: bool) -> Self {
        self.line_numbers = show;
        self
    }

    /// Upstream `isWrapped`: wraps long lines instead of scrolling them sideways.
    pub fn wrapped(mut self, wrapped: bool) -> Self {
        self.wrapped = wrapped;
        self
    }

    /// Upstream `changeIndicator` (`Classic` by default).
    pub fn change_indicator(mut self, indicator: DiffViewerChangeIndicator) -> Self {
        self.change_indicator = indicator;
        self
    }

    /// Upstream `inlineHighlight` (`Word` by default).
    pub fn inline_highlight(mut self, highlight: DiffViewerInlineHighlight) -> Self {
        self.inline_highlight = highlight;
        self
    }

    /// Upstream `hasChangeBackground` (on by default): tints changed rows.
    pub fn change_background(mut self, tint: bool) -> Self {
        self.change_background = tint;
        self
    }

    /// Upstream `isCollapsible` (on by default): the header is a disclosure when the
    /// file has a path to name it.
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible;
        self
    }

    /// Upstream `isCollapsed`: the controlled collapsed state.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = Some(collapsed);
        self
    }

    /// Upstream `defaultIsCollapsed`.
    pub fn default_collapsed(mut self, collapsed: bool) -> Self {
        self.default_collapsed = collapsed;
        self
    }

    /// Upstream `onCollapsedChange`.
    pub fn on_collapsed_change(
        mut self,
        f: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_collapsed_change = Some(Rc::new(f));
        self
    }

    /// Upstream `headerEnd`: content at the end of the file header.
    pub fn header_end(mut self, content: impl IntoElement) -> Self {
        self.header_end = Some(content.into_any_element());
        self
    }

    /// Upstream `renderLineAnnotation`: a full-width row under a line (`None` for
    /// none); in split view under the line's own side.
    pub fn render_line_annotation(
        mut self,
        f: impl Fn(&DiffViewerLineRef, &mut Window, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self {
        self.render_line_annotation = Some(Rc::new(f));
        self
    }

    /// Upstream `onLineNumberClick`: makes each line's anchor number a button.
    pub fn on_line_number_click(
        mut self,
        f: impl Fn(&DiffViewerLineRef, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_line_number_click = Some(Rc::new(f));
        self
    }

    /// Upstream `maxHeight`: the hunk area's height past which it scrolls.
    pub fn max_height(mut self, height: impl Into<Length>) -> Self {
        self.max_height = Some(height.into());
        self
    }

    /// Upstream `syntaxTheme`: a per-instance syntax theme.
    pub fn syntax_theme(mut self, theme: SyntaxThemeDefinition) -> Self {
        self.syntax_theme = Some(theme);
        self
    }
}

impl Styled for DiffViewer {
    /// The root's style (upstream `xstyle`).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for DiffViewer {
    type Element = AnyElement;

    fn into_element(self) -> AnyElement {
        let theme = self.syntax_theme.clone();
        let element = DiffViewerElement(std::cell::RefCell::new(Some(self))).into_any_element();
        match theme {
            Some(theme) => syntax_theme(theme, element),
            None => element,
        }
    }
}

#[derive(IntoElement)]
struct DiffViewerElement(std::cell::RefCell<Option<DiffViewer>>);

impl RenderOnce for DiffViewerElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let viewer = self.0.borrow_mut().take().expect("rendered once");
        render(viewer, window, cx)
    }
}

/// A line with its tabs drawn two columns wide, and where each byte of the line lands.
fn display_line(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len() + 1);
    for (i, c) in text.char_indices() {
        while map.len() < i {
            map.push(out.len());
        }
        map.push(out.len());
        if c == '\t' {
            out.push_str("  ");
        } else {
            out.push(c);
        }
    }
    while map.len() <= text.len() {
        map.push(out.len());
    }
    (out, map)
}

fn blend(color: Hsla) -> Hsla {
    Hsla {
        a: 1. - (1. - color.a) * (1. - color.a),
        ..color
    }
}

/// A line's text with its tokens painted and its changed ranges highlighted.
fn code_text(cell: &LineCell, colors: &SyntaxColors) -> StyledText {
    let content = cell.line.content.as_ref();
    if content.is_empty() {
        return StyledText::new("\u{200B}");
    }
    let (display, map) = display_line(content);
    let change = match cell.line.kind {
        DiffViewerLineKind::Added => Some(blend(colors.added)),
        DiffViewerLineKind::Removed => Some(blend(colors.removed)),
        DiffViewerLineKind::Context => None,
    };
    let highlights: Vec<(Range<usize>, HighlightStyle)> =
        build_pieces(content, &cell.tokens, &cell.emphasis)
            .into_iter()
            .filter_map(|(range, ty, changed)| {
                let color = ty.as_deref().and_then(|t| colors.token(t));
                let background = changed.then_some(change).flatten();
                (color.is_some() || background.is_some()).then(|| {
                    (
                        map[range.start]..map[range.end],
                        HighlightStyle {
                            color,
                            background_color: background,
                            ..Default::default()
                        },
                    )
                })
            })
            .filter(|(r, _)| r.start < r.end)
            .collect();
    StyledText::new(display).with_highlights(highlights)
}

/// A focusable control drawn as plain text (a line-number button, the header toggle).
fn text_button(
    id: ElementId,
    label: SharedString,
    window: &mut Window,
    cx: &mut App,
    on_press: PressFn,
) -> gpui::Stateful<gpui::Div> {
    let focus = control_focus(&id, window, cx);
    let ring = is_focus_visible(&focus, window);
    let theme = current(window, cx);
    let t = theme.tokens();
    let (click, key) = (on_press.clone(), on_press);
    div()
        .id(id)
        .track_focus(&focus)
        .role(Role::Button)
        .aria_label(label)
        .cursor_pointer()
        .rounded(t.radius.element)
        .border_1()
        .border_color(if ring {
            t.focus.outline_color
        } else {
            gpui::transparent_black()
        })
        .on_click(move |_, window, cx| click(window, cx))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
                key(window, cx);
            }
        })
}

#[allow(clippy::too_many_lines)]
fn render(props: DiffViewer, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let i18n = rustryx_i18n::current(cx);
    let colors = syntax_colors(window, cx);
    let id = props.id.clone();
    let file = props.file.clone();
    let path: Option<SharedString> = file.new_path.clone().or(file.old_path.clone());
    let language = props
        .language
        .clone()
        .unwrap_or_else(|| infer_language(path.as_deref(), &props.code_languages));
    let display_path: Option<SharedString> = match (&file.status, &file.old_path, &file.new_path) {
        (DiffViewerFileStatus::Renamed, Some(old), Some(new)) => Some(i18n.format(
            "@astryx.diffViewer.renamedPath",
            &[
                ("oldPath", Arg::from(old.as_ref())),
                ("newPath", Arg::from(new.as_ref())),
            ],
        )),
        _ => path.clone(),
    };
    let state = window.use_keyed_state(ElementId::Name(format!("{id}-diff").into()), cx, |_, _| {
        ViewerState::default()
    });
    let can_collapse = props.collapsible && display_path.is_some();
    let is_collapsed = can_collapse
        && props
            .collapsed
            .unwrap_or_else(|| state.read(cx).collapsed.unwrap_or(props.default_collapsed));

    let mut unfolded_changed = 0;
    let mut has_replaced = false;
    for hunk in &file.hunks {
        if hunk.is_replaced_block {
            has_replaced = true;
            continue;
        }
        unfolded_changed += hunk
            .lines
            .iter()
            .filter(|l| l.kind != DiffViewerLineKind::Context)
            .count();
    }
    let is_large = unfolded_changed > MAX_UNFOLDED_CHANGED_LINES;
    let annotated = if is_large || has_replaced {
        annotated_hunks(&file, props.render_line_annotation.as_ref(), window, cx)
    } else {
        HashSet::new()
    };
    // A large file and a replaced block start folded unless a line they would hide is
    // annotated; the reader's toggles are kept per file.
    let (file_toggle, block_toggles) = {
        let s = state.read(cx);
        (
            s.file_toggle
                .as_ref()
                .filter(|(f, _)| *f == file)
                .map(|(_, open)| *open),
            s.block_toggles
                .as_ref()
                .filter(|(f, _)| *f == file)
                .map(|(_, m)| m.clone()),
        )
    };
    let is_file_folded = is_large && !file_toggle.unwrap_or(!annotated.is_empty());
    let block_expanded = |h: usize| {
        block_toggles
            .as_ref()
            .and_then(|m| m.get(&h).copied())
            .unwrap_or(annotated.contains(&h))
    };
    let expanded: HashSet<usize> = file
        .hunks
        .iter()
        .enumerate()
        .filter(|(h, hunk)| hunk.is_replaced_block && block_expanded(*h))
        .map(|(h, _)| h)
        .collect();
    let cells = if is_file_folded {
        Vec::new()
    } else {
        build_cells(&file, &language, props.inline_highlight, &expanded)
    };
    let rows = if is_file_folded {
        Vec::new()
    } else if props.view_mode == DiffViewerViewMode::Split {
        split_rows(&file, &cells)
    } else {
        unified_rows(&file, &cells)
    };

    let mut added_count = 0;
    let mut removed_count = 0;
    let mut max_line_number: u64 = 1;
    for hunk in &file.hunks {
        for line in &hunk.lines {
            match line.kind {
                DiffViewerLineKind::Added => added_count += 1,
                DiffViewerLineKind::Removed => removed_count += 1,
                DiffViewerLineKind::Context => {}
            }
            max_line_number = max_line_number
                .max(line.old_line_number.unwrap_or(0))
                .max(line.new_line_number.unwrap_or(0));
        }
    }
    let digits = max_line_number.to_string().len();

    // Metrics: the code font's `ch`, the line height.
    let font_size = t.text.code.size.to_pixels(rem);
    let code_font = gpui::font(SharedString::from(
        t.font
            .family
            .code
            .named()
            .next()
            .unwrap_or("monospace")
            .to_string(),
    ));
    let ch = window
        .text_system()
        .shape_line(
            "0".into(),
            font_size,
            &[gpui::TextRun {
                len: 1,
                font: code_font,
                color: t.color.text_primary,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
        .width;
    let line_height = font_size * t.text.code.leading;
    let s1 = t.spacing.s1.to_pixels(rem);
    let s2 = t.spacing.s2.to_pixels(rem);
    let s3 = t.spacing.s3.to_pixels(rem);
    let s4 = t.spacing.s4.to_pixels(rem);
    let gutter_width = ch * digits as f32 + s2 * 2.;
    let marker_width = match props.change_indicator {
        DiffViewerChangeIndicator::Classic => ch + s1 + s2,
        DiffViewerChangeIndicator::Bars => s3,
        DiffViewerChangeIndicator::None => s2,
    };
    let bar_width = t.border.width.to_pixels(rem) * 3.;
    let wrapped = props.wrapped;
    let (scroll_x, _) = {
        let s = state.read(cx);
        (s.scroll_x.min(s.max_x).max(px(0.)), s.max_x)
    };
    // The widest code and the narrowest code view this frame set the sideways range.
    let widest = Rc::new(Cell::new(px(0.)));
    let narrowest = Rc::new(Cell::new(px(f32::MAX)));

    let tint = |kind: Option<DiffViewerLineKind>| -> Option<Hsla> {
        if !props.change_background {
            return None;
        }
        match kind {
            Some(DiffViewerLineKind::Added) => Some(colors.added),
            Some(DiffViewerLineKind::Removed) => Some(colors.removed),
            _ => None,
        }
    };

    let on_click = props.on_line_number_click.clone();
    let gutter_cell = |cell: Option<&LineCell>,
                       side: DiffViewerSide,
                       key: &str,
                       window: &mut Window,
                       cx: &mut App|
     -> Option<AnyElement> {
        if !props.line_numbers {
            return None;
        }
        let number = cell.and_then(|c| match side {
            DiffViewerSide::Old => c.line.old_line_number,
            DiffViewerSide::New => c.line.new_line_number,
        });
        let kind = cell.map(|c| c.line.kind);
        let color = match kind {
            Some(DiffViewerLineKind::Added) => colors.added_marker,
            Some(DiffViewerLineKind::Removed) => colors.removed_marker,
            _ => colors.punctuation,
        };
        let mut el = div()
            .flex_none()
            .w(gutter_width)
            .px(s2)
            .flex()
            .justify_end()
            .text_color(color)
            .bg(if cell.is_none() {
                colors.diff_header
            } else {
                colors.background
            });
        if let Some(bg) = tint(kind) {
            el = el.child(div().absolute().size_full().bg(bg));
        }
        // Only the line's own anchor side is a button: a context line's old number
        // stays plain text.
        let action = cell
            .filter(|c| number.is_some() && c.line_ref.side == side)
            .map(|c| c.line_ref.clone());
        let content: AnyElement = match (number, action, on_click.clone()) {
            (Some(n), Some(line_ref), Some(f)) => {
                let label = i18n.format(
                    "@astryx.diffViewer.lineNumberButton",
                    &[
                        ("side", Arg::from(side.as_str())),
                        ("line", Arg::Num(n as f64)),
                    ],
                );
                let button_id =
                    ElementId::Name(format!("{id}-line-{}-{}", side.as_str(), n).into());
                let selector = format!("{id}-line-{}-{}", side.as_str(), n);
                text_button(
                    button_id,
                    label,
                    window,
                    cx,
                    Rc::new(move |w, cx| f(&line_ref, w, cx)),
                )
                .debug_selector(move || selector)
                .hover(|s| s.underline())
                .child(n.to_string())
                .into_any_element()
            }
            (Some(n), _, _) => div().child(n.to_string()).into_any_element(),
            _ => div().into_any_element(),
        };
        let _ = key;
        Some(el.relative().child(content).into_any_element())
    };

    let marker_cell = |cell: Option<&LineCell>| -> AnyElement {
        let kind = cell.map(|c| c.line.kind);
        let is_change = matches!(
            kind,
            Some(DiffViewerLineKind::Added | DiffViewerLineKind::Removed)
        );
        let marker_color = match kind {
            Some(DiffViewerLineKind::Added) => colors.added_marker,
            Some(DiffViewerLineKind::Removed) => colors.removed_marker,
            _ => t.color.text_secondary,
        };
        let marker_id = ElementId::Name(
            format!(
                "{id}-marker-{}",
                cell.map_or_else(String::new, |c| format!(
                    "{}-{}",
                    c.line_ref.side.as_str(),
                    c.line_ref.line_number
                ))
            )
            .into(),
        );
        let mut el = div()
            .id(marker_id)
            .relative()
            .flex_none()
            .w(marker_width)
            .pl(s1)
            .pr(s2)
            .text_color(marker_color)
            .bg(if cell.is_none() {
                colors.diff_header
            } else {
                colors.background
            });
        if let Some(bg) = tint(kind) {
            el = el.child(div().absolute().top_0().left_0().size_full().bg(bg));
        }
        if is_change {
            // Screen readers hear "Added" or "Removed" in every mode.
            let label = if kind == Some(DiffViewerLineKind::Added) {
                i18n.t("@astryx.diffViewer.added")
            } else {
                i18n.t("@astryx.diffViewer.removed")
            };
            el = el.role(Role::Image).aria_label(label);
            match props.change_indicator {
                DiffViewerChangeIndicator::Classic => {
                    el = el.child(div().relative().child(
                        if kind == Some(DiffViewerLineKind::Added) {
                            "+"
                        } else {
                            "-"
                        },
                    ));
                }
                DiffViewerChangeIndicator::Bars => {
                    let added = kind == Some(DiffViewerLineKind::Added);
                    let dash = t.border.width.to_pixels(rem) * 2.;
                    let color = marker_color;
                    el = el.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds: Bounds<Pixels>, _, window, _| {
                                let bar = Bounds::new(
                                    bounds.origin,
                                    gpui::size(bar_width, bounds.size.height),
                                );
                                if added {
                                    window.paint_quad(gpui::fill(bar, color));
                                    return;
                                }
                                // A dashed bar: on and off in steps of twice the border width.
                                let mut y = bar.origin.y;
                                while y < bar.bottom() {
                                    let h = dash.min(bar.bottom() - y);
                                    window.paint_quad(gpui::fill(
                                        Bounds::new(
                                            gpui::point(bar.origin.x, y),
                                            gpui::size(bar_width, h),
                                        ),
                                        color,
                                    ));
                                    y += dash * 2.;
                                }
                            },
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    );
                }
                DiffViewerChangeIndicator::None => {}
            }
        }
        el.into_any_element()
    };

    let code_cell = |cell: Option<&LineCell>| -> AnyElement {
        let kind = cell.map(|c| c.line.kind);
        let mut el = div()
            .relative()
            .flex_1()
            .min_w_0()
            .pr(s4)
            .bg(if cell.is_none() {
                colors.diff_header
            } else {
                colors.background
            });
        if let Some(bg) = tint(kind) {
            el = el.child(div().absolute().top_0().left_0().size_full().bg(bg));
        }
        let Some(cell) = cell else {
            return el.into_any_element();
        };
        let selector = format!(
            "{id}-code-{}-{}",
            cell.line_ref.side.as_str(),
            cell.line_ref.line_number
        );
        let text = code_text(cell, &colors);
        if wrapped {
            return el
                .debug_selector(move || selector)
                .child(div().relative().child(text))
                .into_any_element();
        }
        let (w, nw) = (widest.clone(), narrowest.clone());
        el.overflow_hidden()
            .debug_selector(move || selector)
            .child(
                canvas(
                    move |bounds: Bounds<Pixels>, _, _| nw.set(nw.get().min(bounds.size.width)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(
                div()
                    .relative()
                    .flex_none()
                    .whitespace_nowrap()
                    .left(-scroll_x)
                    .child(text)
                    .child(
                        canvas(
                            move |bounds: Bounds<Pixels>, _, _| {
                                w.set(w.get().max(bounds.size.width))
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
            )
            .into_any_element()
    };

    let annotation_for =
        |cell: Option<&LineCell>, window: &mut Window, cx: &mut App| -> Option<AnyElement> {
            let cell = cell?;
            let render = props.render_line_annotation.as_ref()?;
            render(&cell.line_ref, window, cx)
        };
    let annotation_row = |note: Option<AnyElement>| -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .py(s2)
            .px(s3)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .text_color(t.color.text_primary)
            .bg(t.color.background_surface)
            .border_t(t.border.width)
            .border_b(t.border.width)
            .border_color(t.color.border)
            .children(note)
            .into_any_element()
    };

    let toggle_block = {
        let (state, file) = (state.clone(), file.clone());
        Rc::new(move |h: usize, current: bool, cx: &mut App| {
            state.update(cx, |s, cx| {
                let mut map = s
                    .block_toggles
                    .as_ref()
                    .filter(|(f, _)| *f == file)
                    .map(|(_, m)| m.clone())
                    .unwrap_or_default();
                map.insert(h, !current);
                s.block_toggles = Some((file.clone(), map));
                cx.notify();
            })
        })
    };
    let fold_control =
        |changed: usize, is_expanded: bool, key: String, on_toggle: PressFn| -> AnyElement {
            let selector = format!("{id}-fold-{key}");
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(s3)
                .child(i18n.format(
                    "@astryx.diffViewer.largeChange",
                    &[("count", Arg::Num(changed as f64))],
                ))
                .child(
                    div().debug_selector(move || selector).child(
                        Button::new(
                            ElementId::Name(format!("{id}-fold-{key}").into()),
                            if is_expanded {
                                i18n.t("@astryx.diffViewer.hideFullDiff")
                            } else {
                                i18n.t("@astryx.diffViewer.showFullDiff")
                            },
                        )
                        .size(ElementSize::Sm)
                        .variant(ButtonVariant::Secondary)
                        .expanded(is_expanded)
                        .on_click(move |_, w, cx| on_toggle(w, cx)),
                    ),
                )
                .into_any_element()
        };
    let band = |child: AnyElement, fold: bool| -> AnyElement {
        let mut el = div()
            .w_full()
            .py(if fold { s2 } else { s1 })
            .px(s3)
            .bg(colors.diff_header)
            .border_t(t.border.width)
            .border_b(t.border.width)
            .border_color(t.color.border);
        el = if fold {
            el.text_color(t.color.text_secondary)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.supporting)
        } else {
            el.text_color(colors.punctuation).whitespace_nowrap()
        };
        el.child(child).into_any_element()
    };

    // The rows.
    let mut table = div()
        .flex()
        .flex_col()
        .w_full()
        .font_stack(&t.font.family.code)
        .text_size(font_size)
        .line_height(line_height)
        .text_color(colors.variable);
    for (r, row) in rows.iter().enumerate() {
        match row {
            Row::Hunk(header) => {
                table = table.child(band(div().child(header.clone()).into_any_element(), false))
            }
            Row::Fold(fold) => {
                let (toggle, f) = (toggle_block.clone(), *fold);
                table = table.child(band(
                    fold_control(
                        fold.changed_lines,
                        fold.expanded,
                        format!("h{}", fold.hunk_index),
                        Rc::new(move |_, cx| toggle(f.hunk_index, f.expanded, cx)),
                    ),
                    true,
                ));
            }
            Row::Line(cell) => {
                let note = annotation_for(Some(cell), window, cx);
                let old_gutter = gutter_cell(
                    Some(cell),
                    DiffViewerSide::Old,
                    &format!("{r}o"),
                    window,
                    cx,
                );
                let new_gutter = gutter_cell(
                    Some(cell),
                    DiffViewerSide::New,
                    &format!("{r}n"),
                    window,
                    cx,
                );
                table = table.child(
                    div()
                        .flex()
                        .w_full()
                        .children(old_gutter)
                        .children(new_gutter)
                        .child(marker_cell(Some(cell)))
                        .child(code_cell(Some(cell))),
                );
                if let Some(note) = note {
                    table = table.child(div().flex().w_full().child(annotation_row(Some(note))));
                }
            }
            Row::Pair(old, new) => {
                // A context line anchors to its new side, so only that side carries its
                // annotation.
                let old_note = old
                    .as_ref()
                    .filter(|c| c.line.kind != DiffViewerLineKind::Context)
                    .and_then(|c| annotation_for(Some(c), window, cx));
                let new_note = annotation_for(new.as_ref(), window, cx);
                let half = |cell: Option<&LineCell>,
                            side: DiffViewerSide,
                            window: &mut Window,
                            cx: &mut App| {
                    let gutter =
                        gutter_cell(cell, side, &format!("{r}{}", side.as_str()), window, cx);
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .children(gutter)
                        .child(marker_cell(cell))
                        .child(code_cell(cell))
                };
                let left = half(old.as_ref(), DiffViewerSide::Old, window, cx)
                    .border_r(t.border.width)
                    .border_color(t.color.border);
                let right = half(new.as_ref(), DiffViewerSide::New, window, cx);
                table = table.child(div().flex().w_full().child(left).child(right));
                if old_note.is_some() || new_note.is_some() {
                    table = table.child(
                        div()
                            .flex()
                            .w_full()
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_w_0()
                                    .border_r(t.border.width)
                                    .border_color(t.color.border)
                                    .child(annotation_row(old_note)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(annotation_row(new_note)),
                            ),
                    );
                }
            }
        }
    }

    // The body.
    let message = |text: SharedString| {
        div()
            .py(s3)
            .px(s4)
            .text_color(t.color.text_secondary)
            .text_style(&t.text.supporting)
            .child(text)
            .into_any_element()
    };
    let body: AnyElement = if file.is_binary {
        message(i18n.t("@astryx.diffViewer.binary"))
    } else if file.hunks.is_empty() {
        message(i18n.t("@astryx.diffViewer.noChanges"))
    } else {
        // The fold row stays first whether the file is folded or not, so its button
        // keeps focus when it toggles.
        let file_fold = is_large.then(|| {
            let (state, f) = (state.clone(), file.clone());
            let folded = is_file_folded;
            band(
                fold_control(
                    unfolded_changed,
                    !is_file_folded,
                    "file".to_string(),
                    Rc::new(move |_, cx| {
                        state.update(cx, |s, cx| {
                            s.file_toggle = Some((f.clone(), folded));
                            cx.notify();
                        })
                    }),
                ),
                true,
            )
        });
        let viewport = (!is_file_folded).then(|| {
            let wheel_state = state.clone();
            let mut viewport = div()
                .id(ElementId::Name(format!("{id}-viewport").into()))
                .overflow_y_scroll()
                .w_full()
                .debug_selector({
                    let s = format!("{id}-viewport");
                    move || s
                })
                .child(table);
            if !wrapped {
                viewport = viewport.on_scroll(move |event: &ScrollEvent, _, cx| {
                    let delta = event.delta.pixel_delta(line_height);
                    let dx = if event.modifiers.shift && delta.x == px(0.) {
                        delta.y
                    } else {
                        delta.x
                    };
                    if dx.abs() <= delta.y.abs() && !event.modifiers.shift {
                        return;
                    }
                    wheel_state.update(cx, |s, cx| {
                        let next = (s.scroll_x - dx).clamp(px(0.), s.max_x.max(px(0.)));
                        if next != s.scroll_x {
                            s.scroll_x = next;
                            cx.notify();
                        }
                    });
                    cx.stop_propagation();
                });
            }
            if let Some(max) = props.max_height {
                viewport = viewport.max_h(max);
            }
            // After drawing: how far the widest line reaches past the narrowest view.
            let (measure_state, w, nw) = (state.clone(), widest.clone(), narrowest.clone());
            viewport = viewport.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, _, cx| {
                        let max = (w.get() - nw.get()).max(px(0.));
                        if nw.get() < px(f32::MAX) && measure_state.read(cx).max_x != max {
                            measure_state.update(cx, |s, cx| {
                                s.max_x = max;
                                s.scroll_x = s.scroll_x.min(max);
                                cx.notify();
                            });
                        }
                    },
                )
                .absolute()
                .size_0(),
            );
            viewport.relative()
        });
        div()
            .flex()
            .flex_col()
            .w_full()
            .children(file_fold)
            .children(viewport)
            .into_any_element()
    };

    // The header.
    let status_token = match file.status {
        DiffViewerFileStatus::Added => {
            Some((TokenColor::Green, i18n.t("@astryx.diffViewer.statusAdded")))
        }
        DiffViewerFileStatus::Deleted => {
            Some((TokenColor::Red, i18n.t("@astryx.diffViewer.statusDeleted")))
        }
        DiffViewerFileStatus::Renamed => {
            Some((TokenColor::Blue, i18n.t("@astryx.diffViewer.statusRenamed")))
        }
        DiffViewerFileStatus::Modified => None,
    };
    let path_el = display_path.clone().map(|p| {
        let mut el = div()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .font_stack(&t.font.family.code)
            .font_weight(t.font.weight.medium)
            .child(p);
        theme.refine(el.style(), "diff-viewer-path", &[], None);
        el
    });
    let header = (display_path.is_some() || props.header_end.is_some()).then(|| {
        let mut header = div()
            .flex()
            .items_center()
            .gap(s2)
            .min_w_0()
            .py(s2)
            .px(s3)
            .bg(t.color.background_surface)
            .text_color(t.color.text_primary)
            .text_style(&t.text.supporting);
        if !is_collapsed {
            header = header.border_b(t.border.width).border_color(t.color.border);
        }
        theme.refine(header.style(), "diff-viewer-header", &[], None);
        if can_collapse {
            let (state, report) = (state.clone(), props.on_collapsed_change.clone());
            let controlled = props.collapsed.is_some();
            let next = !is_collapsed;
            let label = display_path.clone().unwrap_or_default();
            let selector = format!("{id}-toggle");
            header = header.child(
                text_button(
                    ElementId::Name(format!("{id}-toggle").into()),
                    label,
                    window,
                    cx,
                    Rc::new(move |window, cx| {
                        if !controlled {
                            state.update(cx, |s, cx| {
                                s.collapsed = Some(next);
                                cx.notify();
                            });
                        }
                        if let Some(f) = &report {
                            f(next, window, cx);
                        }
                    }),
                )
                .debug_selector(move || selector)
                .aria_expanded(!is_collapsed)
                .flex()
                .items_center()
                .gap(s1)
                .min_w_0()
                .child(
                    Icon::new("chevronRight")
                        .size(IconSize::Xsm)
                        .color(IconColor::Secondary)
                        .rotation(if is_collapsed {
                            0.
                        } else {
                            std::f32::consts::FRAC_PI_2
                        }),
                )
                .children(path_el),
            );
        } else {
            header = header.children(path_el);
        }
        if let Some((color, label)) = status_token {
            header = header.child(
                Token::new(ElementId::Name(format!("{id}-status").into()), label)
                    .size(ElementSize::Sm)
                    .color(color),
            );
        }
        if added_count > 0 || removed_count > 0 {
            header = header.child(
                div()
                    .id(ElementId::Name(format!("{id}-stats").into()))
                    .flex()
                    .flex_none()
                    .gap(s1)
                    .font_stack(&t.font.family.code)
                    .aria_label(i18n.format(
                        "@astryx.diffViewer.stats",
                        &[
                            ("added", Arg::Num(f64::from(added_count))),
                            ("removed", Arg::Num(f64::from(removed_count))),
                        ],
                    ))
                    .child(div().text_color(t.color.text_green).child(i18n.format(
                        "@astryx.diffViewer.addedCount",
                        &[("count", Arg::Num(f64::from(added_count)))],
                    )))
                    .child(div().text_color(t.color.text_red).child(i18n.format(
                        "@astryx.diffViewer.removedCount",
                        &[("count", Arg::Num(f64::from(removed_count)))],
                    ))),
            );
        }
        if let Some(end) = props.header_end {
            header = header.child(
                div()
                    .flex()
                    .items_center()
                    .gap(s2)
                    .ml_auto()
                    .flex_none()
                    .child(end),
            );
        }
        header
    });

    let mut root = div()
        .id(id.clone())
        .flex()
        .flex_col()
        .min_w_0()
        .overflow_hidden()
        .rounded(t.radius.element)
        .border(t.border.width)
        .border_color(t.color.border)
        .bg(colors.background)
        .role(Role::Group)
        .aria_label(
            display_path
                .clone()
                .unwrap_or_else(|| i18n.t("@astryx.diffViewer.label")),
        )
        .debug_selector({
            let s = id.to_string();
            move || s
        });
    theme.refine(
        root.style(),
        "diff-viewer",
        &[(
            "viewMode",
            if props.view_mode == DiffViewerViewMode::Split {
                "split"
            } else {
                "unified"
            },
        )],
        None,
    );
    root.style().refine(&props.style);
    root.children(header)
        .children((!is_collapsed).then_some(body))
        .into_any_element()
}
