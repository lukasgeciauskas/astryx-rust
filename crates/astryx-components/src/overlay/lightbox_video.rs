//! A Lightbox page's video: the counterpart of upstream's `<video controls>`, its
//! frames drawn through `astryx_primitives::media` and the platform-style controls
//! a browser draws over it.

use std::cell::Cell;
use std::rc::Rc;

use astryx_i18n::{Arg, current as i18n};
use astryx_primitives::media::{MediaElement, MediaFrame};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    App, Bounds, ElementId, Entity, Hsla, IntoElement, ObjectFit, ParentElement, Pixels,
    RenderOnce, Role, SharedString, Styled, Window, canvas, div, img, prelude::*, px, svg,
};

use crate::button::{ButtonVariant, IconButton};
use crate::slider::{Slider, SliderValue, ValueDisplay};

/// Upstream `VIDEO_CONTROLS_BAND`: the height of a video's controls, px.
pub(super) const VIDEO_CONTROLS_BAND: f32 = 48.;

/// The controls' glyphs: Lucide's drawings (ISC license), as the browser's own.
fn glyph(body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"#
    )
}

const PLAY: &str = r#"<path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z"/>"#;
const PAUSE: &str = r#"<rect x="14" y="3" width="5" height="18" rx="1"/><rect x="5" y="3" width="5" height="18" rx="1"/>"#;
const SPEAKER: &str = r#"<path d="M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z"/>"#;

fn icon(body: String) -> impl IntoElement {
    svg().data(body.as_bytes()).size(px(16.)).flex_none()
}

/// `m:ss`, or `h:mm:ss` past an hour: the browser's time readout.
fn clock(seconds: f64) -> String {
    let total = if seconds.is_finite() {
        seconds.max(0.).floor() as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The rectangle a picture of `ratio` (width / height) fills inside `bounds` when
/// fitted whole (`object-fit: contain`).
pub(super) fn contain(bounds: Bounds<Pixels>, ratio: f32) -> Bounds<Pixels> {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    if w <= 0. || h <= 0. || ratio <= 0. {
        return bounds;
    }
    let (fw, fh) = if w / h > ratio {
        (h * ratio, h)
    } else {
        (w, w / ratio)
    };
    Bounds {
        origin: gpui::point(
            bounds.origin.x + px((w - fw) / 2.),
            bounds.origin.y + px((h - fh) / 2.),
        ),
        size: gpui::size(px(fw), px(fh)),
    }
}

/// A Lightbox page's video, named by its `alt`, with its controls over the bottom
/// of the picture.
#[derive(IntoElement)]
pub(super) struct LightboxVideo {
    pub id: ElementId,
    pub element: Entity<MediaElement>,
    pub alt: SharedString,
    /// Where the picture lies, in window coordinates, as last painted: a press
    /// outside it is beside the media.
    pub picture: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl RenderOnce for LightboxVideo {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let key = format!("{}", self.id);
        let (frame, ratio, paused, muted, time, duration) = {
            let m = self.element.read(cx);
            let ratio = m
                .video_size()
                .map_or(16. / 9., |s| s.width as f32 / s.height.max(1) as f32);
            (
                m.frame().cloned(),
                ratio,
                m.paused(),
                m.muted(),
                m.current_time(),
                m.duration(),
            )
        };
        let box_bounds = window
            .use_keyed_state(ElementId::Name(format!("{key}-box").into()), cx, |_, _| {
                Rc::new(Cell::new(None::<Bounds<Pixels>>))
            })
            .read(cx)
            .clone();
        let fitted = box_bounds.get().map(|b| contain(b, ratio));
        self.picture.set(fitted);
        let picture: Option<gpui::AnyElement> = match frame {
            Some(MediaFrame::Image(image)) => Some(
                img(image)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
            #[cfg(target_os = "macos")]
            Some(MediaFrame::PixelBuffer(buffer)) => Some(
                gpui::surface(buffer)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
            None => None,
        };
        let i18n = i18n(cx);
        let toggle_label = i18n.t(if paused {
            "@astryx.mediaPlayer.play"
        } else {
            "@astryx.mediaPlayer.pause"
        });
        let mute_label = i18n.t(if muted {
            "@astryx.mediaPlayer.unmute"
        } else {
            "@astryx.mediaPlayer.mute"
        });
        let seekable = duration.is_finite() && duration > 0.;
        let (play_el, mute_el, seek_el, click_el) = (
            self.element.clone(),
            self.element.clone(),
            self.element.clone(),
            self.element.clone(),
        );
        let readout = i18n.format(
            "@astryx.mediaPlayer.timeReadout",
            &[
                ("current", Arg::from(clock(time).as_str())),
                ("duration", Arg::from(clock(duration).as_str())),
            ],
        );
        let on_dark = t.color.on_dark;
        let band = div()
            .id(ElementId::Name(format!("{key}-controls").into()))
            .debug_selector({
                let key = key.clone();
                move || format!("{key}-controls")
            })
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(px(VIDEO_CONTROLS_BAND))
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .px(t.spacing.s2)
            .bg(Hsla { h: 0., s: 0., l: 0., a: 0.5 })
            .text_color(on_dark)
            .text_style(&t.text.supporting)
            // A press on the controls stays the video's: it starts no page drag.
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div().debug_selector({
                    let key = key.clone();
                    move || format!("{key}-toggle")
                }).child(
                    IconButton::new(ElementId::Name(format!("{key}-toggle").into()), toggle_label, icon(glyph(if paused { PLAY } else { PAUSE })))
                        .variant(ButtonVariant::Ghost)
                        .on_click(move |_, _, cx| {
                            play_el.update(cx, |m, cx| {
                                if m.paused() || m.ended() {
                                    m.play(cx);
                                } else {
                                    m.pause(cx);
                                }
                            })
                        }),
                ),
            )
            .child(div().flex_none().whitespace_nowrap().child(readout))
            .child(
                div().flex_1().min_w_0().child(
                    Slider::new(ElementId::Name(format!("{key}-seek").into()), i18n.t("@astryx.mediaPlayer.seek"), if seekable { time.min(duration) } else { 0. })
                        .label_hidden(true)
                        .min(0.)
                        .max(if seekable { duration } else { 1. })
                        .step(0.01)
                        .disabled(!seekable)
                        .value_display(ValueDisplay::None)
                        .on_change(move |value, _, cx| {
                            if let SliderValue::Single(time) = value {
                                seek_el.update(cx, |m, cx| m.set_current_time(time, cx));
                            }
                        })
                        .width(gpui::relative(1.)),
                ),
            )
            .child(
                IconButton::new(
                    ElementId::Name(format!("{key}-mute").into()),
                    mute_label,
                    icon(if muted {
                        glyph(&format!(r#"{SPEAKER}<line x1="22" x2="16" y1="9" y2="15"/><line x1="16" x2="22" y1="9" y2="15"/>"#))
                    } else {
                        glyph(&format!(r#"{SPEAKER}<path d="M16 9a5 5 0 0 1 0 6"/><path d="M19.364 18.364a9 9 0 0 0 0-12.728"/>"#))
                    }),
                )
                .variant(ButtonVariant::Ghost)
                .on_click(move |_, _, cx| mute_el.update(cx, |m, cx| m.set_muted(!m.muted(), cx))),
            );
        let measure = box_bounds.clone();
        let mut outer = div().relative().size_full().child(
            canvas(
                move |bounds, window, _| {
                    if measure.replace(Some(bounds)) != Some(bounds) {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
        if let (Some(fit), Some(outer_bounds)) = (fitted, box_bounds.get()) {
            let selector = format!("{key}-picture");
            outer = outer.child(
                div()
                    .id(ElementId::Name(format!("{key}-picture").into()))
                    .debug_selector(move || selector)
                    .role(Role::Video)
                    .aria_label(self.alt)
                    .absolute()
                    .left(fit.origin.x - outer_bounds.origin.x)
                    .top(fit.origin.y - outer_bounds.origin.y)
                    .w(fit.size.width)
                    .h(fit.size.height)
                    .overflow_hidden()
                    // A click on the picture plays and pauses, as a browser's video does.
                    .on_click(move |_, _, cx| {
                        click_el.update(cx, |m, cx| {
                            if m.paused() || m.ended() {
                                m.play(cx);
                            } else {
                                m.pause(cx);
                            }
                        })
                    })
                    .children(picture)
                    .child(band),
            );
        }
        outer
    }
}
