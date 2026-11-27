//! CodeEditor (upstream lab `CodeEditor/CodeEditor.tsx`): an editable code input
//! with syntax paint, line numbers, a read-only state, a placeholder, and custom
//! tokenizers. It is built on the repo's editing engine
//! (`astryx_primitives::editing`): the engine draws the text, caret and selection,
//! and the tokens paint its text through `TextEditState::highlights` (upstream's CSS
//! Custom Highlight ranges). Tab indents, Shift+Tab outdents, Enter keeps the line's
//! indentation, and brackets and quotes close themselves. Because Tab is captured for
//! indentation, Escape arms a one-shot "Tab moves focus" mode so keyboard users can
//! leave the editor (WCAG 2.1.2).

pub mod examples;

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use astryx_components::syntax_theme::syntax_colors;
use astryx_primitives::editing::{EditorStyle, FieldSettings, TextEditState, text_editor};
use astryx_primitives::prefers_reduced_motion;
use astryx_syntax::{
    CodeLanguageInput, SYNC_TOKENIZE_THRESHOLD, TokenLine, language_key, tokenize,
};
use astryx_theme::{ThemeStyled, current};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, Entity, Focusable, Hsla, KeyDownEvent, Length, Role, ScrollHandle,
    SharedString, StyleRefinement, Window, div, px, relative,
};

/// Upstream `size`: the code's text size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeEditorSize {
    Sm,
    /// The default.
    #[default]
    Md,
}

impl CodeEditorSize {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Md => "md",
        }
    }
}

/// Upstream `tokenizer`: `(code, language)` to each line's tokens.
pub type CodeEditorTokenizer = Arc<dyn Fn(&str, &str) -> Vec<TokenLine> + Send + Sync>;

type ChangeFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// Upstream's default `escapeHint`, in English: the lab package has no catalog for
/// it, so the prop localizes it.
pub const DEFAULT_ESCAPE_HINT: &str = "Press Escape then Tab to move focus out of the editor.";

/// Upstream `AUTO_CLOSE_PAIRS`: a typed opener inserts its closer after the caret.
pub fn auto_close_pair(open: &str) -> Option<&'static str> {
    Some(match open {
        "(" => ")",
        "[" => "]",
        "{" => "}",
        "\"" => "\"",
        "'" => "'",
        "`" => "`",
        _ => return None,
    })
}

/// JavaScript's `\s`.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Upstream's Enter rule: the indentation (`/^(\s*)/`) of the line the caret is on,
/// up to the caret (a byte offset).
pub fn indent_before(text: &str, cursor: usize) -> &str {
    let before = &text[..cursor];
    let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    let end = line
        .char_indices()
        .find(|(_, c)| !is_js_space(*c))
        .map_or(line.len(), |(i, _)| i);
    &line[..end]
}

/// Upstream's Shift+Tab rule: the text with up to two leading spaces removed from the
/// caret's line, and the caret after it; `None` when the line has none. The line
/// starts after the last line break before the caret, as `lastIndexOf('\n',
/// cursor - 1) + 1` finds it (a caret at 0 before a leading break reads the next
/// line, as there).
pub fn outdent(text: &str, cursor: usize) -> Option<(String, usize)> {
    let line_start = if cursor == 0 {
        usize::from(text.starts_with('\n'))
    } else {
        text[..cursor].rfind('\n').map_or(0, |i| i + 1)
    };
    let line = &text[line_start..];
    let remove = if line.starts_with("  ") {
        2
    } else if line.starts_with(' ') {
        1
    } else {
        0
    };
    if remove == 0 {
        return None;
    }
    let next = format!("{}{}", &text[..line_start], &text[line_start + remove..]);
    let removed_before = remove.min(cursor.saturating_sub(line_start));
    Some((next, cursor - removed_before))
}

/// Upstream `applyHighlightRangesFlat`'s offsets: each line's tokens as ranges of the
/// whole text, the next line starting after the line break that ends this one. A
/// token reaching past the text is skipped.
pub fn flat_ranges(text: &str, lines: &[TokenLine]) -> Vec<(Range<usize>, SharedString)> {
    let mut out = Vec::new();
    let mut line_offset = 0usize;
    for tokens in lines {
        for token in tokens {
            let (start, end) = (line_offset + token.start, line_offset + token.end);
            if end > text.len()
                || start > end
                || !text.is_char_boundary(start)
                || !text.is_char_boundary(end)
            {
                continue;
            }
            out.push((start..end, SharedString::from(token.ty.to_string())));
        }
        line_offset = match text.get(line_offset..).and_then(|rest| rest.find('\n')) {
            Some(i) => line_offset + i + 1,
            None => text.len(),
        };
    }
    out
}

/// What the editor keeps across frames.
#[derive(Default)]
struct EditorState {
    /// The tokens of the last text, by its key, once tokenized.
    tokens: Option<(u64, Rc<Vec<TokenLine>>)>,
    /// A long text being tokenized in the background.
    pending: Option<u64>,
}

fn source_key(text: &str, language: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    language.hash(&mut h);
    h.finish()
}

/// Upstream lab `CodeEditor`: an editable code input, controlled.
///
/// ```ignore
/// CodeEditor::new("source", "Source code", code.clone(), |next, _, cx| save(next, cx))
///     .language("typescript")
///     .line_numbers(true)
/// ```
pub struct CodeEditor {
    id: ElementId,
    label: SharedString,
    value: SharedString,
    on_change: ChangeFn,
    language: CodeLanguageInput,
    line_numbers: bool,
    read_only: bool,
    placeholder: Option<SharedString>,
    max_height: Option<Length>,
    size: CodeEditorSize,
    tokenizer: Option<CodeEditorTokenizer>,
    escape_hint: SharedString,
    style: StyleRefinement,
}

impl CodeEditor {
    /// Upstream `label` (the editable region's accessible name), `value` and
    /// `onChange`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        on_change: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        CodeEditor {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            on_change: Rc::new(on_change),
            language: CodeLanguageInput::from("plaintext"),
            line_numbers: false,
            read_only: false,
            placeholder: None,
            max_height: None,
            size: CodeEditorSize::Md,
            tokenizer: None,
            escape_hint: DEFAULT_ESCAPE_HINT.into(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `language` (`plaintext` by default): a built-in language's name or a
    /// language module.
    pub fn language(mut self, language: impl Into<CodeLanguageInput>) -> Self {
        self.language = language.into();
        self
    }

    /// Upstream `hasLineNumbers`: a gutter of line numbers.
    pub fn line_numbers(mut self, show: bool) -> Self {
        self.line_numbers = show;
        self
    }

    /// Upstream `isReadOnly`: the code can be read, selected and copied, not edited.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Upstream `placeholder`: shown while the value is empty.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Upstream `maxHeight`: the height past which the editor scrolls.
    pub fn max_height(mut self, height: impl Into<Length>) -> Self {
        self.max_height = Some(height.into());
        self
    }

    /// Upstream `size` (`Md` by default).
    pub fn size(mut self, size: CodeEditorSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `tokenizer`: replaces the built-in tokenizer for code shorter than
    /// `SYNC_TOKENIZE_THRESHOLD` characters (upstream tokenizes longer code with the
    /// built-in grammar in the background).
    pub fn tokenizer(
        mut self,
        f: impl Fn(&str, &str) -> Vec<TokenLine> + Send + Sync + 'static,
    ) -> Self {
        self.tokenizer = Some(Arc::new(f));
        self
    }

    /// Upstream `escapeHint`: the hint, given to assistive technology as the
    /// editor's description, that says how to leave the editor.
    pub fn escape_hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.escape_hint = hint.into();
        self
    }
}

impl Styled for CodeEditor {
    /// The root's style (upstream `xstyle`, `className`, `style`).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for CodeEditor {
    type Element = AnyElement;

    fn into_element(self) -> AnyElement {
        CodeEditorElement(std::cell::RefCell::new(Some(self))).into_any_element()
    }
}

#[derive(IntoElement)]
struct CodeEditorElement(std::cell::RefCell<Option<CodeEditor>>);

impl RenderOnce for CodeEditorElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let editor = self.0.borrow_mut().take().expect("rendered once");
        render(editor, window, cx)
    }
}

/// The tokens for `text`: short code at once with the editor's tokenizer, long code
/// with the built-in grammar in the background (none until they arrive).
fn tokens_for(
    text: &str,
    language: &CodeLanguageInput,
    tokenizer: Option<&CodeEditorTokenizer>,
    state: &Entity<EditorState>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Rc<Vec<TokenLine>>> {
    let name = language_key(language).to_string();
    let key = source_key(text, &name);
    if let Some((k, tokens)) = state.read(cx).tokens.clone()
        && k == key
    {
        return Some(tokens);
    }
    if text.chars().count() < SYNC_TOKENIZE_THRESHOLD {
        let lines = match tokenizer {
            Some(f) => f(text, &name),
            None => tokenize(text, language),
        };
        let lines = Rc::new(lines);
        state.update(cx, |s, _| s.tokens = Some((key, lines.clone())));
        return Some(lines);
    }
    if state.read(cx).pending != Some(key) {
        state.update(cx, |s, _| s.pending = Some(key));
        let task = cx.background_executor().spawn({
            let (text, language) = (text.to_string(), language.clone());
            async move { tokenize(&text, &language) }
        });
        let state = state.clone();
        window
            .spawn(cx, async move |cx| {
                let lines = task.await;
                cx.update(|window, cx| {
                    state.update(cx, |s, _| {
                        if s.pending == Some(key) {
                            s.pending = None;
                            s.tokens = Some((key, Rc::new(lines)));
                        }
                    });
                    window.refresh();
                })
                .ok();
            })
            .detach();
    }
    None
}

/// Upstream `handleKeyDown`: Escape arms "Tab moves focus"; Tab indents and
/// Shift+Tab outdents unless that mode is armed (one-shot); any other key disarms it;
/// Enter keeps the line's indentation; an opener with no selection closes itself.
/// Returns whether the key was taken (upstream's `preventDefault`).
fn key_down(
    event: &KeyDownEvent,
    editor: &Entity<TextEditState>,
    tab_moves_focus: &Cell<bool>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let keys = &event.keystroke;
    let key = keys.key.as_str();
    if key == "escape" {
        tab_moves_focus.set(true);
        return false;
    }
    if key == "tab" {
        // Tab-moves-focus mode: Tab and Shift+Tab move focus out. One-shot.
        if tab_moves_focus.replace(false) {
            return false;
        }
        let text = editor.read(cx).text().to_string();
        let cursor = editor.read(cx).buffer().selection().start;
        if keys.modifiers.shift {
            if let Some((_, caret)) = outdent(&text, cursor) {
                let line_start = if cursor == 0 {
                    usize::from(text.starts_with('\n'))
                } else {
                    text[..cursor].rfind('\n').map_or(0, |i| i + 1)
                };
                let remove = if text[line_start..].starts_with("  ") {
                    2
                } else {
                    1
                };
                editor.update(cx, |s, cx| {
                    s.replace_range(line_start..line_start + remove, "", window, cx);
                    s.select_range(caret..caret, false, window, cx);
                });
            }
            return true;
        }
        editor.update(cx, |s, cx| s.insert_text("  ", window, cx));
        return true;
    }
    // Any other key re-arms Tab indentation (modifier presses are not keystrokes).
    tab_moves_focus.set(false);
    if key == "enter" {
        let text = editor.read(cx).text().to_string();
        let cursor = editor.read(cx).buffer().selection().start;
        let insert = format!("\n{}", indent_before(&text, cursor));
        editor.update(cx, |s, cx| s.insert_text(&insert, window, cx));
        return true;
    }
    let typed = keys.key_char.clone().unwrap_or_else(|| keys.key.clone());
    if keys.modifiers.control || keys.modifiers.platform || keys.modifiers.alt {
        return false;
    }
    if let Some(close) = auto_close_pair(&typed) {
        // Only with no text selected.
        if !editor.read(cx).buffer().is_collapsed() {
            return false;
        }
        let cursor = editor.read(cx).buffer().cursor();
        editor.update(cx, |s, cx| {
            s.insert_text(&format!("{typed}{close}"), window, cx);
            let caret = cursor + typed.len();
            s.select_range(caret..caret, false, window, cx);
        });
        return true;
    }
    false
}

fn render(props: CodeEditor, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let id = props.id.clone();
    let font_size = match props.size {
        CodeEditorSize::Sm => t.font.size.sm,
        CodeEditorSize::Md => t.font.size.base,
    }
    .to_pixels(rem);
    let leading = t.text.code.leading;
    let editor = window.use_keyed_state(
        ElementId::Name(format!("{id}-editor").into()),
        cx,
        |window, cx| TextEditState::reporting(props.value.to_string(), true, window, cx),
    );
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-tokens").into()),
        cx,
        |_, _| EditorState::default(),
    );
    let tab_moves_focus = window
        .use_keyed_state(
            ElementId::Name(format!("{id}-tab-mode").into()),
            cx,
            |_, _| Rc::new(Cell::new(false)),
        )
        .read(cx)
        .clone();
    let scroll = window
        .use_keyed_state(
            ElementId::Name(format!("{id}-scroll").into()),
            cx,
            |_, _| ScrollHandle::new(),
        )
        .read(cx)
        .clone();

    // Upstream paints with the CSS Custom Highlight API; here the tokens color the
    // engine's text runs.
    let colors = syntax_colors(window, cx);
    let tokens = tokens_for(
        &props.value,
        &props.language,
        props.tokenizer.as_ref(),
        &state,
        window,
        cx,
    );
    let highlights: Vec<(Range<usize>, Hsla)> = tokens
        .map(|lines| {
            flat_ranges(&props.value, &lines)
                .into_iter()
                .filter_map(|(range, ty)| colors.token(&ty).map(|color| (range, color)))
                .collect()
        })
        .unwrap_or_default();
    let read_only = props.read_only;
    let on_change = props.on_change.clone();
    let tab_mode = tab_moves_focus.clone();
    let blur_mode = tab_moves_focus.clone();
    editor.update(cx, |s, cx| {
        s.set_text(&props.value, cx);
        s.set_settings(FieldSettings {
            read_only,
            spell_check: false,
            ..FieldSettings::default()
        });
        s.no_wrap = true;
        s.highlights = highlights;
        s.on_change = Some(Rc::new(move |text, window, cx| {
            on_change(text.into(), window, cx)
        }));
        let entity = cx.entity();
        s.on_key_down = (!read_only).then(|| {
            Rc::new(
                move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                    // An input method composing a character owns its keys.
                    if entity.read(cx).buffer().marked().is_some() {
                        return;
                    }
                    if key_down(event, &entity, &tab_mode, window, cx) {
                        cx.stop_propagation();
                    }
                },
            ) as astryx_primitives::editing::KeyDownFn
        });
        s.on_blur = Some(Rc::new(move |_, _| blur_mode.set(false)));
    });
    let focus = editor.read(cx).focus_handle(cx);
    let focused = focus.is_focused(window);

    let style = EditorStyle {
        text_color: t.color.text_primary,
        placeholder: props.placeholder.clone().filter(|_| props.value.is_empty()),
        placeholder_color: t.color.text_disabled,
        selection_color: {
            let mut c = t.color.accent;
            c.a *= 0.3;
            c
        },
        caret_color: t.color.text_primary,
        multiline: true,
        min_lines: 1,
        max_lines: None,
        height: None,
        reduced_motion: prefers_reduced_motion(cx),
        spelling_color: t.color.error,
    };
    let mut field = text_editor(
        ElementId::Name(format!("{id}-code").into()),
        &editor,
        style,
        cx,
    )
    .role(Role::MultilineTextInput)
    .aria_label(props.label.clone())
    .aria_read_only(read_only)
    .w_auto()
    .min_w(relative(1.))
    .min_h(font_size * 3.)
    .py(t.spacing.s3)
    .px(t.spacing.s4)
    .text_size(font_size)
    .line_height(relative(leading))
    .font_stack(&t.font.family.code)
    .text_color(t.color.text_primary)
    .debug_selector({
        let s = format!("{id}-code");
        move || s
    });
    if !read_only {
        field = field.aria_description(props.escape_hint.clone());
    }

    let line_count = props.value.split('\n').count();
    let gutter = props.line_numbers.then(|| {
        div()
            .flex_none()
            .flex()
            .flex_col()
            .items_end()
            .pt(t.spacing.s3)
            .pb(t.spacing.s3)
            .pl(t.spacing.s4)
            .pr(t.spacing.s3)
            .text_color(t.color.text_disabled)
            .text_size(font_size)
            .border_r(t.border.width)
            .border_color(t.color.border)
            .font_stack(&t.font.family.code)
            .line_height(relative(leading))
            .debug_selector({
                let s = format!("{id}-gutter");
                move || s
            })
            .children((1..=line_count).map(|n| div().child(n.to_string())))
    });

    // Code, with its line numbers, keeps its own left-to-right direction inside a
    // right-to-left page (design:bidirectionality DR2): lines read and indent
    // correctly, and a long line scrolls from its start.
    let mut container = div()
        .id(ElementId::Name(format!("{id}-scroll").into()))
        .track_scroll(&scroll)
        .overflow_scroll()
        .flex()
        .flex_1()
        .min_w_0()
        .children(gutter)
        .child(div().relative().flex_1().min_w_0().child(field));
    if let Some(max) = props.max_height {
        container = container.max_h(max);
    }
    // The browser scrolls a focused editing host to its caret; the container follows
    // the engine's caret here.
    let caret = editor
        .read(cx)
        .offset_bounds(editor.read(cx).buffer().cursor());
    if focused && let (Some(caret), Some(view)) = (caret, Some(scroll.bounds())) {
        let mut offset = scroll.offset();
        let pad = t.spacing.s3.to_pixels(rem);
        if caret.top() < view.top() {
            offset.y += view.top() - caret.top() + pad;
        } else if caret.bottom() > view.bottom() {
            offset.y -= caret.bottom() - view.bottom() + pad;
        }
        if caret.left() < view.left() {
            offset.x += view.left() - caret.left();
        } else if caret.right() > view.right() {
            offset.x -= caret.right() - view.right();
        }
        let max = scroll.max_offset();
        offset.x = offset.x.clamp(-max.x, px(0.));
        offset.y = offset.y.clamp(-max.y, px(0.));
        if offset != scroll.offset() {
            scroll.set_offset(offset);
        }
    }

    let mut root = div()
        .id(id.clone())
        .relative()
        .flex()
        .overflow_hidden()
        .rounded(t.radius.element)
        .bg(t.color.background_muted)
        .border(t.border.width)
        .border_color(if focused {
            t.color.accent
        } else {
            t.color.border
        })
        .debug_selector({
            let s = id.to_string();
            move || s
        });
    if focused {
        // Upstream `rootFocused`: an accent border and a 1px accent ring.
        root = root.shadow(vec![gpui::BoxShadow {
            color: t.color.accent,
            offset: gpui::point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(1.),
            inset: false,
        }]);
    }
    let language = language_key(&props.language).to_string();
    theme.refine(
        root.style(),
        "codeeditor",
        &[("size", props.size.as_str()), ("language", &language)],
        None,
    );
    root.style().refine(&props.style);
    root.child(container).into_any_element()
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `CodeEditor`'s Rust documentation.
pub const CODE_EDITOR_DOC: ComponentDoc = ComponentDoc {
    name: "CodeEditor",
    rust_type: "astryx::lab::code_editor::CodeEditor",
    props: &[
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Argument("on_change")),
        p("label", RustApi::Argument("label")),
        p("language", RustApi::Method("language")),
        p("hasLineNumbers", RustApi::Method("line_numbers")),
        p("isReadOnly", RustApi::Method("read_only")),
        p("placeholder", RustApi::Method("placeholder")),
        p("maxHeight", RustApi::Method("max_height")),
        p("size", RustApi::Method("size")),
        p("tokenizer", RustApi::Method("tokenizer")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
