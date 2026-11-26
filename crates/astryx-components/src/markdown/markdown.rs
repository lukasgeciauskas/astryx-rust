//! Markdown, upstream `Markdown/Markdown.tsx`: a Markdown string rendered with Astryx's
//! own components. The parser is `astryx_markdown`, a port of upstream's.
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

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use astryx_i18n::{Direction, first_strong_direction};
use astryx_markdown::ast::{self, Block, Phrasing};
use astryx_markdown::{ParseOptions, parse_inline_ast, parse_markdown_ast};
use astryx_theme::{ResolvedTheme, ThemeStyled as _, current};
use gpui::{
    AbsoluteLength, AnyElement, App, ClickEvent, DefiniteLength, ElementId, FontStyle, ImageSource,
    IntoElement, Length, ParentElement, Pixels, Refineable as _, RenderOnce, Role, SharedString,
    StrikethroughStyle, StyleRefinement, Styled, TextAlign, TextStyleRefinement, Window, div, img,
    prelude::*, px, relative,
};

use crate::choice::{CheckboxList, CheckboxListItem};
use crate::code_block::CodeBlock;
use crate::content::{Blockquote, Code};
use crate::item::ItemDensity;
use crate::link::Link;
use crate::list::{List, ListEdge, ListItem, ListMarker, ListStyle, ListTask, list_marker_scope};
use crate::table::{
    Table, TableBody, TableCell, TableColumnAlign, TableDividers, TableHeader, TableHeaderCell,
    TableRow, TableTextOverflow,
};
use crate::text::{Heading, HeadingLevel, InlineSpan, TextDisplay, TextType};

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

/// What a supplied `code` renderer receives.
pub struct MarkdownCodeProps {
    pub code: SharedString,
    /// The fence's leading word (`diff` for `diff-tsx`), `plaintext` when it names none.
    pub language: SharedString,
}

/// What a supplied `heading` renderer receives.
pub struct MarkdownHeadingProps {
    /// 1 to 6, after `heading_level_start`.
    pub level: u8,
    /// A root heading's anchor slug; headings inside quotes and lists have none.
    pub id: Option<SharedString>,
    pub children: Vec<AnyElement>,
}

/// What a supplied `image` renderer receives.
pub struct MarkdownImageProps {
    pub src: SharedString,
    pub alt: SharedString,
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
    paragraph: Option<Render<Vec<AnyElement>>>,
    image: Option<Render<MarkdownImageProps>>,
    blockquote: Option<Render<Vec<AnyElement>>>,
    hr: Option<Render<()>>,
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
        paragraph: Vec<AnyElement>
    );
    renderer!(
        /// An image with a safe source, inline or a block of its own.
        image: MarkdownImageProps
    );
    renderer!(
        /// A blockquote, given its rendered blocks.
        blockquote: Vec<AnyElement>
    );
    renderer!(
        /// A thematic break.
        hr: ()
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
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
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

/// The last parse, kept until the source or its options change.
#[derive(Default)]
struct ParseCache {
    key: Option<u64>,
    blocks: Rc<Vec<Block>>,
    inline: Rc<Vec<Phrasing>>,
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
    /// The root headings' anchor slugs, in order.
    heading_ids: std::vec::IntoIter<SharedString>,
    /// Numbers the ids of stateful parts.
    next: RefCell<usize>,
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

/// Upstream `projectMarkdownHeadings`: the anchor slug of each root heading, numbered
/// apart when two read alike.
pub(crate) fn heading_ids(blocks: &[Block]) -> Vec<SharedString> {
    let mut counts = HashMap::new();
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Heading(h) => {
                let label = ast::markdown_ast_text(&h.children);
                let slug = astryx_markdown::slugify(label.trim());
                Some(astryx_markdown::unique_slug(&slug, &mut counts).into())
            }
            _ => None,
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
    let current = astryx_i18n::current(app);
    match direction {
        Some(d) if d != current.direction() => {
            astryx_i18n::provider(current.dir(d), el).into_any_element()
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
        Phrasing::Text(literal) => text(literal.value.clone()),
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
            let value = SharedString::from(code.value.clone());
            match &cx.components.inline_code {
                Some(render) => render(value, window, app),
                None => Code::new(value).into_any_element(),
            }
        }
        // Math needs a math renderer; without one it reads as written (FR6).
        Phrasing::InlineMath(math) => text(format!("${}$", math.value)),
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
        Phrasing::Break(_) => text("\n"),
        // Without sources a citation reads as its marker.
        Phrasing::Citation(c) => text(format!("[{}]", c.source_id)),
        Phrasing::Extension(e) => text(e.source.clone().unwrap_or_default()),
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
        return render(MarkdownImageProps { src, alt }, window, app);
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
    root: bool,
    list_depth: usize,
    cx: &mut Cx,
    window: &mut Window,
    app: &mut App,
) -> Vec<AnyElement> {
    let mut out = Vec::with_capacity(blocks.len());
    let mut previous_end = px(0.);
    for (index, block) in blocks.iter().enumerate() {
        let selector = format!("{path}-{index}");
        let (element, (start, end)) =
            render_block(block, &selector, root, list_depth, cx, window, app);
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
    root: bool,
    list_depth: usize,
    cx: &mut Cx,
    window: &mut Window,
    app: &mut App,
) -> (AnyElement, (Pixels, Pixels)) {
    let margins = spacing(block, cx.density, cx.theme, cx.rem);
    let none = (px(0.), px(0.));
    let own = block_own_direction(block);
    let direction = block_direction(own, cx);
    let t = cx.theme.tokens();
    match block {
        Block::Heading(heading) => {
            let id = if root { cx.heading_ids.next() } else { None };
            let level = heading_level(heading.depth, cx.heading_level_start);
            let children = render_inline(&heading.children, cx, window, app);
            if let Some(render) = cx.components.heading.clone() {
                let props = MarkdownHeadingProps {
                    level: level as u8,
                    id,
                    children,
                };
                return (render(props, window, app), none);
            }
            let mut el = Heading::new(level, "").children(children);
            if let Some(id) = id {
                el = el.id(ElementId::Name(id));
            }
            el.style().text.refine(&aligned(direction));
            let mut el = prose(el, cx);
            target(
                el.style(),
                cx,
                "markdown-heading",
                &[("level", level_name(level))],
            );
            (in_direction(direction, el.into_any_element(), app), margins)
        }
        Block::Paragraph(paragraph) => {
            let children = render_inline(&paragraph.children, cx, window, app);
            if let Some(render) = cx.components.paragraph.clone() {
                return (render(children, window, app), none);
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
            let legacy = code.legacy_language().unwrap_or("plaintext");
            if let Some(render) = cx.components.code.clone() {
                let props = MarkdownCodeProps {
                    code: code.value.clone().into(),
                    language: legacy.to_string().into(),
                };
                return (render(props, window, app), none);
            }
            let (language, diff) = code_fence(code);
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
            (el.child(block).into_any_element(), margins)
        }
        Block::Math(math) => {
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
            let children = render_blocks(
                &quote.children,
                selector,
                false,
                list_depth,
                cx,
                window,
                app,
            );
            cx.around = saved;
            if let Some(render) = cx.components.blockquote.clone() {
                return (render(children, window, app), none);
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
                return (render((), window, app), none);
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
                };
                return (render(props, window, app), none);
            }
            let mut el = div().flex().flex_col().items_start();
            target(el.style(), cx, "markdown-image", &[]);
            let el = el.child(render_image(image, cx, window, app));
            (el.into_any_element(), margins)
        }
        Block::Extension(extension) => {
            let source = extension.source.clone().unwrap_or_default();
            let el =
                prose(div(), cx).child(span(TextStyleRefinement::default(), vec![text(source)]));
            (el.into_any_element(), margins)
        }
    }
}

/// FR29: the language and diff layer a fence gives CodeBlock. The fence's full first
/// word picks the layer (`diff` over plaintext, `diff-<language>` over that language,
/// by its leading word); any other fence paints its leading word.
pub(crate) fn code_fence(code: &ast::Code) -> (String, bool) {
    let legacy = code.legacy_language().unwrap_or("plaintext");
    let (fence_language, diff) =
        astryx_syntax::parse_fence_language(Some(code.lang.as_deref().unwrap_or(legacy)));
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
                false,
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
    let task_label = astryx_i18n::current(app).t("@astryx.markdown.taskList");
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

fn parse_key(source: &str, options: &ParseOptions, inline: bool) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    options.autolink_gfm.hash(&mut hasher);
    options.math.hash(&mut hasher);
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
            ..ParseOptions::default()
        };
        let key = parse_key(&self.source, &options, inline);
        let cache = window.use_keyed_state(
            ElementId::Name(format!("{}-markdown-parse", self.id).into()),
            app,
            |_, _| ParseCache::default(),
        );
        if cache.read(app).key != Some(key) {
            let (blocks, phrasing) = if inline {
                (Vec::new(), parse_inline_ast(&self.source, &options))
            } else {
                (
                    parse_markdown_ast(&self.source, &options).children,
                    Vec::new(),
                )
            };
            cache.update(app, |c, _| {
                c.key = Some(key);
                c.blocks = Rc::new(blocks);
                c.inline = Rc::new(phrasing);
            });
        }
        let (blocks, phrasing) = {
            let c = cache.read(app);
            (c.blocks.clone(), c.inline.clone())
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
        let provider = astryx_i18n::use_direction(app);
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
            heading_ids: heading_ids(&blocks).into_iter(),
            next: RefCell::new(0),
        };

        if inline {
            // FR1: inline display renders no block anatomy and takes its paragraph's
            // type and color.
            let mut style = StyleRefinement::default();
            target(&mut style, &cx, "markdown", &[]);
            style.refine(&self.style);
            return span(style.text, render_inline(&phrasing, &cx, window, app));
        }

        let path = format!("{}-block", self.id);
        let children = render_blocks(&blocks, &path, true, 0, &mut cx, window, app);
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
