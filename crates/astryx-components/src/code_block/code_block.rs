//! CodeBlock, upstream `CodeBlock/CodeBlock.tsx`: read-only code with syntax paint (FR1, FR12,
//! FR13), an optional header (title and language label, FR3), line numbers in file
//! coordinates and highlighted lines (FR4, FR14), a unified-diff layer (FR9), copy (FR2,
//! FR15), caller controls (FR10), the corner cluster of a headerless block (FR11), and a
//! collapsible body (FR5). The viewport scrolls on both axes and takes a tab stop only
//! while it overflows (FR8).

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::current as i18n;
use astryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollRole, ScrollStickyContainment,
    UseScrollableAreaOptions, use_scrollable_area,
};
use astryx_primitives::{ElementSize, Politeness, announce, control_focus, is_focus_visible};
use astryx_syntax::{
    CodeLanguageInput, DiffLineKind, FlatToken, SYNC_TOKENIZE_THRESHOLD, SyntaxThemeDefinition,
    TokenLine, create_code_language, language_key, language_label, split_diff_lines, tokenize,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ClipboardItem, ElementId, FontWeight, IntoElement, KeyDownEvent, Length,
    ParentElement, Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, canvas,
    div, prelude::*, px,
};
use web_time::Instant;

use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconColor, IconSize, IconSource, component_icon};
use crate::scrollable_area::fade::{FadeScrollbar, scroll_edge_fades};
use crate::syntax_theme::{highlighted_line, syntax_colors, syntax_theme};

/// The copy control's icon slot (FR15); state `copied` or `idle`.
pub const CODE_BLOCK_COPY: &str = "code-block-copy";

/// How long the copied feedback shows.
const COPY_FEEDBACK: Duration = Duration::from_millis(2000);

type Action = Rc<dyn Fn(&mut Window, &mut App)>;
type CopyErrorFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// Upstream `size`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeBlockSize {
    Sm,
    #[default]
    Md,
}

/// Upstream `container`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeBlockContainer {
    /// Border, radius and the muted syntax background: a standalone block.
    #[default]
    Card,
    /// No border, radius or background: inside a surface that has them.
    Section,
}

/// Upstream `highlightMode`. GPUI paints every token as a span of styled text, so the
/// modes look alike; the prop is kept for parity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HighlightMode {
    #[default]
    Auto,
    Ranges,
    Spans,
}

/// The block's state between frames.
struct BlockState {
    collapsed: bool,
    copied_at: Option<Instant>,
    failed: bool,
    /// The tokens of the last source, by its key, once tokenized.
    tokens: Option<(u64, Rc<Vec<TokenLine>>)>,
    pending: Option<u64>,
    /// The space the block may take, for its fit-content width.
    available: Option<Pixels>,
}

/// Upstream `CodeBlock`.
#[derive(IntoElement)]
pub struct CodeBlock {
    id: ElementId,
    code: SharedString,
    language: CodeLanguageInput,
    title: Option<SharedString>,
    language_label: bool,
    line_numbers: bool,
    start_line_number: i64,
    highlight_lines: Vec<i64>,
    diff: bool,
    copy_button: bool,
    on_copy: Option<Action>,
    on_copy_error: Option<CopyErrorFn>,
    end_content: Option<AnyElement>,
    wrapped: bool,
    max_height: Option<Length>,
    collapsible: bool,
    collapsible_threshold: usize,
    size: CodeBlockSize,
    width: Option<Length>,
    container: CodeBlockContainer,
    tokenizer: Option<astryx_syntax::TokenizeFn>,
    highlight_mode: HighlightMode,
    syntax_theme: Option<SyntaxThemeDefinition>,
    style: StyleRefinement,
}

impl CodeBlock {
    pub fn new(id: impl Into<ElementId>, code: impl Into<SharedString>) -> Self {
        CodeBlock {
            id: id.into(),
            code: code.into(),
            language: CodeLanguageInput::default(),
            title: None,
            language_label: true,
            line_numbers: false,
            start_line_number: 1,
            highlight_lines: Vec::new(),
            diff: false,
            copy_button: true,
            on_copy: None,
            on_copy_error: None,
            end_content: None,
            wrapped: false,
            max_height: None,
            collapsible: false,
            collapsible_threshold: 10,
            size: CodeBlockSize::Md,
            width: None,
            container: CodeBlockContainer::Card,
            tokenizer: None,
            highlight_mode: HighlightMode::Auto,
            syntax_theme: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `language`: a built-in identifier, a language module, or a custom
    /// language. Default `plaintext`.
    pub fn language(mut self, language: impl Into<CodeLanguageInput>) -> Self {
        self.language = language.into();
        self
    }

    /// Upstream `title`: a file name or label in the header.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Upstream `hasLanguageLabel`. Default true; plaintext shows none.
    pub fn language_label(mut self, show: bool) -> Self {
        self.language_label = show;
        self
    }

    /// Upstream `hasLineNumbers`.
    pub fn line_numbers(mut self, show: bool) -> Self {
        self.line_numbers = show;
        self
    }

    /// Upstream `startLineNumber`: the first line's number in the file (at least 1).
    pub fn start_line_number(mut self, line: i64) -> Self {
        self.start_line_number = line;
        self
    }

    /// Upstream `highlightLines`: file line numbers to highlight.
    pub fn highlight_lines(mut self, lines: impl IntoIterator<Item = i64>) -> Self {
        self.highlight_lines = lines.into_iter().collect();
        self
    }

    /// Upstream `isDiff`: the code is a unified diff over `language`.
    pub fn diff(mut self, diff: bool) -> Self {
        self.diff = diff;
        self
    }

    /// Upstream `hasCopyButton`. Default true.
    pub fn copy_button(mut self, show: bool) -> Self {
        self.copy_button = show;
        self
    }

    /// Upstream `onCopy`.
    pub fn on_copy(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    /// Upstream `onCopyError`: once with the failure, when the write fails.
    pub fn on_copy_error(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_copy_error = Some(Rc::new(f));
        self
    }

    /// Upstream `endContent`: your controls, before Copy.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `isWrapped`: wrap long lines instead of scrolling sideways.
    pub fn wrapped(mut self, wrapped: bool) -> Self {
        self.wrapped = wrapped;
        self
    }

    /// Upstream `maxHeight`: the height past which the code scrolls (0 is a bound).
    pub fn max_height(mut self, height: impl Into<Length>) -> Self {
        self.max_height = Some(height.into());
        self
    }

    /// Upstream `isCollapsible`.
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible;
        self
    }

    /// Upstream `collapsibleThreshold`. Default 10 lines.
    pub fn collapsible_threshold(mut self, lines: usize) -> Self {
        self.collapsible_threshold = lines;
        self
    }

    /// Upstream `size`.
    pub fn size(mut self, size: CodeBlockSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `width`: else fit-content (at least 400px or the space there is).
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Upstream `container`.
    pub fn container(mut self, container: CodeBlockContainer) -> Self {
        self.container = container;
        self
    }

    /// Upstream `tokenizer` (deprecated): paints over the language's grammar.
    #[deprecated(note = "pass a create_code_language(..) as the language")]
    pub fn tokenizer(
        mut self,
        f: impl Fn(&str, &str) -> Vec<FlatToken> + Send + Sync + 'static,
    ) -> Self {
        self.tokenizer = Some(std::sync::Arc::new(f));
        self
    }

    /// Upstream `highlightMode`.
    pub fn highlight_mode(mut self, mode: HighlightMode) -> Self {
        self.highlight_mode = mode;
        self
    }

    /// Upstream `syntaxTheme`: shorthand for wrapping the block in a SyntaxTheme.
    pub fn syntax_theme(mut self, theme: SyntaxThemeDefinition) -> Self {
        self.syntax_theme = Some(theme);
        self
    }
}

impl Styled for CodeBlock {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A stable key for a source and its language.
fn source_key(code: &str, language: &CodeLanguageInput) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut h);
    language_key(language).hash(&mut h);
    language_label(language).hash(&mut h);
    if let CodeLanguageInput::Language(l) = language {
        l.is_custom().hash(&mut h);
    }
    h.finish()
}

impl RenderOnce for CodeBlock {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if let Some(theme) = self.syntax_theme.take() {
            return syntax_theme(theme, self).into_any_element();
        }
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let i18n = i18n(cx);
        let colors = syntax_colors(window, cx);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-code-block", self.id).into()),
            cx,
            |_, _| BlockState {
                collapsed: false,
                copied_at: None,
                failed: false,
                tokens: None,
                pending: None,
                available: None,
            },
        );

        // FR1: lines as written, less one trailing newline.
        let mut lines: Vec<&str> = self.code.split('\n').collect();
        if lines.len() > 1 && lines.last() == Some(&"") {
            lines.pop();
        }
        let diff = self.diff.then(|| split_diff_lines(&lines));
        // The deprecated tokenizer paints over the language's grammar (FR13).
        let language = match &self.tokenizer {
            Some(f) => {
                let f = f.clone();
                let name = language_label(&self.language).to_string();
                CodeLanguageInput::Language(create_code_language(
                    name.clone(),
                    [name],
                    Vec::<&str>::new(),
                    Vec::<&str>::new(),
                    move |code, lang| f(code, lang),
                ))
            }
            None => self.language.clone(),
        };
        let source: String = match &diff {
            Some((_, _, content)) => content.join("\n"),
            None => self.code.to_string(),
        };
        let key = source_key(&source, &language);
        // Short code tokenizes at once; long code in the background, plain until then.
        let tokens: Rc<Vec<TokenLine>> = match state.read(cx).tokens.clone() {
            Some((k, tokens)) if k == key => tokens,
            stale => {
                if source.len() < SYNC_TOKENIZE_THRESHOLD {
                    let tokens = Rc::new(tokenize(&source, &language));
                    state.update(cx, |s, _| s.tokens = Some((key, tokens.clone())));
                    tokens
                } else {
                    if state.read(cx).pending != Some(key) {
                        state.update(cx, |s, _| s.pending = Some(key));
                        let task = cx.background_executor().spawn({
                            let (source, language) = (source.clone(), language.clone());
                            async move { tokenize(&source, &language) }
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
                    stale.map(|(_, t)| t).unwrap_or_default()
                }
            }
        };

        let start = self.start_line_number.max(1);
        let size_text = match self.size {
            CodeBlockSize::Sm => t.text.supporting,
            CodeBlockSize::Md => t.text.code,
        };
        let language_name = language_label(&language).to_string();
        let language_attr = language_key(&language).to_string();
        let label: Option<SharedString> = if !self.language_label {
            None
        } else if self.diff {
            Some(if language_name == "plaintext" {
                "diff".into()
            } else {
                i18n.format(
                    "@astryx.codeBlock.diffLabel",
                    &[("language", language_name.as_str().into())],
                )
            })
        } else if language_name != "plaintext" {
            Some(language_name.clone().into())
        } else {
            None
        };
        let show_header = self.title.is_some() || label.is_some();
        // FR5: only with a visible header and enough lines; losing the control expands.
        let can_collapse =
            show_header && self.collapsible && lines.len() >= self.collapsible_threshold;
        if !can_collapse && state.read(cx).collapsed {
            state.update(cx, |s, _| s.collapsed = false);
        }
        let collapsed = can_collapse && state.read(cx).collapsed;
        // A section has no background of its own (upstream sets the syntax background clear).
        let surface_bg = match self.container {
            CodeBlockContainer::Card => colors.background,
            CodeBlockContainer::Section => gpui::transparent_black(),
        };
        let size_name = match self.size {
            CodeBlockSize::Sm => "sm",
            CodeBlockSize::Md => "md",
        };
        let container_name = match self.container {
            CodeBlockContainer::Card => "card",
            CodeBlockContainer::Section => "section",
        };

        // FR2, FR15: copy, with its feedback.
        let now = cx.background_executor().now();
        let copied = state
            .read(cx)
            .copied_at
            .is_some_and(|at| now.saturating_duration_since(at) < COPY_FEEDBACK);
        let failed = copied && state.read(cx).failed;
        let has_end = self.end_content.is_some();
        let has_controls = self.copy_button || has_end;
        let mut copy_button = self.copy_button.then(|| {
            let fallback = if failed {
                "close"
            } else if copied {
                "check"
            } else {
                "copy"
            };
            let icon = component_icon(
                CODE_BLOCK_COPY,
                if copied && !failed { "copied" } else { "idle" },
                IconSource::from(fallback),
                cx,
            );
            let glyph = match icon {
                Some(source) => Icon::new(source)
                    .size(IconSize::Sm)
                    .color(IconColor::Inherit)
                    .into_any_element(),
                // The slot hides the glyph only; the control keeps its size and name.
                None => div().size(px(16.)).into_any_element(),
            };
            let (tooltip, label) = if failed {
                let f = i18n.t("@astryx.codeBlock.copyFailed");
                (f.clone(), f)
            } else if copied {
                (
                    i18n.t("@astryx.codeBlock.copyCode"),
                    i18n.t("@astryx.codeBlock.copied"),
                )
            } else {
                let c = i18n.t("@astryx.codeBlock.copyCode");
                (c.clone(), c)
            };
            let (code, on_copy, state) = (self.code.clone(), self.on_copy.clone(), state.clone());
            let announcement = i18n.t("@astryx.codeBlock.copied");
            let mut button = IconButton::new(
                ElementId::Name(format!("{}-copy", self.id).into()),
                label,
                glyph,
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .tooltip(tooltip)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                // The platform write reports no failure, so a copy here succeeds.
                cx.write_to_clipboard(ClipboardItem::new_string(code.to_string()));
                let now = cx.background_executor().now();
                state.update(cx, |s, _| {
                    s.copied_at = Some(now);
                    s.failed = false;
                });
                announce(announcement.clone(), Politeness::Polite, window, cx);
                if let Some(f) = &on_copy {
                    f(window, cx);
                }
                let timer = cx.background_executor().timer(COPY_FEEDBACK);
                window
                    .spawn(cx, async move |cx| {
                        timer.await;
                        cx.update(|window, _| window.refresh()).ok();
                    })
                    .detach();
                window.refresh();
            });
            button.style().text.color = Some(colors.comment);
            let mut wrap = div()
                .id(ElementId::Name(format!("{}-copy-wrap", self.id).into()))
                .debug_selector({
                    let s = format!("{}-copy", self.id);
                    move || s
                })
                .text_color(colors.comment)
                .child(button);
            theme.refine(wrap.style(), "code-block-copy-button", &[], None);
            wrap
        });
        let _ = &self.on_copy_error;

        // FR3: the header, with the disclosure control when it collapses (FR5).
        let header = show_header.then(|| {
            let header_focus = control_focus(
                &ElementId::Name(format!("{}-header", self.id).into()),
                window,
                cx,
            );
            let chevron = can_collapse.then(|| {
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(14.))
                    .mr(t.spacing.s1)
                    .text_color(colors.comment)
                    .child(
                        Icon::new("chevronRight")
                            .size(IconSize::Xsm)
                            .color(IconColor::Inherit)
                            .rotation(if collapsed {
                                0.
                            } else {
                                std::f32::consts::FRAC_PI_2
                            }),
                    )
            });
            let mut title_el = div()
                .flex()
                .items_center()
                .min_w_0()
                .font_stack(&t.font.family.code)
                .text_style(&t.text.supporting)
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.comment)
                .children(chevron)
                .child(format!(
                    "{}{}{}",
                    self.title.clone().unwrap_or_default(),
                    if self.title.is_some() && label.is_some() {
                        " — "
                    } else {
                        ""
                    },
                    label.clone().unwrap_or_default()
                ));
            theme.refine(
                title_el.style(),
                "code-block-title",
                &[("size", size_name), ("language", &language_attr)],
                None,
            );
            let mut button = div()
                .id(ElementId::Name(format!("{}-header", self.id).into()))
                .debug_selector({
                    let s = format!("{}-header", self.id);
                    move || s
                })
                .relative()
                .flex()
                .items_center()
                .flex_1()
                .min_w_0()
                .child(title_el);
            if can_collapse {
                let (click_state, key_state) = (state.clone(), state.clone());
                let empty =
                    self.title.as_ref().is_none_or(|t| t.trim().is_empty()) && label.is_none();
                button = button
                    .track_focus(&header_focus)
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_expanded(!collapsed)
                    .when(empty, |el| el.aria_label(i18n.t("@astryx.codeBlock.code")))
                    .on_click(move |_, window, cx| {
                        click_state.update(cx, |s, _| s.collapsed = !s.collapsed);
                        window.refresh();
                    })
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            cx.stop_propagation();
                            key_state.update(cx, |s, _| s.collapsed = !s.collapsed);
                            window.refresh();
                        }
                    })
                    .children(is_focus_visible(&header_focus, window).then(|| {
                        crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)
                    }));
            }
            let mut row = div()
                .flex()
                .flex_none()
                .items_center()
                .justify_between()
                .px(t.spacing.s4)
                .py(t.spacing.s2)
                .bg(surface_bg)
                .when(self.line_numbers, |el| {
                    el.border_b_1().border_color(t.color.border)
                })
                .child(button);
            theme.refine(
                row.style(),
                "code-block-header",
                &[
                    ("size", size_name),
                    ("language", &language_attr),
                    ("container", container_name),
                ],
                None,
            );
            row
        });

        // The code: one row per line (FR4, FR9, FR14).
        let digits = (start + lines.len() as i64 - 1).max(1).to_string().len();
        let ch = {
            let mut probe = div().font_stack(&t.font.family.code).text_style(&size_text);
            let probe = &mut probe;
            let mut style = window.text_style();
            style.refine(&probe.style().text);
            let font_size = style.font_size.to_pixels(rem);
            let text_system = window.text_system();
            let font = text_system.resolve_font(&style.font());
            text_system
                .advance(font, font_size, '0')
                .map(|s| s.width)
                .unwrap_or(font_size * 0.6)
        };
        let gutter = ch * digits as f32;
        let gap = t.spacing.s3.to_pixels(rem) + px(1.) + t.spacing.s4.to_pixels(rem);
        let pad_x = t.spacing.s4.to_pixels(rem);
        let rows = lines.iter().enumerate().map(|(i, line)| {
            let number = start + i as i64;
            let kind = diff.as_ref().map(|(kinds, _, _)| kinds[i]);
            let highlighted = self.highlight_lines.contains(&number);
            let line_tokens = tokens.get(i).cloned().unwrap_or_default();
            let marker_color = match kind {
                Some(DiffLineKind::Added) => Some(colors.added_marker),
                Some(DiffLineKind::Removed) => Some(colors.removed_marker),
                _ => None,
            };
            let content: AnyElement = match (&diff, kind) {
                (Some(_), Some(DiffLineKind::Header)) => div()
                    .child(if line.is_empty() {
                        "\u{200b}".to_string()
                    } else {
                        line.to_string()
                    })
                    .into_any_element(),
                (Some((_, _, content)), _) => {
                    let text = content[i].clone();
                    highlighted_line(
                        if text.is_empty() {
                            "\u{200b}".to_string()
                        } else {
                            text
                        },
                        &line_tokens,
                        &colors,
                    )
                    .into_any_element()
                }
                _ => highlighted_line(
                    if line.is_empty() {
                        "\u{200b}".to_string()
                    } else {
                        line.to_string()
                    },
                    &line_tokens,
                    &colors,
                )
                .into_any_element(),
            };
            let content = div()
                .min_w_0()
                .when(!self.wrapped, |d| d.whitespace_nowrap())
                .child(content);
            let selector = format!("{}-line-{number}", self.id);
            let mut row = div()
                .debug_selector(move || selector)
                .flex()
                .items_start()
                .mx(-pad_x)
                .px(pad_x);
            // A highlight replaces a diff row's paint.
            row = if highlighted {
                row.bg(t.color.accent_muted)
            } else {
                match kind {
                    Some(DiffLineKind::Added) => row.bg(colors.added),
                    Some(DiffLineKind::Removed) => row.bg(colors.removed),
                    Some(DiffLineKind::Header) => {
                        row.bg(colors.diff_header).text_color(colors.punctuation)
                    }
                    _ => row,
                }
            };
            if self.line_numbers {
                row = row.child(
                    div()
                        .flex_none()
                        .w(gutter)
                        .flex()
                        .justify_end()
                        .text_color(marker_color.unwrap_or(colors.punctuation))
                        .child(number.to_string()),
                );
            }
            if let Some((_, glyphs, _)) = &diff {
                let glyph = if glyphs[i].is_empty() {
                    " ".to_string()
                } else {
                    glyphs[i].clone()
                };
                row = row.child(
                    div()
                        .flex_none()
                        .w(ch)
                        .when(self.line_numbers, |d| d.ml(gap))
                        .mr(t.spacing.s2)
                        .whitespace_nowrap()
                        .text_color(marker_color.unwrap_or(t.color.text_secondary))
                        .child(glyph),
                );
                row.child(content.flex_1())
            } else if self.line_numbers {
                row.child(content.ml(gap).flex_1())
            } else {
                row.child(content.flex_1())
            }
        });
        let code = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .py(t.spacing.s3)
            .px(t.spacing.s4)
            .font_stack(&t.font.family.code)
            .text_style(&size_text)
            .text_color(colors.variable)
            .min_w_0()
            .when(self.line_numbers, |el| {
                // The gutter's divider.
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(pad_x + gutter + t.spacing.s3.to_pixels(rem))
                        .w(px(1.))
                        .bg(t.color.border),
                )
            })
            .children(rows);

        // FR8: the viewport scrolls on both axes; a tab stop only while it overflows.
        // Wrapped code fits the width, so only the block axis scrolls.
        let mut options = UseScrollableAreaOptions::new(
            if self.wrapped {
                ScrollAxis::Block
            } else {
                ScrollAxis::Both
            },
            ScrollKeyboardAccess::Viewport {
                label: label
                    .clone()
                    .unwrap_or_else(|| i18n.t("@astryx.codeBlock.code")),
                role: ScrollRole::Group,
            },
        );
        options.sticky_containment = ScrollStickyContainment::Always;
        let area = use_scrollable_area(
            &ElementId::Name(format!("{}-viewport", self.id).into()),
            options,
            window,
            cx,
        );
        let fades = scroll_edge_fades(
            &format!("{}-fade", self.id),
            area.state.inline,
            FadeScrollbar::Native,
            astryx_primitives::coarse_primary_pointer(window, cx),
            area.is_focus_visible(window),
            (false, false),
            t.spacing.s4.to_pixels(rem),
            astryx_i18n::use_direction(cx),
            &t,
        );
        let content = area.content(
            div()
                .flex()
                .when(self.wrapped, |d| d.w_full())
                .when(show_header && !self.line_numbers, |d| {
                    d.mt(-t.spacing.s2.to_pixels(rem))
                })
                .child(code),
        );
        let mut viewport = area
            .viewport(
                div()
                    .relative()
                    .min_h_0()
                    .when(self.wrapped, |d| d.w_full()),
            )
            .debug_selector({
                let s = format!("{}-viewport", self.id);
                move || s
            })
            .child(content);
        if let Some(max) = self.max_height {
            viewport = viewport.max_h(max);
        }
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .flex_1()
            .child(viewport)
            .children(fades);
        // FR5: collapsed, the body stays mounted but out of reach.
        let region = format!("{}-region", self.id);
        let body = div()
            .debug_selector(move || region)
            .flex()
            .flex_col()
            .min_w_0()
            .when(!collapsed, |d| d.flex_1())
            .when(collapsed, |d| {
                d.flex_none().h(px(0.)).min_h(px(0.)).overflow_hidden()
            })
            .child(body)
            .into_any_element();

        let controls = || div().flex().items_center().gap(t.spacing.s1).flex_none();
        let headerless_cluster = has_controls && !show_header;
        let mut root = div()
            .id(self.id.clone())
            .debug_selector({
                let s = format!("{}", self.id);
                move || s
            })
            .relative()
            .flex()
            .overflow_hidden()
            .bg(match self.container {
                CodeBlockContainer::Card => colors.background,
                CodeBlockContainer::Section => gpui::transparent_black(),
            })
            .when(self.container == CodeBlockContainer::Card, |el| {
                el.rounded(t.radius.element)
                    .border_1()
                    .border_color(t.color.border)
            })
            .role(Role::Group);
        root = if headerless_cluster {
            root.flex_row()
        } else {
            root.flex_col()
        };
        root = match self.width {
            Some(width) => root.w(width),
            None => {
                let available = state.read(cx).available.unwrap_or(px(400.));
                root.max_w_full().min_w(px(400.).min(available))
            }
        };
        theme.refine(
            root.style(),
            "code-block",
            &[
                ("size", size_name),
                ("language", &language_attr),
                ("container", container_name),
            ],
            None,
        );
        root.style().refine(&self.style);
        let mut header = header;
        let mut end = self.end_content;
        if let Some(h) = header.take() {
            let mut controls_el = controls();
            if let Some(e) = end.take() {
                controls_el = controls_el.child(
                    div()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(e),
                );
            }
            let h = h.when(has_controls, |h| {
                h.child(controls_el.children(copy_button.take()))
            });
            root = root.child(h);
        }
        root = root.child(body);
        if headerless_cluster {
            // FR11: the corner cluster beside the viewport, centred on the first line.
            let line_height = size_text.size.to_pixels(rem) * size_text.leading;
            root = root.child(
                controls()
                    .self_start()
                    .mx(t.spacing.s2)
                    .h(t.spacing.s3.to_pixels(rem) * 2. + line_height)
                    .children(end)
                    .children(copy_button),
            );
        }
        // The space the block may take, for its fit-content width.
        let probe_state = state.clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let width = bounds.size.width;
                        if probe_state.read(cx).available != Some(width) {
                            probe_state.update(cx, |s, _| s.available = Some(width));
                        }
                    },
                    |_, _, _, _| {},
                )
                .w_full()
                .h(px(0.)),
            )
            .child(root)
            .into_any_element()
    }
}
