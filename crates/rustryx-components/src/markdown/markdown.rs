//! Markdown, upstream `Markdown/Markdown.tsx`: a Markdown string rendered with Astryx's
//! own components. The parser is `rustryx_markdown`, a port of upstream's.
//!
//! - Blocks (FR1, FR2): headings, paragraphs, lists (task lists through CheckboxList),
//!   fenced code through CodeBlock, blockquotes, tables through Table, rules and images,
//!   each with its theme target (`markdown-heading`, `markdown-paragraph`, ..., and the
//!   released `markdown-codeblock`, FR4) and `density` (the heading also its `level`, FR5).
//! - Block spacing follows the density. Upstream's blocks are CSS blocks whose vertical
//!   margins collapse; here each gap is the larger of the two margins that meet, the
//!   first block has no top margin and the last no bottom one.
//! - Prose (headings, paragraphs, lists, quotes) is capped at `content_width`; code blocks
//!   are at least that wide where the space allows.
//! - Each block takes the direction of its own text's first strong character.
//! - A supplied renderer in `components` replaces its part and its target (FR3).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use web_time::Instant;

use gpui::{
    AbsoluteLength, AnyElement, App, ClickEvent, DefiniteLength, ElementId, FontStyle, ImageSource,
    IntoElement, Length, ParentElement, Pixels, Refineable as _, RenderOnce, Role, SharedString,
    StrikethroughStyle, StyleRefinement, Styled, TextAlign, TextStyleRefinement, Window, div, img,
    prelude::*, px, relative,
};
use rustryx_i18n::{Direction, first_strong_direction};
use rustryx_markdown::ast::{self, Block, Phrasing};
use rustryx_markdown::plugins::diagnostics::{DiagnosticCallback, PluginDiagnostic};
use rustryx_markdown::plugins::{
    MarkdownPlugin, PreparedMarkdownPlugins, prepare_markdown_plugins,
};
use rustryx_markdown::{
    IncrementalState, MarkdownHeadingProjection, ParseOptions, create_incremental_state,
    parse_inline_ast, parse_markdown_ast, parse_markdown_ast_incremental,
    project_markdown_headings, trim_streaming_artifacts,
};
use rustryx_primitives::ElementSize;
use rustryx_syntax::{CodeLanguage, CodeLanguageInput, listed_language};
use rustryx_theme::{ResolvedTheme, ThemeStyled as _, current};

use crate::button::{Button, ButtonVariant};
use crate::choice::{CheckboxList, CheckboxListItem};
use crate::code_block::CodeBlock;
use crate::container_reveal::{ContentRevealOptions, use_container_reveal};
use crate::content::{Blockquote, Citation, CitationSource, CitationVariant, Code};
use crate::icon::{Icon, IconSize, IconSource};
use crate::item::ItemDensity;
use crate::link::Link;
use crate::list::{List, ListEdge, ListItem, ListMarker, ListStyle, ListTask, list_marker_scope};
use crate::table::{
    Table, TableBody, TableCell, TableColumnAlign, TableDividers, TableHeader, TableHeaderCell,
    TableRow, TableTextOverflow,
};
use crate::text::{Heading, HeadingLevel, InlineSpan, TextDisplay, TextType};

use super::plugins::render_plugin_node;
use super::streaming::{
    UseStreamingTextOptions, compute_boundaries, compute_segments, use_streaming_text,
};

/// Upstream `density`: the space between blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkdownDensity {
    #[default]
    Default,
    Compact,
}

impl MarkdownDensity {
    pub fn as_str(self) -> &'static str {
        match self {
            MarkdownDensity::Default => "default",
            MarkdownDensity::Compact => "compact",
        }
    }
}

/// Upstream `contentAlign`: where capped blocks sit in a wider column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkdownContentAlign {
    #[default]
    Start,
    Center,
}

/// Upstream `autolink`: bare URLs and emails that become links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownAutolink {
    /// GitHub's autolink literals: `https://…`, `www.…` and `user@host`, plus
    /// CommonMark's `<URL>` and `<email>`.
    Gfm,
}

/// Upstream `MarkdownSourceLines`: the source lines a block came from, 1-based and
/// inclusive. Present only when `markdown_source_lines_plugin` is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkdownSourceLines {
    pub start: usize,
    pub end: usize,
}

/// What a supplied `code` renderer receives.
pub struct MarkdownCodeProps {
    pub code: SharedString,
    /// The fence's leading word (`diff` for `diff-tsx`), `plaintext` when it names none.
    pub language: SharedString,
    pub source_lines: Option<MarkdownSourceLines>,
}

/// What a supplied `heading` renderer receives.
pub struct MarkdownHeadingProps {
    /// 1 to 6, after `heading_level_start`.
    pub level: u8,
    /// The heading's anchor id: a root heading's slug, or with the heading-links plugin
    /// every heading's id; none otherwise.
    pub id: Option<SharedString>,
    pub children: Vec<AnyElement>,
    pub source_lines: Option<MarkdownSourceLines>,
}

/// What a supplied `image` renderer receives.
pub struct MarkdownImageProps {
    pub src: SharedString,
    pub alt: SharedString,
    /// Set for an image that is a block of its own.
    pub source_lines: Option<MarkdownSourceLines>,
}

/// What a supplied `paragraph` renderer receives.
pub struct MarkdownParagraphProps {
    pub children: Vec<AnyElement>,
    pub source_lines: Option<MarkdownSourceLines>,
}

/// What a supplied `blockquote` renderer receives.
pub struct MarkdownBlockquoteProps {
    /// The quote's rendered blocks.
    pub children: Vec<AnyElement>,
    pub source_lines: Option<MarkdownSourceLines>,
}

/// What a supplied `hr` renderer receives.
pub struct MarkdownHrProps {
    pub source_lines: Option<MarkdownSourceLines>,
}

/// Where math sits: in a line of text or as a block of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownMathDisplay {
    Inline,
    Block,
}

/// What a supplied `math` renderer receives (FR7): the source between the delimiters.
pub struct MarkdownMathProps {
    pub value: SharedString,
    pub display: MarkdownMathDisplay,
    /// Set for display math.
    pub source_lines: Option<MarkdownSourceLines>,
}

/// What a supplied `citation` renderer receives.
pub struct MarkdownCitationProps {
    pub source: CitationSource,
    /// The source's number, by first citation in the document.
    pub number: u32,
    pub variant: CitationVariant,
}

type EndIndexFn = dyn Fn(&str, &fancy_regex::Captures) -> Option<usize>;
type InlinePluginRenderFn = dyn Fn(&fancy_regex::Captures, &mut Window, &mut App) -> AnyElement;

/// Upstream `MarkdownInlinePlugin`: a pattern matched in prose text (never in code or
/// math) and the element that replaces each match.
#[derive(Clone)]
pub struct MarkdownInlinePlugin {
    pattern: Rc<fancy_regex::Regex>,
    end_index: Option<Rc<EndIndexFn>>,
    render: Rc<InlinePluginRenderFn>,
}

impl MarkdownInlinePlugin {
    /// Upstream `pattern` and `render`.
    pub fn new(
        pattern: fancy_regex::Regex,
        render: impl Fn(&fancy_regex::Captures, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        MarkdownInlinePlugin {
            pattern: Rc::new(pattern),
            end_index: None,
            render: Rc::new(render),
        }
    }

    /// Upstream `getEndIndex`: where a match ends (a byte index into the text), or
    /// `None` to skip it. By default, where the pattern's match ends.
    pub fn end_index(
        mut self,
        f: impl Fn(&str, &fancy_regex::Captures) -> Option<usize> + 'static,
    ) -> Self {
        self.end_index = Some(Rc::new(f));
        self
    }
}

/// What a supplied `link` renderer receives.
pub struct MarkdownLinkProps {
    pub href: SharedString,
    pub children: Vec<AnyElement>,
}

type Render<P> = Rc<dyn Fn(P, &mut Window, &mut App) -> AnyElement>;

/// Upstream `components`: renderers that replace Markdown's own parts (FR3).
#[derive(Clone, Default)]
pub struct MarkdownComponents {
    code: Option<Render<MarkdownCodeProps>>,
    inline_code: Option<Render<SharedString>>,
    link: Option<Render<MarkdownLinkProps>>,
    heading: Option<Render<MarkdownHeadingProps>>,
    paragraph: Option<Render<MarkdownParagraphProps>>,
    image: Option<Render<MarkdownImageProps>>,
    blockquote: Option<Render<MarkdownBlockquoteProps>>,
    hr: Option<Render<MarkdownHrProps>>,
    math: Option<Render<MarkdownMathProps>>,
    citation: Option<Render<MarkdownCitationProps>>,
}

macro_rules! renderer {
    ($(#[$doc:meta])* $name:ident: $props:ty) => {
        $(#[$doc])*
        pub fn $name(
            mut self,
            render: impl Fn($props, &mut Window, &mut App) -> AnyElement + 'static,
        ) -> Self {
            self.$name = Some(Rc::new(render));
            self
        }
    };
}

impl MarkdownComponents {
    pub fn new() -> Self {
        Self::default()
    }

    renderer!(
        /// A fenced code block, in place of CodeBlock and its target.
        code: MarkdownCodeProps
    );
    renderer!(
        /// Inline code, in place of Code (in table cells too).
        inline_code: SharedString
    );
    renderer!(
        /// A link with a safe destination.
        link: MarkdownLinkProps
    );
    renderer!(
        /// A heading, in place of the heading part and its target.
        heading: MarkdownHeadingProps
    );
    renderer!(
        /// A paragraph, given its inline content.
        paragraph: MarkdownParagraphProps
    );
    renderer!(
        /// An image with a safe source, inline or a block of its own.
        image: MarkdownImageProps
    );
    renderer!(
        /// A blockquote, given its rendered blocks.
        blockquote: MarkdownBlockquoteProps
    );
    renderer!(
        /// A thematic break.
        hr: MarkdownHrProps
    );
    renderer!(
        /// Math: providing it turns on `$…$` and `$$…$$` parsing (FR6, FR7).
        math: MarkdownMathProps
    );
    renderer!(
        /// A citation, in place of the Citation chip.
        citation: MarkdownCitationProps
    );
}

type LinkClickFn = Rc<dyn Fn(&str, &ClickEvent, &mut Window, &mut App) -> bool>;

/// Upstream `Markdown`.
///
/// ```ignore
/// Markdown::new("notes", "# Hello\n\nThis is **bold** and _italic_ text.\n\n- Item one\n- Item two")
/// ```
#[derive(IntoElement)]
pub struct Markdown {
    id: ElementId,
    source: SharedString,
    display: TextDisplay,
    density: MarkdownDensity,
    heading_level_start: u8,
    on_link_click: Option<LinkClickFn>,
    content_width: Length,
    content_align: MarkdownContentAlign,
    components: MarkdownComponents,
    autolink: Option<MarkdownAutolink>,
    sources: Option<Rc<HashMap<String, CitationSource>>>,
    citation_style: CitationVariant,
    inline_plugins: Vec<MarkdownInlinePlugin>,
    code_languages: Vec<CodeLanguage>,
    streaming: bool,
    plugins: Vec<MarkdownPlugin>,
    on_plugin_diagnostic: Option<DiagnosticCallback>,
    style: StyleRefinement,
}

impl Markdown {
    /// Upstream `children`: the Markdown source.
    pub fn new(id: impl Into<ElementId>, source: impl Into<SharedString>) -> Self {
        Markdown {
            id: id.into(),
            source: source.into(),
            display: TextDisplay::Block,
            density: MarkdownDensity::Default,
            heading_level_start: 1,
            on_link_click: None,
            content_width: px(680.).into(),
            content_align: MarkdownContentAlign::Start,
            components: MarkdownComponents::default(),
            autolink: None,
            sources: None,
            citation_style: CitationVariant::Label,
            inline_plugins: Vec::new(),
            code_languages: Vec::new(),
            streaming: false,
            plugins: Vec::new(),
            on_plugin_diagnostic: None,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `display`: `Block` (the default) renders blocks; `Inline` renders the
        /// source's inline content alone, flowing with the paragraph around it.
        display: TextDisplay,
        /// Upstream `density`.
        density: MarkdownDensity,
        /// Upstream `contentAlign`.
        content_align: MarkdownContentAlign,
        /// Upstream `components`.
        components: MarkdownComponents,
    }

    /// Upstream `headingLevelStart`: the level a `#` heading takes (1 to 6); deeper ones
    /// follow, capped at 6.
    pub fn heading_level_start(mut self, level: u8) -> Self {
        self.heading_level_start = level.clamp(1, 6);
        self
    }

    /// Upstream `contentWidth`: the measure prose is capped at, 680px by default.
    /// `Length::Auto` or zero drops the cap.
    pub fn content_width(mut self, width: impl Into<Length>) -> Self {
        self.content_width = width.into();
        self
    }

    /// Upstream `sources`: citation sources by id. A `[id]` or `【id】` naming one renders
    /// as a citation chip, numbered by first appearance.
    pub fn sources<K: Into<String>>(
        mut self,
        sources: impl IntoIterator<Item = (K, CitationSource)>,
    ) -> Self {
        let sources = sources.into_iter().map(|(k, v)| (k.into(), v)).collect();
        self.sources = Some(Rc::new(sources));
        self
    }

    /// Upstream `citationStyle`: a chip with the title (`Label`) or a numbered badge.
    pub fn citation_style(mut self, style: CitationVariant) -> Self {
        self.citation_style = style;
        self
    }

    /// Upstream `inlinePlugins`: patterns in prose text rendered as elements. The first
    /// plugin wins where two overlap.
    pub fn inline_plugins(
        mut self,
        plugins: impl IntoIterator<Item = MarkdownInlinePlugin>,
    ) -> Self {
        self.inline_plugins = plugins.into_iter().collect();
        self
    }

    /// Upstream `codeLanguages`: CodeBlock language values a fence may name (FR30); a
    /// listed module wins over a built-in with the same identifier, a later entry over an
    /// earlier one.
    pub fn code_languages(mut self, languages: impl IntoIterator<Item = CodeLanguage>) -> Self {
        self.code_languages = languages.into_iter().collect();
        self
    }

    /// Upstream `isStreaming`: the source is still arriving. It reveals steadily
    /// (`use_streaming_text`), and each newly shown span of text fades in.
    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// Upstream `plugins`: ordered syntax, document transforms and extension renderers,
    /// from `rustryx_markdown::plugins::create_markdown_plugin` and the first-party helpers
    /// (FR12-FR22). An empty list is no plugins. Keep the list stable between renders: a
    /// changed list parses again.
    pub fn plugins(mut self, plugins: impl IntoIterator<Item = MarkdownPlugin>) -> Self {
        self.plugins = plugins.into_iter().collect();
        self
    }

    /// Upstream `onPluginDiagnostic`: one source-free event per plugin failure or
    /// advisory (FR19).
    pub fn on_plugin_diagnostic(mut self, f: impl Fn(&PluginDiagnostic) + 'static) -> Self {
        self.on_plugin_diagnostic = Some(Rc::new(f));
        self
    }

    /// Upstream `autolink`.
    pub fn autolink(mut self, autolink: MarkdownAutolink) -> Self {
        self.autolink = Some(autolink);
        self
    }

    /// Upstream `onLinkClick`: called with a link's destination before it opens;
    /// returning `false` keeps it from opening.
    pub fn on_link_click(
        mut self,
        f: impl Fn(&str, &ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_link_click = Some(Rc::new(f));
        self
    }
}

impl Styled for Markdown {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The last parse, kept until the source, its options or its plugins change.
#[derive(Default)]
struct ParseCache {
    key: Option<u64>,
    plugins: Vec<MarkdownPlugin>,
    blocks: Rc<Vec<Block>>,
    inline: Rc<Vec<Phrasing>>,
    prepared: Option<Rc<PreparedMarkdownPlugins>>,
    projection: Rc<MarkdownHeadingProjection>,
    /// While streaming, the incremental parse's state and the parse identity it began
    /// under.
    incremental: Option<(String, IncrementalState)>,
}

/// The cap on prose and the floor of code blocks.
#[derive(Clone, Copy)]
enum Measure {
    None,
    Length(DefiniteLength),
}

impl Measure {
    fn of(width: Length) -> Self {
        match width {
            Length::Auto => Measure::None,
            Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(p)))
                if p <= px(0.) =>
            {
                Measure::None
            }
            Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Rems(r))) if r.0 <= 0. => {
                Measure::None
            }
            Length::Definite(DefiniteLength::Fraction(f)) if f <= 0. => Measure::None,
            Length::Definite(length) => Measure::Length(length),
            _ => Measure::None,
        }
    }
}

/// What every part of one render shares.
struct Cx<'a> {
    id: &'a ElementId,
    theme: &'a ResolvedTheme,
    density: MarkdownDensity,
    heading_level_start: u8,
    on_link_click: Option<LinkClickFn>,
    components: &'a MarkdownComponents,
    measure: Measure,
    align: MarkdownContentAlign,
    rem: Pixels,
    /// The width of the body font's "0", upstream's `ch`.
    ch: Pixels,
    /// The document's direction, and the direction of the container around a block.
    provider: Direction,
    around: Direction,
    /// Heading ids, labels and permalinks, by each heading's place in document order.
    projection: &'a MarkdownHeadingProjection,
    /// Headings rendered so far, the next one's index in the projection.
    headings_seen: Cell<usize>,
    plugins: Option<&'a PreparedMarkdownPlugins>,
    on_diagnostic: Option<&'a DiagnosticCallback>,
    /// Numbers the ids of stateful parts.
    next: RefCell<usize>,
    /// Citation numbers by first appearance, when there are sources.
    citations: Option<RefCell<Citations<'a>>>,
    inline_plugins: &'a [MarkdownInlinePlugin],
    code_languages: &'a [CodeLanguage],
    /// The streaming cursor: the text visited so far in this render, against the
    /// recent renders' boundaries.
    cursor: RefCell<Cursor>,
}

/// Upstream's `StreamingCursor`, with each fading span's current opacity.
struct Cursor {
    offset: usize,
    active: bool,
    boundaries: Vec<usize>,
    /// Opacity by the boundary a span fades in from.
    fades: HashMap<usize, f32>,
}

impl Cx<'_> {
    /// Moves the cursor past content that does not fade.
    fn advance(&self, len: usize) {
        self.cursor.borrow_mut().offset += len;
    }

    /// Upstream `wrapTextWithFade`: text, its newest parts fading in while streaming.
    fn fade_text(&self, content: &str) -> AnyElement {
        let mut cursor = self.cursor.borrow_mut();
        let start = cursor.offset;
        cursor.offset += content.len();
        if !cursor.active {
            return text(content.to_string());
        }
        match compute_segments(content, start, &cursor.boundaries) {
            None => text(content.to_string()),
            Some(segments) => span(
                TextStyleRefinement::default(),
                segments
                    .into_iter()
                    .map(|segment| match segment.fading_from {
                        None => text(segment.text),
                        Some(from) => {
                            let opacity = cursor.fades.get(&from).copied().unwrap_or(1.);
                            InlineSpan::new(TextStyleRefinement::default())
                                .opacity(opacity)
                                .child(text(segment.text))
                                .into_any_element()
                        }
                    })
                    .collect(),
            ),
        }
    }
}

/// Upstream `countInlineTextLength`: the text a render's cursor passes in phrasing.
fn count_inline_text_length(nodes: &[Phrasing]) -> usize {
    nodes
        .iter()
        .map(|node| match node {
            Phrasing::Text(n) | Phrasing::InlineCode(n) | Phrasing::InlineMath(n) => n.value.len(),
            Phrasing::Image(n) => n.alt.len(),
            Phrasing::Strong(p) | Phrasing::Emphasis(p) | Phrasing::Delete(p) => {
                count_inline_text_length(&p.children)
            }
            Phrasing::Link(l) => count_inline_text_length(&l.children),
            Phrasing::Break(_) | Phrasing::Citation(_) => 1,
            Phrasing::Extension(e) => e.source.as_deref().map_or(0, str::len),
        })
        .sum()
}

/// Upstream `countBlockTextLength`.
fn count_block_text_length(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|block| match block {
            Block::Heading(h) => count_inline_text_length(&h.children),
            Block::Paragraph(p) => count_inline_text_length(&p.children),
            Block::Code(c) => c.value.len(),
            Block::Math(m) => m.value.len(),
            Block::Blockquote(q) => count_block_text_length(&q.children),
            Block::List(l) => l
                .children
                .iter()
                .map(|i| count_block_text_length(&i.children))
                .sum(),
            Block::Table(t) => t
                .children
                .iter()
                .flat_map(|row| &row.children)
                .map(|cell| count_inline_text_length(&cell.children))
                .sum(),
            Block::Extension(e) => e.source.as_deref().map_or(0, str::len),
            Block::ThematicBreak(_) => 0,
            Block::Image(i) => i.alt.len(),
        })
        .sum()
}

/// Upstream's `CitationContext`.
struct Citations<'a> {
    sources: &'a HashMap<String, CitationSource>,
    numbers: HashMap<String, u32>,
    style: CitationVariant,
}

impl Citations<'_> {
    fn number(&mut self, source_id: &str) -> u32 {
        let next = self.numbers.len() as u32 + 1;
        *self.numbers.entry(source_id.to_string()).or_insert(next)
    }
}

impl Cx<'_> {
    fn part_id(&self, kind: &str) -> ElementId {
        let mut n = self.next.borrow_mut();
        *n += 1;
        ElementId::Name(format!("{}-{kind}-{}", self.id, *n).into())
    }
}

/// A block's own margins, upstream `getElementSpacing`: (block start, block end).
fn spacing(
    block: &Block,
    density: MarkdownDensity,
    theme: &ResolvedTheme,
    rem: Pixels,
) -> (Pixels, Pixels) {
    let s = &theme.tokens().spacing;
    let compact = density == MarkdownDensity::Compact;
    let pick = |default: (_, _), dense: (_, _)| if compact { dense } else { default };
    let (start, end) = match block {
        Block::Heading(h) if h.depth <= 3 => pick((s.s6, s.s3), (s.s4, s.s2)),
        Block::Heading(_) => pick((s.s4, s.s2), (s.s3, s.s1)),
        Block::Paragraph(_) | Block::Extension(_) | Block::List(_) => {
            pick((s.s3, s.s3), (s.s1, s.s1))
        }
        Block::Code(_) | Block::Math(_) | Block::Blockquote(_) | Block::Table(_) => {
            pick((s.s4, s.s4), (s.s2, s.s2))
        }
        Block::ThematicBreak(_) => pick((s.s6, s.s6), (s.s3, s.s3)),
        Block::Image(_) => pick((s.s3, s.s3), (s.s2, s.s2)),
    };
    (start.to_pixels(rem), end.to_pixels(rem))
}

/// Upstream `markdownBlockDirection` for phrasing: the direction of the first strong
/// character, skipping code, math, images, citations and breaks.
fn phrasing_direction(nodes: &[Phrasing]) -> Option<Direction> {
    nodes.iter().find_map(|node| match node {
        Phrasing::Text(t) => first_strong_direction(&t.value),
        Phrasing::Strong(p) | Phrasing::Emphasis(p) | Phrasing::Delete(p) => {
            phrasing_direction(&p.children)
        }
        Phrasing::Link(l) => phrasing_direction(&l.children),
        _ => None,
    })
}

fn blocks_direction(blocks: &[Block]) -> Option<Direction> {
    blocks.iter().find_map(block_own_direction)
}

/// Upstream `markdownBlockDirection`: tables, code, math, rules and images have none.
fn block_own_direction(block: &Block) -> Option<Direction> {
    match block {
        Block::Heading(h) => phrasing_direction(&h.children),
        Block::Paragraph(p) => phrasing_direction(&p.children),
        Block::Blockquote(q) => blocks_direction(&q.children),
        Block::List(l) => l
            .children
            .iter()
            .find_map(|i| blocks_direction(&i.children)),
        _ => None,
    }
}

/// Upstream `blockDirection`: a block's own direction, else the document's where the
/// block sits in a container that reads against it.
fn block_direction(own: Option<Direction>, cx: &Cx) -> Option<Direction> {
    own.or((cx.around != cx.provider).then_some(cx.provider))
}

/// Upstream `computeTableColumnMinWidths`: each column's floor in `ch`, the larger of half
/// its longest cell (so the cell wraps to about two lines) and its header label (up to
/// 20), between 4 and 24. Lengths are UTF-16 units, as upstream's.
pub(crate) fn table_column_floors(table: &ast::Table) -> Vec<usize> {
    let Some((header, rows)) = table.children.split_first() else {
        return Vec::new();
    };
    let len = |cell: &ast::TableCell| {
        ast::markdown_ast_text(&cell.children)
            .encode_utf16()
            .count()
    };
    header
        .children
        .iter()
        .enumerate()
        .map(|(column, cell)| {
            let header_len = len(cell);
            let longest = rows
                .iter()
                .filter_map(|row| row.children.get(column))
                .map(len)
                .fold(header_len, usize::max);
            longest.div_ceil(2).max(header_len.min(20)).clamp(4, 24)
        })
        .collect()
}

fn heading_level(depth: u8, start: u8) -> HeadingLevel {
    match (depth + start - 1).min(6) {
        1 => HeadingLevel::H1,
        2 => HeadingLevel::H2,
        3 => HeadingLevel::H3,
        4 => HeadingLevel::H4,
        5 => HeadingLevel::H5,
        _ => HeadingLevel::H6,
    }
}

/// A part's theme target, as upstream's `themeProps(target, {density, ...})`.
fn target(style: &mut StyleRefinement, cx: &Cx, name: &str, extra: &[(&str, &str)]) {
    let mut props = vec![("density", cx.density.as_str())];
    props.extend_from_slice(extra);
    cx.theme.refine(style, name, &props, None);
}

/// Caps prose at the measure, at the start or centred.
fn prose<E: Styled>(el: E, cx: &Cx) -> E {
    let el = el.w_full().min_w_0();
    let el = match cx.measure {
        Measure::None => el,
        Measure::Length(width) => el.max_w(width),
    };
    match (cx.align, cx.provider) {
        (MarkdownContentAlign::Center, _) => el.mx_auto(),
        (MarkdownContentAlign::Start, Direction::Rtl) => el.ml_auto(),
        (MarkdownContentAlign::Start, Direction::Ltr) => el,
    }
}

/// The text style of a block reading in `direction`: its lines start on that side.
fn aligned(direction: Option<Direction>) -> TextStyleRefinement {
    TextStyleRefinement {
        text_align: direction.map(|d| match d {
            Direction::Ltr => TextAlign::Left,
            Direction::Rtl => TextAlign::Right,
        }),
        ..TextStyleRefinement::default()
    }
}

/// A block in its own direction: the components inside it mirror with it.
fn in_direction(direction: Option<Direction>, el: AnyElement, app: &App) -> AnyElement {
    let current = rustryx_i18n::current(app);
    match direction {
        Some(d) if d != current.direction() => {
            rustryx_i18n::provider(current.dir(d), el).into_any_element()
        }
        _ => el,
    }
}

fn span(refinement: TextStyleRefinement, children: Vec<AnyElement>) -> AnyElement {
    InlineSpan::new(refinement)
        .children(children)
        .into_any_element()
}

fn text(value: impl Into<SharedString>) -> AnyElement {
    value.into().into_any_element()
}

fn render_inline(
    nodes: &[Phrasing],
    cx: &Cx,
    window: &mut Window,
    app: &mut App,
) -> Vec<AnyElement> {
    nodes
        .iter()
        .map(|node| render_phrasing(node, cx, window, app))
        .collect()
}

/// Upstream `renderInline`.
fn render_phrasing(node: &Phrasing, cx: &Cx, window: &mut Window, app: &mut App) -> AnyElement {
    let t = cx.theme.tokens();
    match node {
        Phrasing::Text(literal) => {
            if cx.inline_plugins.is_empty() {
                return cx.fade_text(&literal.value);
            }
            let segments = apply_inline_plugins(&literal.value, cx, window, app);
            span(TextStyleRefinement::default(), segments)
        }
        Phrasing::Strong(p) => span(
            TextStyleRefinement {
                font_weight: Some(t.font.weight.semibold),
                ..Default::default()
            },
            render_inline(&p.children, cx, window, app),
        ),
        Phrasing::Emphasis(p) => span(
            TextStyleRefinement {
                font_style: Some(FontStyle::Italic),
                ..Default::default()
            },
            render_inline(&p.children, cx, window, app),
        ),
        Phrasing::Delete(p) => span(
            TextStyleRefinement {
                color: Some(t.color.text_secondary),
                strikethrough: Some(StrikethroughStyle {
                    thickness: px(1.),
                    color: None,
                }),
                ..Default::default()
            },
            render_inline(&p.children, cx, window, app),
        ),
        Phrasing::InlineCode(code) => {
            cx.advance(code.value.len());
            let value = SharedString::from(code.value.clone());
            match &cx.components.inline_code {
                Some(render) => render(value, window, app),
                None => Code::new(value).into_any_element(),
            }
        }
        // Math needs a math renderer; without one it reads as written (FR6).
        Phrasing::InlineMath(math) => match &cx.components.math {
            Some(render) => {
                cx.advance(math.value.len());
                render(
                    MarkdownMathProps {
                        value: math.value.clone().into(),
                        display: MarkdownMathDisplay::Inline,
                        source_lines: None,
                    },
                    window,
                    app,
                )
            }
            None => cx.fade_text(&format!("${}$", math.value)),
        },
        Phrasing::Link(link) => {
            let children = render_inline(&link.children, cx, window, app);
            // An unsafe destination leaves the link's text (FR14).
            if link.url.is_empty() {
                return span(TextStyleRefinement::default(), children);
            }
            let href = SharedString::from(link.url.clone());
            if let Some(render) = &cx.components.link {
                return render(MarkdownLinkProps { href, children }, window, app);
            }
            let external = link.url.starts_with("https://") || link.url.starts_with("http://");
            let mut el = Link::new(cx.part_id("link"), "")
                .href(href.clone())
                .text_type(TextType::Inherit)
                .underline(true)
                .children(children);
            if external {
                el = el.target("_blank");
            }
            if let Some(f) = cx.on_link_click.clone() {
                el = el.on_click(move |event, window, cx| {
                    if !f(&href, event, window, cx) {
                        window.prevent_default();
                    }
                });
            }
            el.into_any_element()
        }
        Phrasing::Image(image) => render_image(image, cx, window, app),
        Phrasing::Break(_) => {
            cx.advance(1);
            text("\n")
        }
        Phrasing::Citation(c) => {
            cx.advance(1);
            // Without sources a citation reads as its marker.
            let Some(citations) = &cx.citations else {
                return text(format!("[{}]", c.source_id));
            };
            let (number, source, variant) = {
                let mut citations = citations.borrow_mut();
                let number = citations.number(&c.source_id);
                let source = citations
                    .sources
                    .get(&c.source_id)
                    .cloned()
                    .unwrap_or_else(|| CitationSource {
                        title: Some(c.source_id.clone().into()),
                        ..CitationSource::default()
                    });
                (number, source, citations.style)
            };
            match &cx.components.citation {
                Some(render) => render(
                    MarkdownCitationProps {
                        source,
                        number,
                        variant,
                    },
                    window,
                    app,
                ),
                None => Citation::new(cx.part_id("citation"), source, number)
                    .variant(variant)
                    .into_any_element(),
            }
        }
        Phrasing::Extension(e) => {
            let fallback = cx.fade_text(&extension_fallback(e, cx));
            render_plugin_node(cx.plugins, e, || fallback, cx.on_diagnostic, window, app)
        }
    }
}

/// An image, or its alternative text in brackets when its source is unsafe.
fn render_image(image: &ast::Image, cx: &Cx, window: &mut Window, app: &mut App) -> AnyElement {
    if image.url.is_empty() {
        return text(format!("[{}]", image.alt));
    }
    let src = SharedString::from(image.url.clone());
    let alt = SharedString::from(image.alt.clone());
    if let Some(render) = &cx.components.image {
        let props = MarkdownImageProps {
            src,
            alt,
            source_lines: None,
        };
        return render(props, window, app);
    }
    let t = cx.theme.tokens();
    div()
        .id(cx.part_id("image"))
        .role(Role::Image)
        .aria_label(alt)
        .max_w_full()
        .child(
            img(ImageSource::from(src))
                .max_w_full()
                .rounded(t.radius.element),
        )
        .into_any_element()
}

/// Blocks with the collapsed margins between them. Each block's wrapper has the debug
/// selector `{path}-{index}`, and its parts take theirs from it.
fn render_blocks(
    blocks: &[Block],
    path: &str,
    list_depth: usize,
    cx: &mut Cx,
    window: &mut Window,
    app: &mut App,
) -> Vec<AnyElement> {
    let mut out = Vec::with_capacity(blocks.len());
    let mut previous_end = px(0.);
    for (index, block) in blocks.iter().enumerate() {
        let selector = format!("{path}-{index}");
        let (element, (start, end)) = render_block(block, &selector, list_depth, cx, window, app);
        let gap = if index == 0 {
            px(0.)
        } else {
            previous_end.max(start)
        };
        previous_end = end;
        out.push(
            div()
                .debug_selector(move || selector.clone())
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .mt(gap)
                .child(element)
                .into_any_element(),
        );
    }
    out
}

/// Upstream `renderBlock`: one block and its margins. A supplied renderer's part has
/// none (FR3).
fn render_block(
    block: &Block,
    selector: &str,
    list_depth: usize,
    cx: &mut Cx,
    window: &mut Window,
    app: &mut App,
) -> (AnyElement, (Pixels, Pixels)) {
    let margins = spacing(block, cx.density, cx.theme, cx.rem);
    let none = (px(0.), px(0.));
    let lines = block_source_lines(block);
    let own = block_own_direction(block);
    let direction = block_direction(own, cx);
    let t = cx.theme.tokens();
    match block {
        Block::Heading(heading) => {
            let index = cx.headings_seen.get();
            cx.headings_seen.set(index + 1);
            let projected = cx.projection.heading(index);
            let id = projected.map(|h| SharedString::from(h.id.clone()));
            let level = heading_level(heading.depth, cx.heading_level_start);
            let children = render_inline(&heading.children, cx, window, app);
            if let Some(render) = cx.components.heading.clone() {
                let props = MarkdownHeadingProps {
                    level: level as u8,
                    id,
                    children,
                    source_lines: lines,
                };
                return (render(props, window, app), none);
            }
            let mut el = Heading::new(level, "").children(children);
            if let Some(id) = id.clone() {
                el = el.id(ElementId::Name(id));
            }
            el.style().text.refine(&aligned(direction));
            target(
                el.style(),
                cx,
                "markdown-heading",
                &[("level", level_name(level))],
            );
            let el = match projected.and_then(|h| h.permalink_url.clone().map(|url| (h, url))) {
                // The heading-links plugin: a permalink copy control after the heading.
                Some((projected, url)) => prose(div().flex(), cx)
                    .child(HeadingLinksRow {
                        id: ElementId::Name(format!("{selector}-heading-links").into()),
                        heading: el.flex_initial().min_w_0().into_any_element(),
                        label: projected.label.clone().into(),
                        heading_id: projected.id.clone().into(),
                        permalink: url.into(),
                        level,
                    })
                    .into_any_element(),
                None => prose(el, cx).into_any_element(),
            };
            (in_direction(direction, el, app), margins)
        }
        Block::Paragraph(paragraph) => {
            let children = render_inline(&paragraph.children, cx, window, app);
            if let Some(render) = cx.components.paragraph.clone() {
                let props = MarkdownParagraphProps {
                    children,
                    source_lines: lines,
                };
                return (render(props, window, app), none);
            }
            // A block, never a text paragraph, so anything can compose inside it.
            let part = format!("{selector}-paragraph");
            let mut el = prose(
                div()
                    .id(cx.part_id("paragraph"))
                    .debug_selector(move || part.clone())
                    .role(Role::Paragraph),
                cx,
            );
            target(el.style(), cx, "markdown-paragraph", &[]);
            let el = el.child(span(aligned(direction), children));
            (in_direction(direction, el.into_any_element(), app), margins)
        }
        Block::Code(code) => {
            cx.advance(code.value.len());
            let legacy = code.legacy_language().unwrap_or("plaintext");
            if let Some(render) = cx.components.code.clone() {
                let props = MarkdownCodeProps {
                    code: code.value.clone().into(),
                    language: legacy.to_string().into(),
                    source_lines: lines,
                };
                return (render(props, window, app), none);
            }
            let (language, diff) = fence_language(code, cx.code_languages);
            let id = ElementId::Name(format!("{selector}-code").into());
            let mut block = CodeBlock::new(id, code.value.clone())
                .language(language)
                .diff(diff)
                .collapsible(true)
                .wrapped(true);
            // At least the measure wide, where the space allows: min(measure, 100%).
            block = match cx.measure {
                Measure::None => block,
                Measure::Length(DefiniteLength::Fraction(f)) => block.min_w(relative(f.min(1.))),
                Measure::Length(DefiniteLength::Absolute(width)) => {
                    block.min_width_floor(width.to_pixels(cx.rem))
                }
            };
            if cx.align == MarkdownContentAlign::Center {
                block = block.mx_auto();
            }
            let mut el = div().flex().flex_col().w_full().min_w_0();
            target(el.style(), cx, "markdown-codeblock", &[]);
            let el = el.child(block).into_any_element();
            // A semantic fence's proposal renders through its plugin, the code block
            // standing in when it cannot.
            let el = match code.fence_proposal() {
                Some(node) => {
                    render_plugin_node(cx.plugins, node, || el, cx.on_diagnostic, window, app)
                }
                None => el,
            };
            (el, margins)
        }
        Block::Math(math) => {
            cx.advance(math.value.len());
            if let Some(render) = cx.components.math.clone() {
                let props = MarkdownMathProps {
                    value: math.value.clone().into(),
                    display: MarkdownMathDisplay::Block,
                    source_lines: lines,
                };
                return (render(props, window, app), none);
            }
            // Without a math renderer, display math reads as written, a paragraph with
            // no spacing of its own (FR6).
            let el = div()
                .id(cx.part_id("math"))
                .role(Role::Paragraph)
                .child(span(
                    TextStyleRefinement::default(),
                    vec![text(format!("$${}$$", math.value))],
                ));
            (el.into_any_element(), none)
        }
        Block::Blockquote(quote) => {
            let saved = cx.around;
            cx.around = own.unwrap_or(cx.provider);
            let children = render_blocks(&quote.children, selector, list_depth, cx, window, app);
            cx.around = saved;
            if let Some(render) = cx.components.blockquote.clone() {
                let props = MarkdownBlockquoteProps {
                    children,
                    source_lines: lines,
                };
                return (render(props, window, app), none);
            }
            let mut el = prose(Blockquote::new(), cx).children(children);
            target(el.style(), cx, "markdown-blockquote", &[]);
            (in_direction(direction, el.into_any_element(), app), margins)
        }
        Block::List(list) => {
            let content = render_list(list, selector, list_depth, cx, window, app);
            let mut el = prose(div().flex().flex_col(), cx);
            target(el.style(), cx, "markdown-list", &[]);
            let el = el.child(content);
            (in_direction(direction, el.into_any_element(), app), margins)
        }
        Block::Table(table) => (render_table(table, selector, cx, window, app), margins),
        Block::ThematicBreak(_) => {
            if let Some(render) = cx.components.hr.clone() {
                let props = MarkdownHrProps {
                    source_lines: lines,
                };
                return (render(props, window, app), none);
            }
            let mut el = div()
                .id(cx.part_id("hr"))
                .role(Role::Splitter)
                .w_full()
                .h(px(0.))
                .border_t(t.border.width)
                .border_color(t.color.border);
            target(el.style(), cx, "markdown-hr", &[]);
            (el.into_any_element(), margins)
        }
        Block::Image(image) => {
            if !image.url.is_empty()
                && let Some(render) = cx.components.image.clone()
            {
                let props = MarkdownImageProps {
                    src: image.url.clone().into(),
                    alt: image.alt.clone().into(),
                    source_lines: lines,
                };
                return (render(props, window, app), none);
            }
            let mut el = div().flex().flex_col().items_start();
            target(el.style(), cx, "markdown-image", &[]);
            let el = el.child(render_image(image, cx, window, app));
            (el.into_any_element(), margins)
        }
        Block::Extension(extension) => {
            let fallback = cx.fade_text(&extension_fallback(extension, cx));
            let content = render_plugin_node(
                cx.plugins,
                extension,
                || span(TextStyleRefinement::default(), vec![fallback]),
                cx.on_diagnostic,
                window,
                app,
            );
            (prose(div(), cx).child(content).into_any_element(), margins)
        }
    }
}

/// FR29: the language and diff layer a fence gives CodeBlock. The fence's full first
/// word picks the layer (`diff` over plaintext, `diff-<language>` over that language,
/// by its leading word); any other fence paints its leading word.
pub(crate) fn code_fence(code: &ast::Code) -> (String, bool) {
    let legacy = code.legacy_language().unwrap_or("plaintext");
    let (fence_language, diff) =
        rustryx_syntax::parse_fence_language(Some(code.lang.as_deref().unwrap_or(legacy)));
    if !diff {
        return (legacy.to_string(), false);
    }
    let word: String = fence_language
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (
        if word.is_empty() {
            "plaintext".into()
        } else {
            word
        },
        true,
    )
}

/// FR30: the fence's language as CodeBlock takes it; a listed language module claims
/// its identifiers.
pub(crate) fn fence_language(
    code: &ast::Code,
    languages: &[CodeLanguage],
) -> (CodeLanguageInput, bool) {
    let (word, diff) = code_fence(code);
    let language = match listed_language(languages, &word) {
        Some(listed) => CodeLanguageInput::Language(listed.clone()),
        None => CodeLanguageInput::Name(word.into()),
    };
    (language, diff)
}

/// An extension node's readable text: its source, or its plugin's text projection.
fn extension_fallback(node: &ast::ExtensionNode, cx: &Cx) -> String {
    node.source.clone().unwrap_or_else(|| {
        rustryx_markdown::plugins::markdown_extension_text(cx.plugins, node, cx.on_diagnostic)
    })
}

/// Upstream `blockSourceLines`: the lines a block came from, when the parse recorded them
/// (the source-lines plugin).
fn block_source_lines(block: &Block) -> Option<MarkdownSourceLines> {
    let position = match block {
        Block::Heading(n) => n.position,
        Block::Paragraph(n) => n.position,
        Block::Code(n) => n.position,
        Block::Math(n) => n.position,
        Block::Blockquote(n) => n.position,
        Block::List(n) => n.position,
        Block::Table(n) => n.position,
        Block::ThematicBreak(n) => n.position,
        Block::Image(n) => n.position,
        Block::Extension(n) => n.position,
    }?;
    Some(MarkdownSourceLines {
        start: position.start.line?,
        end: position.end.line?,
    })
}

/// How long the copy control shows its result.
const COPY_FEEDBACK: std::time::Duration = std::time::Duration::from_millis(1500);

/// Upstream `HeadingLinksRenderer`: a heading and, after it, a ghost `#` button that copies
/// the heading's permalink, shown while the pointer rests on the row or focus is in it.
#[derive(IntoElement)]
struct HeadingLinksRow {
    id: ElementId,
    heading: AnyElement,
    label: SharedString,
    heading_id: SharedString,
    permalink: SharedString,
    level: HeadingLevel,
}

impl RenderOnce for HeadingLinksRow {
    fn render(self, window: &mut Window, app: &mut App) -> impl IntoElement {
        let theme = current(window, app);
        let t = theme.tokens();
        let rem = window.rem_size();
        let i18n = rustryx_i18n::current(app);
        let copied_at = window.use_keyed_state(
            ElementId::Name(format!("{}-copied", self.id).into()),
            app,
            |_, _| None::<web_time::Instant>,
        );
        let now = app.background_executor().now();
        let copied = copied_at
            .read(app)
            .is_some_and(|at| now.duration_since(at) < COPY_FEEDBACK);
        if copied {
            // Back to the copy label once the feedback ends.
            let entity = window.current_view();
            app.spawn(async move |cx| {
                cx.background_executor().timer(COPY_FEEDBACK).await;
                cx.update(|cx| cx.notify(entity));
            })
            .detach();
        }
        let name = if copied {
            i18n.t("@astryx.markdownHeadingLinks.copied")
        } else {
            let heading = if self.label.is_empty() {
                &self.heading_id
            } else {
                &self.label
            };
            i18n.format(
                "@astryx.markdownHeadingLinks.copy",
                &[("heading", heading.to_string().into())],
            )
        };
        let glyph = if copied {
            Icon::new(IconSource::Named("check".into()))
                .size(IconSize::Sm)
                .into_any_element()
        } else {
            SharedString::from("#").into_any_element()
        };
        let reveal = use_container_reveal(self.id.clone(), Default::default(), window, app);
        let style = heading_text(self.level, t);
        let line = style.size.to_pixels(rem) * style.leading;
        let permalink = self.permalink.clone();
        // Upstream `COARSE_TARGET`: with a touch screen present the slot and the button take
        // 24 px, the WCAG minimum, in place of the small element size.
        // Upstream keys this on `(any-pointer: coarse)`: any touch screen.
        let coarse = rustryx_primitives::modality::any_pointer_coarse(window, app);
        let slot_w: gpui::Length = if coarse {
            px(COARSE_TARGET).into()
        } else {
            t.size.element_sm.into()
        };
        let button = Button::new(ElementId::Name(format!("{}-copy", self.id).into()), name)
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .icon_only(true)
            .icon(glyph)
            .when(coarse, |b| b.w(px(COARSE_TARGET)).h(px(COARSE_TARGET)))
            .on_click(move |event, _, cx| {
                // An unmodified primary activation only.
                if event.modifiers().modified() {
                    return;
                }
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(permalink.to_string()));
                let now = cx.background_executor().now();
                copied_at.update(cx, |at, _| *at = Some(now));
            });
        reveal
            .container(Default::default())
            .flex()
            .items_end()
            .min_w_0()
            .max_w_full()
            .child(self.heading)
            .child(
                div()
                    .relative()
                    .flex_none()
                    .w(slot_w)
                    .h(line)
                    .ml(t.spacing.s1_5)
                    .text_color(t.color.text_secondary)
                    .child(
                        reveal
                            .content(ContentRevealOptions {
                                layout_preserved: true,
                                ..Default::default()
                            })
                            .debug_selector({
                                let selector = format!("{}-copy", self.id);
                                move || selector
                            })
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(button),
                    ),
            )
    }
}

/// Upstream `COARSE_TARGET`: the heading link's target under a coarse pointer, px.
const COARSE_TARGET: f32 = 24.;

/// A heading level's type style, as the Heading component draws it.
fn heading_text(level: HeadingLevel, t: &rustryx_theme::Tokens) -> &rustryx_theme::TextStyleTokens {
    match level {
        HeadingLevel::H1 => &t.text.heading_1,
        HeadingLevel::H2 => &t.text.heading_2,
        HeadingLevel::H3 => &t.text.heading_3,
        HeadingLevel::H4 => &t.text.heading_4,
        HeadingLevel::H5 => &t.text.heading_5,
        HeadingLevel::H6 => &t.text.heading_6,
    }
}

fn level_name(level: HeadingLevel) -> &'static str {
    ["1", "2", "3", "4", "5", "6"][level as usize - 1]
}

/// An item's label: its paragraph's inline content when that is all it holds, its
/// blocks otherwise.
fn item_content(
    item: &ast::ListItem,
    selector: String,
    list_depth: usize,
    cx: &mut Cx,
    window: &mut Window,
    app: &mut App,
) -> AnyElement {
    let saved = cx.around;
    cx.around = blocks_direction(&item.children).unwrap_or(cx.provider);
    let content = match item.children.as_slice() {
        [Block::Paragraph(p)] => {
            let direction = block_direction(phrasing_direction(&p.children), cx);
            span(
                aligned(direction),
                render_inline(&p.children, cx, window, app),
            )
        }
        blocks => div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .children(render_blocks(
                blocks,
                &selector,
                list_depth + 1,
                cx,
                window,
                app,
            ))
            .into_any_element(),
    };
    cx.around = saved;
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .child(content)
        .into_any_element()
}

/// The plain text of an item's first paragraph, its task box's name.
fn item_name(item: &ast::ListItem) -> Option<SharedString> {
    item.children.iter().find_map(|block| match block {
        Block::Paragraph(p) => Some(ast::markdown_ast_text(&p.children).into()),
        _ => None,
    })
}

/// The markers nested lists cycle through (upstream `BULLET_MARKERS`, `NUMBER_MARKERS`).
const BULLET_MARKERS: [ListMarker; 3] = [ListMarker::Disc, ListMarker::Circle, ListMarker::Square];
const NUMBER_MARKERS: [ListMarker; 3] = [
    ListMarker::Decimal,
    ListMarker::LowerAlpha,
    ListMarker::LowerRoman,
];

fn render_list(
    list: &ast::List,
    selector: &str,
    list_depth: usize,
    cx: &mut Cx,
    window: &mut Window,
    app: &mut App,
) -> AnyElement {
    let task_label = rustryx_i18n::current(app).t("@astryx.markdown.taskList");
    // A list of task items that hold only paragraphs is a read-only CheckboxList.
    let task_list = !list.children.is_empty()
        && list.children.iter().all(|item| {
            item.checked.is_some()
                && item
                    .children
                    .iter()
                    .all(|block| matches!(block, Block::Paragraph(_)))
        });
    if task_list {
        let mut checked = Vec::new();
        let mut items = Vec::new();
        for (i, item) in list.children.iter().enumerate() {
            let value: SharedString = format!("task-{i}").into();
            if item.checked == Some(true) {
                checked.push(value.clone());
            }
            let label = item_content(
                item,
                format!("{selector}-item-{i}"),
                list_depth,
                cx,
                window,
                app,
            );
            items.push(
                CheckboxListItem::new_element(label)
                    .value(value)
                    .aria_label(item_name(item).unwrap_or_default()),
            );
        }
        return CheckboxList::new(cx.part_id("tasks"), task_label)
            .label_hidden(true)
            .value(checked)
            .read_only(true)
            .density(ItemDensity::Compact)
            .items(items)
            .into_any_element();
    }
    // Otherwise a List; a task item in it keeps its box in the marker's place (FR23).
    let markers = if list.ordered {
        NUMBER_MARKERS
    } else {
        BULLET_MARKERS
    };
    let marker = markers[list_depth % 3];
    let mut el = List::new(cx.part_id("list"))
        .list_style(if list.ordered {
            ListStyle::Decimal
        } else {
            ListStyle::Disc
        })
        .density(ItemDensity::Compact)
        .edge(ListEdge::Flush);
    if list.ordered {
        el = el.start(list.start.unwrap_or(1));
    }
    for (i, item) in list.children.iter().enumerate() {
        let label = item_content(
            item,
            format!("{selector}-item-{i}"),
            list_depth,
            cx,
            window,
            app,
        );
        let task = item.checked.map(|checked| ListTask {
            checked,
            label: item_name(item).unwrap_or_else(|| task_label.clone()),
        });
        let row = ListItem::new_element(cx.part_id("item"), label);
        el = el.child(MarkerScope { marker, task, row });
    }
    el.into_any_element()
}

/// A row in its marker scope, which reads the list's context as the row renders.
#[derive(IntoElement)]
struct MarkerScope {
    marker: ListMarker,
    task: Option<ListTask>,
    row: ListItem,
}

impl RenderOnce for MarkerScope {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        list_marker_scope(self.marker, self.task, self.row, cx)
    }
}

/// Upstream's table block: a Table with grid dividers and wrapping cells, each column at
/// least its content-derived floor (FR26), the header never truncated (FR27), the
/// Table's own region the only scroller (FR28).
fn render_table(
    table: &ast::Table,
    selector: &str,
    cx: &mut Cx,
    window: &mut Window,
    app: &mut App,
) -> AnyElement {
    let t = cx.theme.tokens();
    let floors = table_column_floors(table);
    let pad = t.spacing.s2;
    let pad_px = pad.to_pixels(cx.rem);
    let align = |column: usize| match table.align.get(column).copied().flatten() {
        Some(ast::TableAlign::Center) => Some(TableColumnAlign::Center),
        Some(ast::TableAlign::Right) => Some(TableColumnAlign::End),
        _ => None,
    };
    let (header, rows) = match table.children.split_first() {
        Some((header, rows)) => (Some(header), rows),
        None => (None, &[][..]),
    };
    let mut head_row = TableRow::new();
    for (column, cell) in header.iter().flat_map(|h| h.children.iter().enumerate()) {
        let floor = floors.get(column).copied().unwrap_or(4) as f32;
        let mut el = TableHeaderCell::new()
            .p(pad)
            .min_w(cx.ch * floor + pad_px * 2.)
            .child(span(
                TextStyleRefinement::default(),
                render_inline(&cell.children, cx, window, app),
            ));
        if let Some(a) = align(column) {
            el = el.align(a);
        }
        head_row = head_row.child(el);
    }
    let mut body = TableBody::new();
    for row in rows {
        let mut row_el = TableRow::new();
        for (column, cell) in row.children.iter().enumerate() {
            let mut el = TableCell::new().p(pad).child(span(
                TextStyleRefinement::default(),
                render_inline(&cell.children, cx, window, app),
            ));
            if let Some(a) = align(column) {
                el = el.align(a);
            }
            row_el = row_el.child(el);
        }
        body = body.child(row_el);
    }
    let table_el = Table::composed(ElementId::Name(format!("{selector}-table").into()))
        .dividers(TableDividers::Grid)
        .text_overflow(TableTextOverflow::Wrap)
        .border(t.border.width)
        .border_color(t.color.border)
        .child(TableHeader::new().child(head_row))
        .child(body);
    let mut el = div().flex().flex_col().w_full().min_w_0();
    target(el.style(), cx, "markdown-table", &[]);
    el.child(table_el).into_any_element()
}

/// Upstream `applyInlinePlugins`: the text split at every plugin match, each match
/// rendered by its plugin. Matches are taken by start; one overlapping an earlier
/// match is dropped, so the first plugin wins a tie.
fn apply_inline_plugins(
    value: &str,
    cx: &Cx,
    window: &mut Window,
    app: &mut App,
) -> Vec<AnyElement> {
    let plugins = cx.inline_plugins;
    struct Found<'t> {
        start: usize,
        end: usize,
        captures: fancy_regex::Captures<'t>,
        plugin: usize,
    }
    let mut found = Vec::new();
    for (index, plugin) in plugins.iter().enumerate() {
        for captures in plugin.pattern.captures_iter(value).flatten() {
            let whole = captures.get(0).expect("a match has group 0");
            let end = match &plugin.end_index {
                Some(f) => match f(value, &captures) {
                    Some(end) => end,
                    None => continue,
                },
                None => whole.end(),
            };
            found.push(Found {
                start: whole.start(),
                end,
                captures,
                plugin: index,
            });
        }
    }
    found.sort_by_key(|m| m.start);
    let mut out = Vec::new();
    let mut cursor = 0;
    for m in found {
        if m.start < cursor {
            continue;
        }
        if m.start > cursor {
            out.push(cx.fade_text(&value[cursor..m.start]));
        }
        out.push((plugins[m.plugin].render)(&m.captures, window, app));
        let end = m.end.min(value.len()).max(m.start);
        cx.advance(end - m.start);
        cursor = end;
    }
    if cursor < value.len() {
        out.push(cx.fade_text(&value[cursor..]));
    }
    out
}

/// What the fade-in remembers between renders.
#[derive(Default)]
struct FadeState {
    /// The revealed source's length at the last render.
    source_len: Option<usize>,
    /// The text the last render showed.
    rendered_len: usize,
    boundaries: Vec<usize>,
    /// When each boundary first appeared: its span's fade starts then.
    started: HashMap<usize, Instant>,
}

struct Fade {
    state: gpui::Entity<FadeState>,
    boundaries: Vec<usize>,
    fades: HashMap<usize, f32>,
    animating: bool,
}

/// Upstream's boundaries and fade-in: a new boundary each time the revealed source
/// grows, at most as many as the fast motion's ticks, each span fading in over the
/// medium duration from when its boundary appeared.
fn fade_state(
    id: &ElementId,
    source_len: usize,
    streaming: bool,
    theme: &ResolvedTheme,
    window: &mut Window,
    app: &mut App,
) -> Fade {
    let t = theme.tokens();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-markdown-fade").into()),
        app,
        |_, _| FadeState::default(),
    );
    let tick_ms = {
        let base = t.duration.fast_min.as_secs_f32() * 1000.;
        if base > 0. {
            (base / 10.).round().max(4.)
        } else {
            50.
        }
    };
    let max_spans = ((t.duration.fast_max.as_secs_f32() * 1000. / tick_ms).ceil() as usize).min(12);
    let duration = if rustryx_primitives::prefers_reduced_motion(app) {
        std::time::Duration::ZERO
    } else {
        t.duration.medium
    };
    let ease = t.ease.standard;
    let now = app.background_executor().now();
    state.update(app, |s, _| {
        if s.source_len != Some(source_len) {
            s.source_len = Some(source_len);
            s.boundaries = compute_boundaries(&s.boundaries, s.rendered_len, max_spans.max(1));
        }
        let boundaries = s.boundaries.clone();
        s.started.retain(|b, _| boundaries.contains(b));
        for b in &boundaries {
            s.started.entry(*b).or_insert(now);
        }
        let mut animating = false;
        let fades = s
            .started
            .iter()
            .map(|(&b, &at)| {
                let progress = if duration.is_zero() {
                    1.
                } else {
                    (now.duration_since(at).as_secs_f32() / duration.as_secs_f32()).min(1.)
                };
                animating |= streaming && progress < 1.;
                (b, ease.ease(progress))
            })
            .collect();
        Fade {
            state: state.clone(),
            boundaries,
            fades,
            animating,
        }
    })
}

fn parse_key(source: &str, options: &ParseOptions, inline: bool) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    // FR15: every parse-affecting option and each syntax plugin's identity.
    options.parse_identity().hash(&mut hasher);
    inline.hash(&mut hasher);
    hasher.finish()
}

impl RenderOnce for Markdown {
    fn render(self, window: &mut Window, app: &mut App) -> impl IntoElement {
        let theme = current(window, app);
        let t = theme.tokens();
        let rem = window.rem_size();
        let inline = self.display == TextDisplay::Inline;
        let options = ParseOptions {
            autolink_gfm: self.autolink == Some(MarkdownAutolink::Gfm),
            // FR6: math parses only for a math renderer.
            math: self.components.math.is_some(),
            source_ids: self
                .sources
                .as_ref()
                .map(|sources| sources.keys().cloned().collect()),
            plugins: self.plugins.clone(),
            on_plugin_diagnostic: self.on_plugin_diagnostic.clone(),
            // Transforms that need the whole input wait until streaming ends (FR21).
            is_final: !self.streaming,
            ..ParseOptions::default()
        };
        // While streaming, the source reveals steadily.
        let source = if self.streaming {
            use_streaming_text(
                self.id.clone(),
                &self.source,
                true,
                UseStreamingTextOptions::default(),
                window,
                app,
            )
        } else {
            self.source.clone()
        };
        let key = parse_key(&source, &options, inline);
        let cache = window.use_keyed_state(
            ElementId::Name(format!("{}-markdown-parse", self.id).into()),
            app,
            |_, _| ParseCache::default(),
        );
        let stale = {
            let c = cache.read(app);
            c.key != Some(key)
                || c.plugins.len() != self.plugins.len()
                || c.plugins
                    .iter()
                    .zip(&self.plugins)
                    .any(|(a, b)| !a.ptr_eq(b))
        };
        if stale {
            let prepared = prepare_markdown_plugins(&self.plugins)
                .ok()
                .filter(|_| !self.plugins.is_empty())
                .map(Rc::new);
            // While streaming, partial syntax is trimmed and settled blocks are reused
            // (FR11); a change of autolink, math or plugins starts the state again.
            let streaming = self.streaming;
            let input = if streaming {
                trim_streaming_artifacts(&source, options.math)
            } else {
                source.to_string()
            };
            let identity = options.parse_identity();
            let (blocks, phrasing, projection) = if inline {
                (
                    Vec::new(),
                    parse_inline_ast(&input, &options),
                    Default::default(),
                )
            } else {
                let root = cache.update(app, |c, _| {
                    if !streaming {
                        c.incremental = None;
                        return parse_markdown_ast(&input, &options);
                    }
                    let reset = input.is_empty()
                        || c.incremental
                            .as_ref()
                            .is_none_or(|(key, _)| *key != identity);
                    if reset {
                        c.incremental = Some((identity.clone(), create_incremental_state()));
                    }
                    let (_, state) = c.incremental.as_mut().expect("just made");
                    parse_markdown_ast_incremental(&input, state, &options)
                });
                let projection = project_markdown_headings(&root, prepared.as_deref());
                (root.children, Vec::new(), projection)
            };
            cache.update(app, |c, _| {
                c.key = Some(key);
                c.plugins = self.plugins.clone();
                c.blocks = Rc::new(blocks);
                c.inline = Rc::new(phrasing);
                c.prepared = prepared;
                c.projection = Rc::new(projection);
            });
        }
        let (blocks, phrasing, prepared, projection) = {
            let c = cache.read(app);
            (
                c.blocks.clone(),
                c.inline.clone(),
                c.prepared.clone(),
                c.projection.clone(),
            )
        };

        let ch = {
            let mut probe = div()
                .font_stack(&t.font.family.body)
                .text_style(&t.text.body);
            let mut style = window.text_style();
            style.refine(&probe.style().text);
            let font_size = style.font_size.to_pixels(rem);
            let text_system = window.text_system();
            let font = text_system.resolve_font(&style.font());
            text_system
                .advance(font, font_size, '0')
                .map(|s| s.width)
                .unwrap_or(font_size * 0.5)
        };
        let fade = fade_state(&self.id, source.len(), self.streaming, &theme, window, app);
        let provider = rustryx_i18n::use_direction(app);
        let mut cx = Cx {
            id: &self.id,
            theme: &theme,
            density: self.density,
            heading_level_start: self.heading_level_start,
            on_link_click: self.on_link_click.clone(),
            components: &self.components,
            measure: Measure::of(self.content_width),
            align: self.content_align,
            rem,
            ch,
            provider,
            around: provider,
            projection: &projection,
            headings_seen: Cell::new(0),
            plugins: prepared.as_deref(),
            on_diagnostic: self.on_plugin_diagnostic.as_ref(),
            next: RefCell::new(0),
            citations: self.sources.as_deref().map(|sources| {
                RefCell::new(Citations {
                    sources,
                    numbers: HashMap::new(),
                    style: self.citation_style,
                })
            }),
            inline_plugins: &self.inline_plugins,
            code_languages: &self.code_languages,
            cursor: RefCell::new(Cursor {
                offset: 0,
                active: self.streaming,
                boundaries: fade.boundaries,
                fades: fade.fades,
            }),
        };
        // The text this render shows, where the next render's newest text begins.
        let rendered = if inline {
            count_inline_text_length(&phrasing)
        } else {
            count_block_text_length(&blocks)
        };
        fade.state.update(app, |s, _| s.rendered_len = rendered);
        if fade.animating {
            window.request_animation_frame();
        }

        if inline {
            // FR1: inline display renders no block anatomy and takes its paragraph's
            // type and color.
            let mut style = StyleRefinement::default();
            target(&mut style, &cx, "markdown", &[]);
            style.refine(&self.style);
            return span(style.text, render_inline(&phrasing, &cx, window, app));
        }

        let path = format!("{}-block", self.id);
        let children = render_blocks(&blocks, &path, 0, &mut cx, window, app);
        let mut el = div()
            .id(self.id.clone())
            .role(Role::Document)
            .flex()
            .flex_col()
            .w_full()
            .max_w_full()
            .min_w_0()
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .text_color(t.color.text_primary);
        target(el.style(), &cx, "markdown", &[]);
        el.style().refine(&self.style);
        el.children(children).into_any_element()
    }
}
