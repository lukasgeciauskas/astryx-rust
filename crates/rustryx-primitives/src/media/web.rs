//! The web: an `HTMLVideoElement` or `HTMLAudioElement` plays the media, and each
//! new video frame is read through a canvas into a GPUI image.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{RenderImage, Size, size};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlMediaElement, HtmlVideoElement};

use super::{
    MediaBackend, MediaCrossOrigin, MediaError, MediaEventKind, MediaFrame, MediaKind,
    MediaPreload, MediaSource,
};

/// The element's events this backend follows.
const EVENTS: [(&str, MediaEventKind); 17] = [
    ("loadstart", MediaEventKind::LoadStart),
    ("loadedmetadata", MediaEventKind::LoadedMetadata),
    ("durationchange", MediaEventKind::DurationChange),
    ("timeupdate", MediaEventKind::TimeUpdate),
    ("progress", MediaEventKind::Progress),
    ("play", MediaEventKind::Play),
    ("playing", MediaEventKind::Playing),
    ("pause", MediaEventKind::Pause),
    ("ended", MediaEventKind::Ended),
    ("waiting", MediaEventKind::Waiting),
    ("canplay", MediaEventKind::CanPlay),
    ("seeking", MediaEventKind::Seeking),
    ("seeked", MediaEventKind::Seeked),
    ("volumechange", MediaEventKind::VolumeChange),
    ("ratechange", MediaEventKind::RateChange),
    ("error", MediaEventKind::Error),
    ("emptied", MediaEventKind::Emptied),
];

/// The widest frame read back; a larger video is scaled down to it.
const MAX_FRAME_WIDTH: u32 = 1920;

type Queue = Rc<RefCell<Vec<MediaEventKind>>>;
/// An event listener on the element, by the event's name.
type Listener = (&'static str, Closure<dyn FnMut()>);

/// The backend on the web.
pub struct HtmlMediaBackend {
    element: Option<HtmlMediaElement>,
    kind: MediaKind,
    queue: Queue,
    listeners: Vec<Listener>,
    canvas: Option<(HtmlCanvasElement, CanvasRenderingContext2d)>,
    /// The playhead of the last frame read, so a still video reads no new one.
    last_frame: Option<f64>,
    /// Where the platform refused a canvas read (a cross-origin video without CORS).
    tainted: bool,
}

impl HtmlMediaBackend {
    pub fn new() -> Self {
        HtmlMediaBackend {
            element: None,
            kind: MediaKind::Video,
            queue: Rc::default(),
            listeners: Vec::new(),
            canvas: None,
            last_frame: None,
            tainted: false,
        }
    }

    fn detach(&mut self) {
        if let Some(element) = &self.element {
            for (name, listener) in self.listeners.drain(..) {
                let _ = element
                    .remove_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
            }
            element.set_src("");
        }
        self.element = None;
    }

    fn create(&mut self, kind: MediaKind) -> Option<HtmlMediaElement> {
        let document = web_sys::window()?.document()?;
        let tag = match kind {
            MediaKind::Video => "video",
            MediaKind::Audio => "audio",
        };
        let element: HtmlMediaElement = document.create_element(tag).ok()?.dyn_into().ok()?;
        let _ = element.set_attribute("playsinline", "");
        for (name, event) in EVENTS {
            let queue = self.queue.clone();
            let listener = Closure::<dyn FnMut()>::new(move || queue.borrow_mut().push(event));
            let _ =
                element.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
            self.listeners.push((name, listener));
        }
        Some(element)
    }

    fn read_frame(&mut self) -> Option<MediaFrame> {
        if self.kind != MediaKind::Video || self.tainted {
            return None;
        }
        let video: HtmlVideoElement = self.element.as_ref()?.clone().dyn_into().ok()?;
        // HAVE_CURRENT_DATA: a frame to draw.
        if video.ready_state() < 2 {
            return None;
        }
        let time = video.current_time();
        if self.last_frame == Some(time) {
            return None;
        }
        let (vw, vh) = (video.video_width(), video.video_height());
        if vw == 0 || vh == 0 {
            return None;
        }
        let w = vw.min(MAX_FRAME_WIDTH);
        let h = ((f64::from(vh) * f64::from(w) / f64::from(vw)).round() as u32).max(1);
        if self.canvas.is_none() {
            let document = web_sys::window()?.document()?;
            let canvas: HtmlCanvasElement =
                document.create_element("canvas").ok()?.dyn_into().ok()?;
            let context: CanvasRenderingContext2d =
                canvas.get_context("2d").ok()??.dyn_into().ok()?;
            self.canvas = Some((canvas, context));
        }
        let (canvas, context) = self.canvas.as_ref()?;
        if canvas.width() != w || canvas.height() != h {
            canvas.set_width(w);
            canvas.set_height(h);
        }
        context
            .draw_image_with_html_video_element_and_dw_and_dh(
                &video,
                0.,
                0.,
                f64::from(w),
                f64::from(h),
            )
            .ok()?;
        let data = match context.get_image_data(0., 0., f64::from(w), f64::from(h)) {
            Ok(data) => data,
            Err(_) => {
                // The video came from another origin without CORS: the platform keeps
                // its pixels from script. It still plays; no frame draws.
                self.tainted = true;
                return None;
            }
        };
        self.last_frame = Some(time);
        let mut pixels = data.data().0;
        // RGBA to the BGRA GPUI images hold.
        for px in pixels.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let buffer = image::RgbaImage::from_raw(w, h, pixels)?;
        Some(MediaFrame::Image(Arc::new(RenderImage::new(vec![
            image::Frame::new(buffer),
        ]))))
    }
}

impl Default for HtmlMediaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for HtmlMediaBackend {
    fn drop(&mut self) {
        if let Some(element) = &self.element {
            let _ = element.pause();
        }
        self.detach();
    }
}

impl MediaBackend for HtmlMediaBackend {
    fn load(&mut self, source: &MediaSource) {
        if self.element.is_none() || self.kind != source.kind {
            self.detach();
            self.kind = source.kind;
            self.element = self.create(source.kind);
        }
        self.last_frame = None;
        self.tainted = false;
        let Some(element) = &self.element else {
            self.queue.borrow_mut().push(MediaEventKind::Error);
            return;
        };
        element.set_preload(match source.preload {
            MediaPreload::None => "none",
            MediaPreload::Metadata => "metadata",
            MediaPreload::Auto => "auto",
        });
        element.set_cross_origin(source.cross_origin.map(|c| match c {
            MediaCrossOrigin::Anonymous => "anonymous",
            MediaCrossOrigin::UseCredentials => "use-credentials",
        }));
        element.set_autoplay(source.autoplay);
        element.set_loop(source.looping);
        element.set_default_muted(source.muted);
        element.set_muted(source.muted);
        element.set_src(&source.src);
        element.load();
    }
    fn play(&mut self) -> bool {
        let Some(element) = &self.element else {
            return false;
        };
        match element.play() {
            Ok(promise) => {
                // A refusal (an autoplay policy) arrives later and leaves the element
                // paused; caught here so it reports nothing else.
                let refused = Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(|_| {});
                let _ = promise.catch(&refused);
                refused.forget();
                true
            }
            Err(_) => false,
        }
    }
    fn pause(&mut self) {
        if let Some(element) = &self.element {
            let _ = element.pause();
        }
    }
    fn paused(&self) -> bool {
        self.element.as_ref().is_none_or(|e| e.paused())
    }
    fn ended(&self) -> bool {
        self.element.as_ref().is_some_and(|e| e.ended())
    }
    fn current_time(&self) -> f64 {
        self.element.as_ref().map_or(0., |e| e.current_time())
    }
    fn set_current_time(&mut self, time: f64) {
        if let Some(element) = &self.element {
            element.set_current_time(time);
        }
    }
    fn duration(&self) -> f64 {
        self.element.as_ref().map_or(f64::NAN, |e| e.duration())
    }
    fn buffered(&self) -> Vec<Range<f64>> {
        let Some(element) = &self.element else {
            return Vec::new();
        };
        let ranges = element.buffered();
        (0..ranges.length())
            .filter_map(|i| Some(ranges.start(i).ok()?..ranges.end(i).ok()?))
            .collect()
    }
    fn volume(&self) -> f64 {
        self.element.as_ref().map_or(1., |e| e.volume())
    }
    fn set_volume(&mut self, volume: f64) {
        if let Some(element) = &self.element {
            element.set_volume(volume);
        }
    }
    fn muted(&self) -> bool {
        self.element.as_ref().is_some_and(|e| e.muted())
    }
    fn set_muted(&mut self, muted: bool) {
        if let Some(element) = &self.element {
            element.set_muted(muted);
        }
    }
    fn playback_rate(&self) -> f64 {
        self.element.as_ref().map_or(1., |e| e.playback_rate())
    }
    fn set_playback_rate(&mut self, rate: f64) {
        if let Some(element) = &self.element {
            element.set_playback_rate(rate);
        }
    }
    fn error(&self) -> Option<MediaError> {
        let error = self.element.as_ref()?.error()?;
        Some(MediaError {
            code: error.code(),
            message: error.message().into(),
        })
    }
    fn video_size(&self) -> Option<Size<u32>> {
        let video: HtmlVideoElement = self.element.as_ref()?.clone().dyn_into().ok()?;
        let (w, h) = (video.video_width(), video.video_height());
        (w > 0 && h > 0).then(|| size(w, h))
    }
    fn take_events(&mut self) -> Vec<MediaEventKind> {
        std::mem::take(&mut *self.queue.borrow_mut())
    }
    fn take_frame(&mut self) -> Option<MediaFrame> {
        self.read_frame()
    }
    /// iOS Safari ignores script writes to `volume`, so a probe element shows
    /// whether the platform lets script set it (upstream `canScriptSetVolume`).
    fn can_set_volume(&self) -> bool {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return true;
        };
        let Some(probe) = document
            .create_element("audio")
            .ok()
            .and_then(|e| e.dyn_into::<HtmlMediaElement>().ok())
        else {
            return false;
        };
        probe.set_volume(0.5);
        probe.volume() == 0.5
    }
}
