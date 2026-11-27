//! The Comments family's glyphs (upstream lab `Comments/icons.tsx`): the active
//! theme's icon for each `comment:*` key when one is registered, else a bundled glyph
//! drawn on a 24 px grid with 1.5 px round strokes.

use gpui::prelude::*;
use gpui::{
    App, Bounds, Hsla, LineCap, LineJoin, PathBuilder, PathStyle, Pixels, Point, StrokeOptions,
    Window, canvas, point, px,
};
use rustryx_components::icon::{Icon, IconColor, IconSize};

/// Upstream `COMMENT_ICON_KEYS`: stable icon-registry keys for the family's glyphs.
pub mod comment_icon_keys {
    pub const ADD_REACTION: &str = "comment:addReaction";
    pub const REPLY: &str = "comment:reply";
    pub const COPY_LINK: &str = "comment:copyLink";
    pub const RESOLVE: &str = "comment:resolve";
    pub const EDIT: &str = "comment:edit";
    pub const DELETE: &str = "comment:delete";
    pub const ATTACH: &str = "comment:attach";
    pub const EMOJI: &str = "comment:emoji";
    pub const EVENT: &str = "comment:event";
}

/// Upstream `CommentIconName`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentIconName {
    AddReaction,
    Reply,
    CopyLink,
    Resolve,
    Edit,
    Delete,
    Attach,
    Emoji,
    Event,
}

impl CommentIconName {
    /// The registry key.
    pub fn key(self) -> &'static str {
        use comment_icon_keys::*;
        match self {
            Self::AddReaction => ADD_REACTION,
            Self::Reply => REPLY,
            Self::CopyLink => COPY_LINK,
            Self::Resolve => RESOLVE,
            Self::Edit => EDIT,
            Self::Delete => DELETE,
            Self::Attach => ATTACH,
            Self::Emoji => EMOJI,
            Self::Event => EVENT,
        }
    }

    /// The bundled glyph's paths, or `None` where the system registry already has the
    /// glyph (`resolve` draws `check`).
    pub(crate) fn paths(self) -> Option<&'static [&'static str]> {
        Some(match self {
            // A smiling face with a plus at its upper end.
            Self::AddReaction => &[
                "M20.5 12.5A8.5 8.5 0 1 1 11.5 3.5",
                "M8.5 14.5a4 4 0 0 0 7 0",
                "M9 10h.01M15 10h.01",
                "M19 2.5v5M16.5 5h5",
            ],
            // A speech bubble.
            Self::Reply => &[
                "M4.5 5.5h15a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H10l-4.5 3.5v-3.5h-1a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z",
            ],
            // Two chain links.
            Self::CopyLink => &[
                "M10 14a4.5 4.5 0 0 0 6.36 0l3-3a4.5 4.5 0 0 0-6.36-6.36l-1.25 1.25",
                "M14 10a4.5 4.5 0 0 0-6.36 0l-3 3a4.5 4.5 0 0 0 6.36 6.36l1.25-1.25",
            ],
            Self::Resolve => return None,
            // A pencil.
            Self::Edit => &["M15.5 4.5l4 4L8 20H4v-4L15.5 4.5z", "M13.5 6.5l4 4"],
            // A bin with a lid.
            Self::Delete => &[
                "M4.5 6.5h15",
                "M9.5 6.5v-2a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v2",
                "M6.5 6.5l1 13a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1l1-13",
                "M10.5 10.5v6M13.5 10.5v6",
            ],
            // A paper clip.
            Self::Attach => &[
                "M19.5 11.5l-7.6 7.6a4.5 4.5 0 0 1-6.36-6.36l7.95-7.95a3 3 0 0 1 4.24 4.24l-7.6 7.6a1.5 1.5 0 0 1-2.12-2.12l7.07-7.07",
            ],
            // A smiling face.
            Self::Emoji => &[
                "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z",
                "M8.5 14.5a4 4 0 0 0 7 0",
                "M9 10h.01M15 10h.01",
            ],
            // A point on a timeline.
            Self::Event => &[
                "M12 3v5.5M12 15.5V21",
                "M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z",
            ],
        })
    }
}

/// One SVG path command, absolute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PathCommand {
    Move(f32, f32),
    Line(f32, f32),
    Arc {
        rx: f32,
        ry: f32,
        rotation: f32,
        large: bool,
        sweep: bool,
        x: f32,
        y: f32,
    },
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// The numbers of an SVG path's arguments, as SVG reads them (`.01`, `-1-1`, `1 1`).
fn numbers(text: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b' ' || c == b',' {
            i += 1;
            continue;
        }
        let start = i;
        if c == b'-' || c == b'+' {
            i += 1;
        }
        let mut dot = false;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && !dot)) {
            dot |= bytes[i] == b'.';
            i += 1;
        }
        if i == start {
            i += 1;
            continue;
        }
        if let Ok(n) = text[start..i].parse::<f32>() {
            out.push(n);
        }
    }
    out
}

/// An SVG path's `d` as absolute commands (M, L, H, V, A, C and Z, either case).
pub(crate) fn parse_path(d: &str) -> Vec<PathCommand> {
    let mut out = Vec::new();
    let (mut x, mut y, mut sx, mut sy) = (0f32, 0f32, 0f32, 0f32);
    let mut chars = d.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !c.is_ascii_alphabetic() {
            continue;
        }
        let end = chars.peek().map(|(j, _)| *j).unwrap_or(d.len());
        let mut j = end;
        while j < d.len() && !d.as_bytes()[j].is_ascii_alphabetic() {
            j += 1;
        }
        while chars.peek().is_some_and(|(k, _)| *k < j) {
            chars.next();
        }
        let args = numbers(&d[i + 1..j]);
        let rel = c.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0., 0.) };
        match c.to_ascii_uppercase() {
            'M' => {
                for (k, pair) in args.chunks(2).enumerate() {
                    if pair.len() < 2 {
                        break;
                    }
                    let (ox, oy) = if rel { (x, y) } else { (0., 0.) };
                    (x, y) = (ox + pair[0], oy + pair[1]);
                    if k == 0 {
                        (sx, sy) = (x, y);
                        out.push(PathCommand::Move(x, y));
                    } else {
                        out.push(PathCommand::Line(x, y));
                    }
                }
            }
            'L' => {
                for pair in args.chunks(2).filter(|p| p.len() == 2) {
                    let (ox, oy) = if rel { (x, y) } else { (0., 0.) };
                    (x, y) = (ox + pair[0], oy + pair[1]);
                    out.push(PathCommand::Line(x, y));
                }
            }
            'H' => {
                for v in &args {
                    x = if rel { x + v } else { *v };
                    out.push(PathCommand::Line(x, y));
                }
            }
            'V' => {
                for v in &args {
                    y = if rel { y + v } else { *v };
                    out.push(PathCommand::Line(x, y));
                }
            }
            'A' => {
                for a in args.chunks(7).filter(|a| a.len() == 7) {
                    let (ox, oy) = if rel { (x, y) } else { (0., 0.) };
                    (x, y) = (ox + a[5], oy + a[6]);
                    out.push(PathCommand::Arc {
                        rx: a[0],
                        ry: a[1],
                        rotation: a[2],
                        large: a[3] != 0.,
                        sweep: a[4] != 0.,
                        x,
                        y,
                    });
                }
            }
            'C' => {
                for a in args.chunks(6).filter(|a| a.len() == 6) {
                    let (ox, oy) = if rel { (x, y) } else { (0., 0.) };
                    out.push(PathCommand::Cubic(
                        ox + a[0],
                        oy + a[1],
                        ox + a[2],
                        oy + a[3],
                        ox + a[4],
                        oy + a[5],
                    ));
                    (x, y) = (ox + a[4], oy + a[5]);
                }
            }
            'Z' => {
                (x, y) = (sx, sy);
                out.push(PathCommand::Close);
            }
            _ => {}
        }
        let _ = (ox, oy);
    }
    out
}

/// Strokes a glyph's paths into `bounds` (a 24-unit grid) in `color`.
fn paint_glyph(paths: &[&str], bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let unit = f32::from(bounds.size.width) / 24.;
    let at =
        |x: f32, y: f32| -> Point<Pixels> { bounds.origin + point(px(x * unit), px(y * unit)) };
    for d in paths {
        let mut b =
            PathBuilder::stroke(px(1.5 * unit)).with_style(PathStyle::Stroke(StrokeOptions {
                line_width: 1.5 * unit,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..StrokeOptions::default()
            }));
        for command in parse_path(d) {
            match command {
                PathCommand::Move(x, y) => b.move_to(at(x, y)),
                PathCommand::Line(x, y) => b.line_to(at(x, y)),
                PathCommand::Arc {
                    rx,
                    ry,
                    rotation,
                    large,
                    sweep,
                    x,
                    y,
                } => b.arc_to(
                    point(px(rx * unit), px(ry * unit)),
                    px(rotation),
                    large,
                    sweep,
                    at(x, y),
                ),
                PathCommand::Cubic(x1, y1, x2, y2, x, y) => {
                    b.cubic_bezier_to(at(x, y), at(x1, y1), at(x2, y2))
                }
                PathCommand::Close => b.close(),
            }
        }
        if let Ok(path) = b.build() {
            window.paint_path(path, color);
        }
    }
}

/// Upstream `CommentIcon`: a comment glyph at the icon slot's size: the active theme's
/// icon for its `comment:*` key when one is registered, else the bundled default.
#[derive(IntoElement)]
pub struct CommentIcon {
    name: CommentIconName,
    color: IconColor,
    size: IconSize,
}

impl CommentIcon {
    /// A glyph in secondary color at the small size.
    pub fn new(name: CommentIconName) -> Self {
        CommentIcon {
            name,
            color: IconColor::Secondary,
            size: IconSize::Sm,
        }
    }

    /// Upstream `color` (secondary by default).
    pub fn color(mut self, color: IconColor) -> Self {
        self.color = color;
        self
    }

    /// Upstream `size` (small by default).
    pub fn size(mut self, size: IconSize) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for CommentIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let key = self.name.key();
        // A theme entry for the key answers it.
        if theme.theme.icon(key).is_some() {
            return Icon::new(key)
                .color(self.color)
                .size(self.size)
                .into_any_element();
        }
        let Some(paths) = self.name.paths() else {
            return Icon::new("check")
                .color(self.color)
                .size(self.size)
                .into_any_element();
        };
        let t = theme.tokens();
        let color = self
            .color
            .token(t)
            .or_else(|| window.text_style().color.into())
            .unwrap_or(t.color.icon_primary);
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| paint_glyph(paths, bounds, color, window),
        )
        .flex_none()
        .size(self.size.rems())
        .into_any_element()
    }
}
