//! Windows: Media Foundation's `IMFMediaEngine` in frame-server mode. The engine
//! reports the media element's events through its notify callback (on Media
//! Foundation's threads, queued for the main thread) and hands each new video frame
//! to a WIC bitmap, copied into a GPUI image.

use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};

use gpui::{RenderImage, Size, size};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICBitmap, IWICImagingFactory,
    WICBitmapCacheOnLoad, WICBitmapLockRead, WICRect,
};
use windows::Win32::Media::MediaFoundation::{
    CLSID_MFMediaEngineClassFactory, IMFMediaEngine, IMFMediaEngineClassFactory,
    IMFMediaEngineNotify, IMFMediaEngineNotify_Impl, MF_MEDIA_ENGINE_CALLBACK,
    MF_MEDIA_ENGINE_PRELOAD, MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT, MF_VERSION, MFCreateAttributes,
    MFSTARTUP_FULL, MFStartup,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::{BSTR, IUnknown, Interface, implement};

use super::{MediaBackend, MediaError, MediaEventKind, MediaFrame, MediaPreload, MediaSource};

/// The engine's events, queued from Media Foundation's threads.
type Queue = Arc<Mutex<Vec<u32>>>;

#[implement(IMFMediaEngineNotify)]
struct Notify {
    queue: Queue,
}

impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, _: usize, _: u32) -> windows::core::Result<()> {
        if let Ok(mut queue) = self.queue.lock() {
            queue.push(event);
        }
        Ok(())
    }
}

/// `MF_MEDIA_ENGINE_EVENT` values as the DOM's events.
fn event_kind(event: u32) -> Option<MediaEventKind> {
    Some(match event {
        1 => MediaEventKind::LoadStart,
        2 => MediaEventKind::Progress,
        5 => MediaEventKind::Error,
        6 => MediaEventKind::Emptied,
        8 => MediaEventKind::Play,
        9 => MediaEventKind::Pause,
        10 => MediaEventKind::LoadedMetadata,
        12 => MediaEventKind::Waiting,
        13 => MediaEventKind::Playing,
        14 => MediaEventKind::CanPlay,
        16 => MediaEventKind::Seeking,
        17 => MediaEventKind::Seeked,
        18 => MediaEventKind::TimeUpdate,
        19 => MediaEventKind::Ended,
        20 => MediaEventKind::RateChange,
        21 => MediaEventKind::DurationChange,
        22 => MediaEventKind::VolumeChange,
        _ => return None,
    })
}

/// Starts COM and Media Foundation once for the process.
fn started() -> bool {
    static STARTED: OnceLock<bool> = OnceLock::new();
    *STARTED.get_or_init(|| unsafe {
        // The main thread's COM is already up (GPUI's); another thread joins the MTA.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL).is_ok()
    })
}

fn create_engine(queue: &Queue) -> windows::core::Result<IMFMediaEngine> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: IMFMediaEngineClassFactory =
            CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 2)?;
        let attributes = attributes.ok_or_else(windows::core::Error::empty)?;
        let notify: IMFMediaEngineNotify = Notify {
            queue: queue.clone(),
        }
        .into();
        attributes.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify.cast::<IUnknown>()?)?;
        attributes.SetUINT32(
            &MF_MEDIA_ENGINE_VIDEO_OUTPUT_FORMAT,
            DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32,
        )?;
        factory.CreateInstance(0, &attributes)
    }
}

/// A source as the URL the engine takes: a local path becomes a `file:` URL.
fn source_url(src: &str) -> String {
    if src.contains("://") || src.starts_with("data:") {
        return src.to_string();
    }
    let path = std::fs::canonicalize(src)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| src.to_string());
    let path = path
        .strip_prefix(r"\\?\")
        .unwrap_or(&path)
        .replace('\\', "/");
    format!("file:///{}", path.trim_start_matches('/'))
}

/// The backend on Windows.
pub struct MediaEngineBackend {
    engine: Option<IMFMediaEngine>,
    queue: Queue,
    wic: Option<IWICImagingFactory>,
    bitmap: Option<(IWICBitmap, u32, u32)>,
    last_frame: Option<i64>,
    failure: Option<MediaError>,
    pending: Vec<MediaEventKind>,
}

impl MediaEngineBackend {
    /// A backend with its engine, or one whose every source fails when Media
    /// Foundation cannot start.
    pub fn new() -> Self {
        let queue: Queue = Arc::default();
        let engine = if started() {
            create_engine(&queue).ok()
        } else {
            None
        };
        let wic =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() };
        MediaEngineBackend {
            engine,
            queue,
            wic,
            bitmap: None,
            last_frame: None,
            failure: None,
            pending: Vec::new(),
        }
    }

    fn frame(&mut self) -> Option<MediaFrame> {
        let engine = self.engine.as_ref()?;
        unsafe {
            if !engine.HasVideo().as_bool() {
                return None;
            }
            let pts = engine.OnVideoStreamTick().ok()?;
            if self.last_frame == Some(pts) {
                return None;
            }
            let (mut w, mut h) = (0u32, 0u32);
            engine.GetNativeVideoSize(Some(&mut w), Some(&mut h)).ok()?;
            if w == 0 || h == 0 {
                return None;
            }
            if self
                .bitmap
                .as_ref()
                .is_none_or(|(_, bw, bh)| (*bw, *bh) != (w, h))
            {
                let bitmap = self
                    .wic
                    .as_ref()?
                    .CreateBitmap(w, h, &GUID_WICPixelFormat32bppBGRA, WICBitmapCacheOnLoad)
                    .ok()?;
                self.bitmap = Some((bitmap, w, h));
            }
            let (bitmap, _, _) = self.bitmap.as_ref()?;
            let rect = RECT {
                left: 0,
                top: 0,
                right: w as i32,
                bottom: h as i32,
            };
            engine.TransferVideoFrame(bitmap, None, &rect, None).ok()?;
            self.last_frame = Some(pts);
            let lock = bitmap
                .Lock(
                    &WICRect {
                        X: 0,
                        Y: 0,
                        Width: w as i32,
                        Height: h as i32,
                    },
                    WICBitmapLockRead.0 as u32,
                )
                .ok()?;
            let stride = lock.GetStride().ok()? as usize;
            let mut len = 0u32;
            let mut data = std::ptr::null_mut();
            lock.GetDataPointer(&mut len, &mut data).ok()?;
            let source = std::slice::from_raw_parts(data, len as usize);
            let row = w as usize * 4;
            let mut pixels = Vec::with_capacity(row * h as usize);
            for y in 0..h as usize {
                pixels.extend_from_slice(&source[y * stride..y * stride + row]);
            }
            // The video is opaque; its alpha byte may be unset.
            for alpha in pixels.iter_mut().skip(3).step_by(4) {
                *alpha = 255;
            }
            let buffer = image::RgbaImage::from_raw(w, h, pixels)?;
            Some(MediaFrame::Image(Arc::new(RenderImage::new(vec![
                image::Frame::new(buffer),
            ]))))
        }
    }
}

impl Default for MediaEngineBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MediaEngineBackend {
    fn drop(&mut self) {
        if let Some(engine) = self.engine.take() {
            unsafe {
                let _ = engine.Shutdown();
            }
        }
    }
}

impl MediaBackend for MediaEngineBackend {
    fn load(&mut self, source: &MediaSource) {
        self.last_frame = None;
        let Some(engine) = self.engine.as_ref() else {
            self.failure = Some(MediaError::unsupported(
                "Media Foundation is not available.",
            ));
            self.pending.extend([
                MediaEventKind::Emptied,
                MediaEventKind::LoadStart,
                MediaEventKind::Error,
            ]);
            return;
        };
        let preload = match source.preload {
            MediaPreload::None => 2,
            MediaPreload::Metadata => 3,
            MediaPreload::Auto => 4,
        };
        unsafe {
            let _ = engine.SetPreload(MF_MEDIA_ENGINE_PRELOAD(preload));
            let _ = engine.SetAutoPlay(source.autoplay);
            let _ = engine.SetLoop(source.looping);
            let _ = engine.SetMuted(source.muted);
            let _ = engine.SetSource(&BSTR::from(source_url(&source.src)));
        }
    }
    fn play(&mut self) -> bool {
        self.engine
            .as_ref()
            .is_some_and(|e| unsafe { e.Play().is_ok() })
    }
    fn pause(&mut self) {
        if let Some(e) = &self.engine {
            unsafe {
                let _ = e.Pause();
            }
        }
    }
    fn paused(&self) -> bool {
        self.engine
            .as_ref()
            .is_none_or(|e| unsafe { e.IsPaused().as_bool() })
    }
    fn ended(&self) -> bool {
        self.engine
            .as_ref()
            .is_some_and(|e| unsafe { e.IsEnded().as_bool() })
    }
    fn current_time(&self) -> f64 {
        self.engine
            .as_ref()
            .map_or(0., |e| unsafe { e.GetCurrentTime() })
    }
    fn set_current_time(&mut self, time: f64) {
        if let Some(e) = &self.engine {
            unsafe {
                let _ = e.SetCurrentTime(time);
            }
        }
    }
    fn duration(&self) -> f64 {
        self.engine
            .as_ref()
            .map_or(f64::NAN, |e| unsafe { e.GetDuration() })
    }
    fn buffered(&self) -> Vec<Range<f64>> {
        let Some(e) = &self.engine else {
            return Vec::new();
        };
        unsafe {
            let Ok(ranges) = e.GetBuffered() else {
                return Vec::new();
            };
            (0..ranges.GetLength())
                .filter_map(|i| Some(ranges.GetStart(i).ok()?..ranges.GetEnd(i).ok()?))
                .collect()
        }
    }
    fn volume(&self) -> f64 {
        self.engine
            .as_ref()
            .map_or(1., |e| unsafe { e.GetVolume() })
    }
    fn set_volume(&mut self, volume: f64) {
        if let Some(e) = &self.engine {
            unsafe {
                let _ = e.SetVolume(volume);
            }
        }
    }
    fn muted(&self) -> bool {
        self.engine
            .as_ref()
            .is_some_and(|e| unsafe { e.GetMuted().as_bool() })
    }
    fn set_muted(&mut self, muted: bool) {
        if let Some(e) = &self.engine {
            unsafe {
                let _ = e.SetMuted(muted);
            }
        }
    }
    fn playback_rate(&self) -> f64 {
        self.engine
            .as_ref()
            .map_or(1., |e| unsafe { e.GetPlaybackRate() })
    }
    fn set_playback_rate(&mut self, rate: f64) {
        if let Some(e) = &self.engine {
            unsafe {
                let _ = e.SetDefaultPlaybackRate(rate);
                let _ = e.SetPlaybackRate(rate);
            }
        }
    }
    fn error(&self) -> Option<MediaError> {
        if let Some(failure) = &self.failure {
            return Some(failure.clone());
        }
        let e = self.engine.as_ref()?;
        unsafe {
            let error = e.GetError().ok()?;
            let code = error.GetErrorCode();
            (code != 0).then(|| MediaError {
                code,
                // The extended code is the failure's HRESULT; its system message
                // stands in for the browser's.
                message: error
                    .GetExtendedErrorCode()
                    .err()
                    .map(|e| e.message())
                    .unwrap_or_default()
                    .into(),
            })
        }
    }
    fn video_size(&self) -> Option<Size<u32>> {
        let e = self.engine.as_ref()?;
        unsafe {
            if !e.HasVideo().as_bool() {
                return None;
            }
            let (mut w, mut h) = (0u32, 0u32);
            e.GetNativeVideoSize(Some(&mut w), Some(&mut h)).ok()?;
            (w > 0 && h > 0).then(|| size(w, h))
        }
    }
    fn take_events(&mut self) -> Vec<MediaEventKind> {
        let mut out = std::mem::take(&mut self.pending);
        if let Ok(mut queue) = self.queue.lock() {
            out.extend(queue.drain(..).filter_map(event_kind));
        }
        out
    }
    fn take_frame(&mut self) -> Option<MediaFrame> {
        self.frame()
    }
}

#[cfg(test)]
mod smoke {
    //! The engine against real media: a short tone this test writes, so no media
    //! file ships with the repository.

    use std::time::{Duration, Instant};

    use super::super::{MediaBackend, MediaEventKind, MediaKind, MediaSource};
    use super::MediaEngineBackend;

    /// A mono 16-bit PCM WAV file of `seconds` of a 440 Hz tone.
    fn tone(seconds: f32) -> Vec<u8> {
        let rate = 8000u32;
        let samples = (rate as f32 * seconds) as u32;
        let data_len = samples * 2;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..samples {
            let t = i as f32 / rate as f32;
            let v = ((t * 440. * std::f32::consts::TAU).sin() * 8000.) as i16;
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// Reads the backend until `done` holds, gathering its events.
    fn until(
        backend: &mut MediaEngineBackend,
        seen: &mut Vec<MediaEventKind>,
        done: impl Fn(&MediaEngineBackend, &[MediaEventKind]) -> bool,
    ) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(15) {
            seen.extend(backend.take_events());
            if done(backend, seen) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// CI's Windows runners are headless servers: no audio endpoint for Media Foundation's
    /// audio renderer and no GPU video decoding (frames come out black). The smoke tests
    /// need real media hardware, so they run on a desktop and are skipped on CI.
    fn on_ci() -> bool {
        if std::env::var_os("CI").is_some() {
            eprintln!("skipped on CI: needs an audio endpoint and GPU video decoding");
            return true;
        }
        false
    }

    #[test]
    fn media_foundation_loads_and_plays_a_wav_file() {
        if on_ci() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("astryx-media-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        std::fs::write(&path, tone(1.5)).unwrap();
        let mut backend = MediaEngineBackend::new();
        assert!(backend.engine.is_some(), "Media Foundation's engine starts");
        let source = MediaSource {
            kind: MediaKind::Audio,
            src: path.to_string_lossy().into_owned().into(),
            muted: true,
            ..Default::default()
        };
        backend.load(&source);
        let mut seen = Vec::new();
        assert!(
            until(&mut backend, &mut seen, |_, seen| seen
                .contains(&MediaEventKind::LoadedMetadata)),
            "metadata loads: {seen:?}"
        );
        assert!(seen.contains(&MediaEventKind::LoadStart));
        assert!(
            (backend.duration() - 1.5).abs() < 0.05,
            "duration {}",
            backend.duration()
        );
        assert!(backend.paused());
        assert!(backend.error().is_none());

        assert!(backend.play());
        assert!(
            until(&mut backend, &mut seen, |b, _| b.current_time() > 0.2),
            "playback advances: {seen:?}"
        );
        assert!(!backend.paused());
        assert!(seen.contains(&MediaEventKind::Play));
        backend.pause();
        assert!(until(&mut backend, &mut seen, |b, seen| b.paused()
            && seen.contains(&MediaEventKind::Pause)));

        backend.set_current_time(1.);
        assert!(until(&mut backend, &mut seen, |_, seen| seen
            .contains(&MediaEventKind::Seeked)));
        assert!(
            (backend.current_time() - 1.).abs() < 0.1,
            "seeked to {}",
            backend.current_time()
        );
        backend.set_playback_rate(2.);
        assert_eq!(backend.playback_rate(), 2.);
        backend.set_volume(0.25);
        assert!((backend.volume() - 0.25).abs() < 1e-6);
        assert!(backend.play());
        assert!(
            until(&mut backend, &mut seen, |b, seen| b.ended()
                && seen.contains(&MediaEventKind::Ended)),
            "it plays to the end: {seen:?}"
        );
        assert!(backend.video_size().is_none(), "a tone has no picture");
        assert!(backend.take_frame().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An uncompressed 32-bit AVI of `count` frames of `w` × `h`, each a flat color.
    fn clip(w: u32, h: u32, count: u32) -> Vec<u8> {
        fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(body.len() + 9);
            out.extend_from_slice(id);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(body);
            if body.len() % 2 == 1 {
                out.push(0);
            }
            out
        }
        fn list(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut inner = kind.to_vec();
            inner.extend_from_slice(body);
            chunk(b"LIST", &inner)
        }
        let le = |v: u32| v.to_le_bytes();
        let frame_size = w * h * 4;
        let micro_per_frame = 100_000u32; // 10 frames a second
        let mut avih = Vec::new();
        for v in [
            micro_per_frame,
            frame_size * 10,
            0,
            0x10,
            count,
            0,
            1,
            frame_size,
            w,
            h,
            0,
            0,
            0,
            0,
        ] {
            avih.extend_from_slice(&le(v));
        }
        let mut strh = Vec::new();
        strh.extend_from_slice(b"vids");
        strh.extend_from_slice(b"DIB ");
        for v in [0u32, 0, 0, 1, 10, 0, count, frame_size, u32::MAX, 0] {
            strh.extend_from_slice(&le(v));
        }
        strh.extend_from_slice(&[0u8; 8]); // rcFrame
        let mut strf = Vec::new();
        for v in [40u32, w, h] {
            strf.extend_from_slice(&le(v));
        }
        strf.extend_from_slice(&1u16.to_le_bytes());
        strf.extend_from_slice(&32u16.to_le_bytes());
        for v in [0u32, frame_size, 0, 0, 0, 0] {
            strf.extend_from_slice(&le(v));
        }
        let strl = list(
            b"strl",
            &[chunk(b"strh", &strh), chunk(b"strf", &strf)].concat(),
        );
        let hdrl = list(b"hdrl", &[chunk(b"avih", &avih), strl].concat());
        let mut movi_body = Vec::new();
        let mut index = Vec::new();
        for i in 0..count {
            let offset = movi_body.len() as u32 + 4;
            let shade = (i * 25 % 256) as u8;
            let pixels: Vec<u8> = (0..w * h)
                .flat_map(|_| [shade, 128, 255 - shade, 255])
                .collect();
            movi_body.extend_from_slice(&chunk(b"00db", &pixels));
            index.extend_from_slice(b"00db");
            index.extend_from_slice(&le(0x10));
            index.extend_from_slice(&le(offset));
            index.extend_from_slice(&le(frame_size));
        }
        let movi = list(b"movi", &movi_body);
        let body = [b"AVI ".to_vec(), hdrl, movi, chunk(b"idx1", &index)].concat();
        chunk(b"RIFF", &body)
    }

    #[test]
    fn frames_of_an_uncompressed_avi_reach_a_gpui_image() {
        if on_ci() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("astryx-media-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.avi");
        std::fs::write(&path, clip(32, 16, 10)).unwrap();
        let mut backend = MediaEngineBackend::new();
        backend.load(&MediaSource {
            kind: MediaKind::Video,
            src: path.to_string_lossy().into_owned().into(),
            muted: true,
            ..Default::default()
        });
        let mut seen = Vec::new();
        assert!(
            until(&mut backend, &mut seen, |_, seen| seen
                .contains(&MediaEventKind::LoadedMetadata)
                || seen.contains(&MediaEventKind::Error)),
            "{seen:?}"
        );
        assert!(
            backend.error().is_none(),
            "the clip loads: {:?}",
            backend.error()
        );
        assert_eq!(backend.video_size(), Some(gpui::size(32, 16)));
        assert!(backend.play());
        let start = Instant::now();
        let mut frame = None;
        while frame.is_none() && start.elapsed() < Duration::from_secs(10) {
            seen.extend(backend.take_events());
            frame = backend.take_frame();
            std::thread::sleep(Duration::from_millis(15));
        }
        let Some(super::super::MediaFrame::Image(image)) = frame else {
            panic!("a frame arrives: {seen:?}")
        };
        assert_eq!(
            image.size(0),
            gpui::size(gpui::DevicePixels(32), gpui::DevicePixels(16))
        );
        // The first frames are orange (red 255, green 128, blue near 0): GPUI images
        // hold BGRA.
        let bytes = image.as_bytes(0).expect("pixels");
        let (b, g, r, a) = (bytes[0], bytes[1], bytes[2], bytes[3]);
        assert!(
            r > 200 && (90..170).contains(&g) && b < 90 && a == 255,
            "bgra {b} {g} {r} {a}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn media_foundation_reports_a_source_it_cannot_play() {
        let dir = std::env::temp_dir().join(format!("astryx-media-broken-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("broken.wav");
        std::fs::write(&path, b"RIFF0000WAVEnot really a wav file").unwrap();
        let mut backend = MediaEngineBackend::new();
        backend.load(&MediaSource {
            kind: MediaKind::Audio,
            src: path.to_string_lossy().into_owned().into(),
            ..Default::default()
        });
        let mut seen = Vec::new();
        assert!(
            until(&mut backend, &mut seen, |_, seen| seen
                .contains(&MediaEventKind::Error)),
            "{seen:?}"
        );
        let error = backend.error().expect("a media error");
        assert!((1..=4).contains(&error.code), "code {}", error.code);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
