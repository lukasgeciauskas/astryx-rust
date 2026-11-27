//! macOS: AVFoundation's `AVPlayer` plays the media, and an
//! `AVPlayerItemVideoOutput` hands over each new video frame as a `CVPixelBuffer`
//! that GPUI draws as a surface. AVFoundation reports no media element events, so
//! the backend reads the player each poll and fires the events a browser would.
//!
//! The classes are looked up at run time and messaged through `objc2::msg_send!`.

use std::ops::Range;
use std::ptr::NonNull;

use gpui::{Size, size};
use objc2::encode::{Encode, Encoding, RefEncode};
use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVPixelBuffer, kCVPixelBufferMetalCompatibilityKey, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
};
use objc2_foundation::NSString;
use web_time::{Duration, Instant};

use super::{MediaBackend, MediaError, MediaEventKind, MediaFrame, MediaSource};

// AVPlayer and its classes register when the framework loads.
#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {}

/// CoreMedia's `CMTime`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

// SAFETY: the layout and encoding of CoreMedia's anonymous `CMTime` struct.
unsafe impl Encode for CMTime {
    const ENCODING: Encoding = Encoding::Struct(
        "?",
        &[i64::ENCODING, i32::ENCODING, u32::ENCODING, i64::ENCODING],
    );
}

// SAFETY: a pointer to a CMTime, as `itemTimeForDisplay:` takes.
unsafe impl RefEncode for CMTime {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
}

/// CoreMedia's `CMTimeRange`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CMTimeRange {
    start: CMTime,
    duration: CMTime,
}

// SAFETY: the layout and encoding of CoreMedia's anonymous `CMTimeRange` struct.
unsafe impl Encode for CMTimeRange {
    const ENCODING: Encoding = Encoding::Struct("?", &[CMTime::ENCODING, CMTime::ENCODING]);
}

/// CoreGraphics' `CGSize` (64-bit `CGFloat`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CGSize {
    width: f64,
    height: f64,
}

// SAFETY: the layout and encoding of `CGSize` on 64-bit macOS.
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}

const VALID: u32 = 1;
const INDEFINITE: u32 = 1 << 4;

impl CMTime {
    fn seconds(self) -> f64 {
        if self.flags & VALID == 0 {
            f64::NAN
        } else if self.flags & INDEFINITE != 0 {
            f64::INFINITY
        } else if self.timescale == 0 {
            f64::NAN
        } else {
            self.value as f64 / f64::from(self.timescale)
        }
    }

    fn from_seconds(seconds: f64) -> Self {
        const SCALE: i32 = 600;
        CMTime {
            value: (seconds.max(0.) * f64::from(SCALE)).round() as i64,
            timescale: SCALE,
            flags: VALID,
            epoch: 0,
        }
    }
}

/// `AVPlayerItemStatus`.
const READY: isize = 1;
const FAILED: isize = 2;
/// `AVPlayerTimeControlStatus`.
const WAITING: isize = 1;
const PLAYING: isize = 2;

/// How often playback reports its time, as a browser's `timeupdate` does.
const TIME_UPDATE: Duration = Duration::from_millis(250);

fn class(name: &std::ffi::CStr) -> Option<&'static AnyClass> {
    AnyClass::get(name)
}

/// A source as an `NSURL`: a URL, or a local path.
fn url(src: &str) -> Option<Retained<AnyObject>> {
    let string = NSString::from_str(src);
    let nsurl = class(c"NSURL")?;
    // SAFETY: `+URLWithString:` and `+fileURLWithPath:` take an NSString and
    // return an autoreleased NSURL or nil.
    unsafe {
        if src.contains("://") || src.starts_with("data:") {
            msg_send![nsurl, URLWithString: &*string]
        } else {
            msg_send![nsurl, fileURLWithPath: &*string]
        }
    }
}

/// The video output's attributes: frames in the biplanar 4:2:0 format GPUI's
/// surfaces draw, Metal-compatible.
fn output_attributes() -> Option<Retained<AnyObject>> {
    let number = class(c"NSNumber")?;
    let dictionary = class(c"NSDictionary")?;
    // SAFETY: NSNumber's factories return autoreleased numbers; the CoreVideo keys
    // are CFStrings, toll-free bridged with NSString; `+dictionaryWithObjects:
    // forKeys:count:` copies two objects and two keys from the arrays.
    unsafe {
        let format: Retained<AnyObject> = msg_send![number, numberWithUnsignedInt: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange];
        let metal: Retained<AnyObject> = msg_send![number, numberWithBool: true];
        let keys: [*const AnyObject; 2] = [
            (kCVPixelBufferPixelFormatTypeKey as *const _ as *const AnyObject),
            (kCVPixelBufferMetalCompatibilityKey as *const _ as *const AnyObject),
        ];
        let objects: [*const AnyObject; 2] = [Retained::as_ptr(&format), Retained::as_ptr(&metal)];
        msg_send![dictionary, dictionaryWithObjects: objects.as_ptr(), forKeys: keys.as_ptr(), count: 2usize]
    }
}

/// The backend on macOS.
pub struct AvPlayerBackend {
    player: Option<Retained<AnyObject>>,
    item: Option<Retained<AnyObject>>,
    output: Option<Retained<AnyObject>>,
    source: Option<MediaSource>,
    paused: bool,
    ended: bool,
    rate: f64,
    events: Vec<MediaEventKind>,
    status: isize,
    control: isize,
    duration: f64,
    time: f64,
    reported_at: Option<Instant>,
    buffered_end: f64,
}

impl AvPlayerBackend {
    pub fn new() -> Self {
        AvPlayerBackend {
            player: None,
            item: None,
            output: None,
            source: None,
            paused: true,
            ended: false,
            rate: 1.,
            events: Vec::new(),
            status: 0,
            control: 0,
            duration: f64::NAN,
            time: 0.,
            reported_at: None,
            buffered_end: 0.,
        }
    }

    fn player(&self) -> Option<&AnyObject> {
        self.player.as_deref()
    }

    fn item(&self) -> Option<&AnyObject> {
        self.item.as_deref()
    }

    fn start(&mut self) {
        if let Some(player) = self.player() {
            // SAFETY: `-setRate:` takes a float; a rate above 0 plays.
            unsafe {
                let _: () = msg_send![player, setRate: self.rate as f32];
            }
        }
    }

    fn seek_to(&self, time: f64) {
        if let Some(player) = self.player() {
            let zero = CMTime::from_seconds(0.);
            // SAFETY: `-seekToTime:toleranceBefore:toleranceAfter:` takes three CMTimes;
            // zero tolerance seeks to the exact time.
            unsafe {
                let _: () = msg_send![player, seekToTime: CMTime::from_seconds(time), toleranceBefore: zero, toleranceAfter: zero];
            }
        }
    }

    fn read_time(&self) -> f64 {
        let Some(player) = self.player() else {
            return 0.;
        };
        // SAFETY: `-currentTime` returns a CMTime.
        let time: CMTime = unsafe { msg_send![player, currentTime] };
        let seconds = time.seconds();
        if seconds.is_finite() { seconds } else { 0. }
    }

    fn read_duration(&self) -> f64 {
        let Some(item) = self.item() else {
            return f64::NAN;
        };
        if self.status != READY {
            return f64::NAN;
        }
        // SAFETY: `-duration` returns a CMTime; indefinite for a live stream.
        let duration: CMTime = unsafe { msg_send![item, duration] };
        duration.seconds()
    }

    /// Reads the player and fires the events of what changed since the last read.
    fn sync(&mut self) {
        let (Some(player), Some(item)) = (self.player.clone(), self.item.clone()) else {
            return;
        };
        // SAFETY: `-status` and `-timeControlStatus` return NSIntegers.
        let (status, control): (isize, isize) = unsafe {
            (
                msg_send![&*item, status],
                msg_send![&*player, timeControlStatus],
            )
        };
        if status != self.status {
            self.status = status;
            match status {
                READY => {
                    self.duration = self.read_duration();
                    self.events.extend([
                        MediaEventKind::LoadedMetadata,
                        MediaEventKind::DurationChange,
                        MediaEventKind::CanPlay,
                    ]);
                    if self.source.as_ref().is_some_and(|s| s.autoplay) && self.paused {
                        self.play();
                    }
                }
                FAILED => self.events.push(MediaEventKind::Error),
                _ => {}
            }
        }
        if status != READY {
            return;
        }
        let duration = self.read_duration();
        if duration.to_bits() != self.duration.to_bits() {
            self.duration = duration;
            self.events.push(MediaEventKind::DurationChange);
        }
        if !self.paused && control != self.control {
            match control {
                WAITING => self.events.push(MediaEventKind::Waiting),
                PLAYING => self.events.push(MediaEventKind::Playing),
                _ => {}
            }
        }
        self.control = control;
        let time = self.read_time();
        let now = Instant::now();
        if time != self.time
            && self
                .reported_at
                .is_none_or(|at| now.duration_since(at) >= TIME_UPDATE)
        {
            self.time = time;
            self.reported_at = Some(now);
            self.events.push(MediaEventKind::TimeUpdate);
        }
        let buffered_end = self
            .buffered()
            .iter()
            .fold(0., |end: f64, r| end.max(r.end));
        if buffered_end != self.buffered_end {
            self.buffered_end = buffered_end;
            self.events.push(MediaEventKind::Progress);
        }
        // The end: AVPlayer stops by itself at the end of the item.
        if !self.paused && duration.is_finite() && time >= duration - 0.05 && control == 0 {
            if self.source.as_ref().is_some_and(|s| s.looping) {
                self.seek_to(0.);
                self.start();
            } else {
                self.paused = true;
                self.ended = true;
                self.time = time;
                self.events.extend([
                    MediaEventKind::TimeUpdate,
                    MediaEventKind::Pause,
                    MediaEventKind::Ended,
                ]);
            }
        }
    }
}

impl Default for AvPlayerBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AvPlayerBackend {
    fn drop(&mut self) {
        if let Some(player) = self.player() {
            // SAFETY: `-pause` takes nothing.
            unsafe {
                let _: () = msg_send![player, pause];
            }
        }
    }
}

impl MediaBackend for AvPlayerBackend {
    fn load(&mut self, source: &MediaSource) {
        self.paused = true;
        self.ended = false;
        self.status = 0;
        self.control = 0;
        self.duration = f64::NAN;
        self.time = 0.;
        self.reported_at = None;
        self.buffered_end = 0.;
        self.source = Some(source.clone());
        self.events
            .extend([MediaEventKind::Emptied, MediaEventKind::LoadStart]);
        let made = (|| {
            let url = url(&source.src)?;
            // SAFETY: `+playerItemWithURL:` takes an NSURL and returns an item;
            // `-initWithPixelBufferAttributes:` takes a dictionary; `-addOutput:`
            // attaches the output to the item.
            unsafe {
                let item: Retained<AnyObject> =
                    msg_send![class(c"AVPlayerItem")?, playerItemWithURL: &*url];
                let attributes = output_attributes()?;
                let allocated: Allocated<AnyObject> =
                    msg_send![class(c"AVPlayerItemVideoOutput")?, alloc];
                let output: Retained<AnyObject> =
                    msg_send![allocated, initWithPixelBufferAttributes: &*attributes];
                let _: () = msg_send![&*item, addOutput: &*output];
                Some((item, output))
            }
        })();
        let Some((item, output)) = made else {
            self.item = None;
            self.output = None;
            self.events.push(MediaEventKind::Error);
            return;
        };
        match self.player.clone() {
            // SAFETY: `-replaceCurrentItemWithPlayerItem:` takes an item.
            Some(player) => unsafe {
                let _: () = msg_send![&*player, replaceCurrentItemWithPlayerItem: &*item];
            },
            None => {
                // SAFETY: `+playerWithPlayerItem:` takes an item and returns a player.
                let player: Option<Retained<AnyObject>> = class(c"AVPlayer")
                    .and_then(|c| unsafe { msg_send![c, playerWithPlayerItem: &*item] });
                self.player = player;
            }
        }
        if let Some(player) = self.player() {
            // SAFETY: `-setMuted:` takes a BOOL.
            unsafe {
                let _: () = msg_send![player, setMuted: source.muted];
            }
        }
        self.item = Some(item);
        self.output = Some(output);
    }
    fn play(&mut self) -> bool {
        if self.player.is_none() {
            return false;
        }
        if self.paused {
            if self.ended {
                self.seek_to(0.);
                self.ended = false;
            }
            self.paused = false;
            self.events.push(MediaEventKind::Play);
        }
        self.start();
        true
    }
    fn pause(&mut self) {
        if let Some(player) = self.player() {
            // SAFETY: `-pause` takes nothing.
            unsafe {
                let _: () = msg_send![player, pause];
            }
        }
        if !self.paused {
            self.paused = true;
            self.events.push(MediaEventKind::Pause);
        }
    }
    fn paused(&self) -> bool {
        self.paused
    }
    fn ended(&self) -> bool {
        self.ended
    }
    fn current_time(&self) -> f64 {
        self.read_time()
    }
    fn set_current_time(&mut self, time: f64) {
        self.seek_to(time);
        self.ended = false;
        self.time = time;
        self.events.extend([
            MediaEventKind::Seeking,
            MediaEventKind::Seeked,
            MediaEventKind::TimeUpdate,
        ]);
    }
    fn duration(&self) -> f64 {
        self.read_duration()
    }
    fn buffered(&self) -> Vec<Range<f64>> {
        let Some(item) = self.item() else {
            return Vec::new();
        };
        // SAFETY: `-loadedTimeRanges` returns an NSArray of NSValues holding
        // CMTimeRanges.
        unsafe {
            let ranges: Option<Retained<AnyObject>> = msg_send![item, loadedTimeRanges];
            let Some(ranges) = ranges else {
                return Vec::new();
            };
            let count: usize = msg_send![&*ranges, count];
            (0..count)
                .filter_map(|i| {
                    let value: Option<Retained<AnyObject>> = msg_send![&*ranges, objectAtIndex: i];
                    let range: CMTimeRange = msg_send![&*value?, CMTimeRangeValue];
                    let start = range.start.seconds();
                    let end = start + range.duration.seconds();
                    (start.is_finite() && end.is_finite()).then_some(start..end)
                })
                .collect()
        }
    }
    fn volume(&self) -> f64 {
        // SAFETY: `-volume` returns a float.
        self.player().map_or(1., |p| {
            let volume: f32 = unsafe { msg_send![p, volume] };
            f64::from(volume)
        })
    }
    fn set_volume(&mut self, volume: f64) {
        if let Some(player) = self.player() {
            // SAFETY: `-setVolume:` takes a float.
            unsafe {
                let _: () = msg_send![player, setVolume: volume as f32];
            }
            self.events.push(MediaEventKind::VolumeChange);
        }
    }
    fn muted(&self) -> bool {
        // SAFETY: `-isMuted` returns a BOOL.
        self.player()
            .is_some_and(|p| unsafe { msg_send![p, isMuted] })
    }
    fn set_muted(&mut self, muted: bool) {
        if let Some(player) = self.player() {
            // SAFETY: `-setMuted:` takes a BOOL.
            unsafe {
                let _: () = msg_send![player, setMuted: muted];
            }
            self.events.push(MediaEventKind::VolumeChange);
        }
    }
    fn playback_rate(&self) -> f64 {
        self.rate
    }
    fn set_playback_rate(&mut self, rate: f64) {
        if rate != self.rate {
            self.rate = rate;
            if !self.paused {
                self.start();
            }
            self.events.push(MediaEventKind::RateChange);
        }
    }
    fn error(&self) -> Option<MediaError> {
        let item = self.item();
        if item.is_none() && self.source.is_some() {
            return Some(MediaError::unsupported("The source could not be opened."));
        }
        if self.status != FAILED {
            return None;
        }
        // SAFETY: `-error` returns an NSError or nil; `-localizedDescription`
        // returns an NSString.
        let message = unsafe {
            let error: Option<Retained<AnyObject>> = msg_send![item?, error];
            error.and_then(|e| {
                let text: Option<Retained<NSString>> = msg_send![&*e, localizedDescription];
                text.map(|t| t.to_string())
            })
        };
        Some(MediaError {
            code: 4,
            message: message.unwrap_or_default().into(),
        })
    }
    fn video_size(&self) -> Option<Size<u32>> {
        let item = self.item()?;
        // SAFETY: `-presentationSize` returns a CGSize.
        let s: CGSize = unsafe { msg_send![item, presentationSize] };
        (s.width >= 1. && s.height >= 1.)
            .then(|| size(s.width.round() as u32, s.height.round() as u32))
    }
    fn take_events(&mut self) -> Vec<MediaEventKind> {
        self.sync();
        std::mem::take(&mut self.events)
    }
    fn take_frame(&mut self) -> Option<MediaFrame> {
        let (player, output) = (self.player.clone()?, self.output.clone()?);
        // SAFETY: `-currentTime` returns a CMTime; `-hasNewPixelBufferForItemTime:`
        // takes one and returns a BOOL; `-copyPixelBufferForItemTime:
        // itemTimeForDisplay:` returns a +1 CVPixelBuffer (or NULL), which
        // `CFRetained::from_raw` takes over.
        unsafe {
            let time: CMTime = msg_send![&*player, currentTime];
            let fresh: bool = msg_send![&*output, hasNewPixelBufferForItemTime: time];
            if !fresh {
                return None;
            }
            let buffer: *mut CVPixelBuffer = msg_send![&*output, copyPixelBufferForItemTime: time, itemTimeForDisplay: std::ptr::null_mut::<CMTime>()];
            NonNull::new(buffer).map(|buffer| MediaFrame::PixelBuffer(CFRetained::from_raw(buffer)))
        }
    }
}
