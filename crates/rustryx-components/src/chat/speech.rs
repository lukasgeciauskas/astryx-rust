//! useSpeechRecognition, upstream `Chat/useSpeechRecognition.ts`, over a pluggable
//! [`SpeechRecognitionProvider`]: upstream wraps the browser's Web Speech API and reads the
//! microphone through Web Audio; GPUI has neither, so an app installs a provider (the
//! platform's recognizer, a cloud service, a test double) and this hook keeps upstream's
//! session state, transcript handling and volume analysis.

#[cfg(test)]
use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, ElementId, Entity, Global, SharedString, Window};

/// One recognition result: a transcript and whether it is final.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeechResult {
    pub transcript: String,
    pub is_final: bool,
}

/// What a provider reports during a session (the Web Speech API's events), plus the
/// microphone's frequency data for the volume meter.
#[derive(Clone, Debug, PartialEq)]
pub enum SpeechEvent {
    Start,
    End,
    SpeechStart,
    SpeechEnd,
    /// The session's results from `result_index` on (upstream's `onresult` event).
    Result {
        result_index: usize,
        results: Vec<SpeechResult>,
    },
    Error {
        error: String,
        message: Option<String>,
    },
    NoMatch,
    /// One frame of byte frequency data, 0 to 255 a bin (Web Audio's
    /// `getByteFrequencyData` with an FFT of 256: 128 bins).
    AudioFrame(Vec<u8>),
}

/// Where a provider sends its events. Cheap to clone; safe to call after the hook is
/// gone.
#[derive(Clone)]
pub struct SpeechSink(Rc<SinkFn>);

type SinkFn = dyn Fn(SpeechEvent, &mut App);

impl SpeechSink {
    pub fn send(&self, event: SpeechEvent, cx: &mut App) {
        (self.0)(event, cx)
    }
}

/// A session's settings (upstream's recognition properties).
#[derive(Clone, Debug, PartialEq)]
pub struct SpeechRequest {
    /// A BCP 47 locale; the locale in effect by default.
    pub lang: SharedString,
    pub continuous: bool,
    pub interim_results: bool,
}

/// A speech recognizer, installed with [`set_speech_recognition_provider`].
pub trait SpeechRecognitionProvider {
    /// Upstream `isSupported`: whether it can recognize speech here.
    fn is_supported(&self) -> bool;
    /// Starts a session, replacing any running one; report through `sink`.
    fn start(&self, request: SpeechRequest, sink: SpeechSink, cx: &mut App);
    /// Ends the session after the speech so far (a final result may follow, then `End`).
    fn stop(&self, cx: &mut App);
    /// Ends the session at once.
    fn abort(&self, cx: &mut App);
    /// Plays mono samples, for dictation's start and stop sounds; by default, silence.
    fn play_samples(&self, _samples: &[f32], _sample_rate: u32, _cx: &mut App) {}
}

struct GlobalProvider(Rc<dyn SpeechRecognitionProvider>);

impl Global for GlobalProvider {}

/// Installs the app's speech recognizer. Without one, speech is unsupported and the
/// dictation button hides.
pub fn set_speech_recognition_provider(provider: Rc<dyn SpeechRecognitionProvider>, cx: &mut App) {
    cx.set_global(GlobalProvider(provider));
}

/// The installed provider; on the web, the browser's own when none is installed.
pub(crate) fn provider(cx: &App) -> Option<Rc<dyn SpeechRecognitionProvider>> {
    if let Some(global) = cx.try_global::<GlobalProvider>() {
        return Some(global.0.clone());
    }
    #[cfg(target_family = "wasm")]
    {
        thread_local! {
            static WEB: Rc<super::speech_web::WebSpeechProvider> = Rc::default();
        }
        return Some(WEB.with(|web| web.clone() as Rc<dyn SpeechRecognitionProvider>));
    }
    #[allow(unreachable_code)]
    None
}

/// Upstream's analyser constants: noise calibration over the first 60 frames, voice
/// bands split at these bins.
const CALIBRATION_FRAMES: usize = 60;
const VOICE_SPLITS: [usize; 4] = [3, 6, 11, 18];

/// Upstream `createVolumeAnalyser` over the provider's frames: a noise floor from the
/// first frames, then levels above it.
#[derive(Default)]
pub(crate) struct VolumeAnalyser {
    data: Vec<u8>,
    noise_floor: Vec<f32>,
    calibration_samples: usize,
}

impl VolumeAnalyser {
    pub fn frame(&mut self, frame: Vec<u8>) {
        if self.noise_floor.len() != frame.len() {
            self.noise_floor = vec![0.; frame.len()];
        }
        self.data = frame;
    }

    fn calibrate(&mut self) {
        self.calibration_samples += 1;
        for (i, &value) in self.data.iter().enumerate() {
            let value = value as f32 / 255.;
            if value > self.noise_floor[i] {
                self.noise_floor[i] = value;
            }
        }
    }

    fn clean_bin(&self, i: usize) -> f32 {
        if self.calibration_samples < CALIBRATION_FRAMES {
            return 0.;
        }
        (self.data[i] as f32 / 255. - self.noise_floor[i] * 1.1).max(0.)
    }

    /// Upstream `getVolume`: the mean level above the noise floor.
    pub fn volume(&mut self) -> f32 {
        if self.data.is_empty() {
            return 0.;
        }
        if self.calibration_samples < CALIBRATION_FRAMES {
            self.calibrate();
        }
        (0..self.data.len()).map(|i| self.clean_bin(i)).sum::<f32>() / self.data.len() as f32
    }

    fn splits(&self, count: usize) -> Vec<usize> {
        let mut splits: Vec<usize> = VOICE_SPLITS.to_vec();
        splits.push(self.data.len());
        if count <= splits.len() {
            splits.truncate(count);
        }
        splits
    }

    /// Upstream `getBands`: voice bands above the noise floor.
    pub fn bands(&mut self, count: usize) -> Vec<f32> {
        if self.data.is_empty() {
            return vec![0.; count];
        }
        if self.calibration_samples < CALIBRATION_FRAMES {
            self.calibrate();
        }
        let mut start = 1;
        self.splits(count)
            .into_iter()
            .map(|end| {
                let end = end.min(self.data.len());
                let sum: f32 = (start..end).map(|i| self.clean_bin(i)).sum();
                let band = sum / (end.saturating_sub(start)).max(1) as f32;
                start = end;
                band
            })
            .collect()
    }

    /// Upstream `getRawBands`: voice bands, noise included.
    pub fn raw_bands(&self, count: usize) -> Vec<f32> {
        if self.data.is_empty() {
            return vec![0.; count];
        }
        let mut start = 1;
        self.splits(count)
            .into_iter()
            .map(|end| {
                let end = end.min(self.data.len());
                let sum: f32 = (start..end).map(|i| self.data[i] as f32 / 255.).sum();
                let band = sum / (end.saturating_sub(start)).max(1) as f32;
                start = end;
                band
            })
            .collect()
    }
}

type TranscriptFn = Rc<dyn Fn(&str, bool, &mut App)>;
type TextFn = Rc<dyn Fn(&str, &mut App)>;
type ErrorFn = Rc<dyn Fn(&str, Option<&str>, &mut App)>;
type EventFn = Rc<dyn Fn(&mut App)>;
type TransformFn = Rc<dyn Fn(&str) -> String>;

/// Upstream `UseSpeechRecognitionOptions` (less `audioContext`: the provider owns audio).
#[derive(Clone)]
pub struct UseSpeechRecognitionOptions {
    /// Upstream `lang`; the locale in effect by default.
    pub lang: Option<SharedString>,
    pub continuous: bool,
    pub interim_results: bool,
    pub transform_transcript: Option<TransformFn>,
    pub on_transcript: Option<TranscriptFn>,
    pub on_result: Option<TextFn>,
    pub on_error: Option<ErrorFn>,
    pub on_start: Option<EventFn>,
    pub on_end: Option<EventFn>,
}

impl Default for UseSpeechRecognitionOptions {
    fn default() -> Self {
        UseSpeechRecognitionOptions {
            lang: None,
            continuous: true,
            interim_results: true,
            transform_transcript: None,
            on_transcript: None,
            on_result: None,
            on_error: None,
            on_start: None,
            on_end: None,
        }
    }
}

impl UseSpeechRecognitionOptions {
    /// Upstream `onTranscript`: interim and final transcripts as they come.
    pub fn on_transcript(mut self, f: impl Fn(&str, bool, &mut App) + 'static) -> Self {
        self.on_transcript = Some(Rc::new(f));
        self
    }

    /// Upstream `onResult`: each final transcript.
    pub fn on_result(mut self, f: impl Fn(&str, &mut App) + 'static) -> Self {
        self.on_result = Some(Rc::new(f));
        self
    }

    /// Upstream `onError`: the error code and its message.
    pub fn on_error(mut self, f: impl Fn(&str, Option<&str>, &mut App) + 'static) -> Self {
        self.on_error = Some(Rc::new(f));
        self
    }

    /// Upstream `onStart`.
    pub fn on_start(mut self, f: impl Fn(&mut App) + 'static) -> Self {
        self.on_start = Some(Rc::new(f));
        self
    }

    /// Upstream `onEnd`.
    pub fn on_end(mut self, f: impl Fn(&mut App) + 'static) -> Self {
        self.on_end = Some(Rc::new(f));
        self
    }

    /// Upstream `transformTranscript`: rewrites each transcript before it is reported.
    pub fn transform_transcript(mut self, f: impl Fn(&str) -> String + 'static) -> Self {
        self.transform_transcript = Some(Rc::new(f));
        self
    }
}

pub(crate) struct SpeechState {
    pub listening: bool,
    pub speaking: bool,
    pub interim: SharedString,
    pub volume: f32,
    pub bands: Vec<f32>,
    pub raw_bands: Vec<f32>,
    pub analyser: Option<VolumeAnalyser>,
    /// The latest render's options, which events read.
    pub options: UseSpeechRecognitionOptions,
    /// Each session's number, so a replaced session's late events are ignored.
    pub session: u64,
}

/// Upstream `UseSpeechRecognitionReturn`: the session's state and its controls.
#[derive(Clone)]
pub struct SpeechRecognition {
    pub(crate) state: Entity<SpeechState>,
    pub(crate) supported: bool,
    lang: SharedString,
}

/// Upstream `useSpeechRecognition`.
///
/// ```ignore
/// let speech = use_speech_recognition("dictate", UseSpeechRecognitionOptions::default()
///     .on_result(|text, _| println!("{text}")), window, cx);
/// ChatDictationButton::new("mic", speech)
/// ```
pub fn use_speech_recognition(
    id: impl Into<ElementId>,
    options: UseSpeechRecognitionOptions,
    window: &mut Window,
    cx: &mut App,
) -> SpeechRecognition {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-speech").into()),
        cx,
        |_, _| SpeechState {
            listening: false,
            speaking: false,
            interim: SharedString::default(),
            volume: 0.,
            bands: vec![0.; 5],
            raw_bands: vec![0.; 5],
            analyser: None,
            options: UseSpeechRecognitionOptions::default(),
            session: 0,
        },
    );
    let lang = options
        .lang
        .clone()
        .unwrap_or_else(|| rustryx_i18n::use_locale(cx));
    // The volume meter reads the analyser once a frame while listening.
    let polling = state.update(cx, |s, _| {
        s.options = options;
        if let Some(analyser) = s.analyser.as_mut() {
            s.volume = analyser.volume();
            s.bands = analyser.bands(5);
            s.raw_bands = analyser.raw_bands(5);
        }
        s.listening && s.analyser.is_some()
    });
    if polling {
        window.request_animation_frame();
    }
    let supported = provider(cx).is_some_and(|p| p.is_supported());
    SpeechRecognition {
        state,
        supported,
        lang,
    }
}

/// Handles one provider event (upstream's recognition handlers).
fn handle(state: &Entity<SpeechState>, session: u64, event: SpeechEvent, cx: &mut App) {
    let current = state.read(cx).session;
    if current != session {
        return;
    }
    let options = state.read(cx).options.clone();
    match event {
        SpeechEvent::Start => {
            state.update(cx, |s, _| {
                s.listening = true;
                s.analyser = Some(VolumeAnalyser::default());
            });
            if let Some(f) = &options.on_start {
                f(cx);
            }
        }
        SpeechEvent::End => {
            state.update(cx, |s, _| {
                s.listening = false;
                s.speaking = false;
                s.interim = SharedString::default();
                s.analyser = None;
                s.volume = 0.;
                s.bands = vec![0.; 5];
                s.raw_bands = vec![0.; 5];
            });
            if let Some(f) = &options.on_end {
                f(cx);
            }
        }
        SpeechEvent::SpeechStart => state.update(cx, |s, _| s.speaking = true),
        SpeechEvent::SpeechEnd => state.update(cx, |s, _| s.speaking = false),
        SpeechEvent::Result {
            result_index: _,
            results,
        } => {
            let mut interim = String::new();
            for result in results {
                let transcript = match &options.transform_transcript {
                    Some(f) => f(&result.transcript),
                    None => result.transcript,
                };
                if result.is_final {
                    if let Some(f) = &options.on_result {
                        f(&transcript, cx);
                    }
                    if let Some(f) = &options.on_transcript {
                        f(&transcript, true, cx);
                    }
                    state.update(cx, |s, _| s.interim = SharedString::default());
                } else {
                    interim.push_str(&transcript);
                }
            }
            if !interim.is_empty() {
                state.update(cx, |s, _| s.interim = interim.clone().into());
                if let Some(f) = &options.on_transcript {
                    f(&interim, false, cx);
                }
            }
        }
        SpeechEvent::Error { error, message } => {
            if let Some(f) = &options.on_error {
                f(&error, message.as_deref(), cx);
            }
        }
        SpeechEvent::NoMatch => {
            let message =
                rustryx_i18n::current(cx).t("@astryx.chat.speechRecognition.noSpeechDetected");
            if let Some(f) = &options.on_error {
                f("no-speech", Some(&message), cx);
            }
        }
        SpeechEvent::AudioFrame(frame) => state.update(cx, |s, _| {
            if let Some(analyser) = s.analyser.as_mut() {
                analyser.frame(frame);
            }
        }),
    }
    cx.refresh_windows();
}

impl SpeechRecognition {
    /// Upstream `isSupported`.
    pub fn is_supported(&self) -> bool {
        self.supported
    }

    /// Upstream `isListening`.
    pub fn is_listening(&self, cx: &App) -> bool {
        self.state.read(cx).listening
    }

    /// Upstream `isSpeaking`.
    pub fn is_speaking(&self, cx: &App) -> bool {
        self.state.read(cx).speaking
    }

    /// Upstream `volume`: the level above the noise floor, 0 to 1.
    pub fn volume(&self, cx: &App) -> f32 {
        self.state.read(cx).volume
    }

    /// Upstream `bands`: five voice bands above the noise floor.
    pub fn bands(&self, cx: &App) -> Vec<f32> {
        self.state.read(cx).bands.clone()
    }

    /// Upstream `rawBands`: the five bands, noise included.
    pub fn raw_bands(&self, cx: &App) -> Vec<f32> {
        self.state.read(cx).raw_bands.clone()
    }

    /// Upstream `interimTranscript`.
    pub fn interim_transcript(&self, cx: &App) -> SharedString {
        self.state.read(cx).interim.clone()
    }

    /// Upstream `start`: a new session, replacing any running one.
    pub fn start(&self, cx: &mut App) {
        let Some(provider) = provider(cx).filter(|p| p.is_supported()) else {
            return;
        };
        provider.abort(cx);
        let (options, session) = self.state.update(cx, |s, _| {
            s.session += 1;
            (s.options.clone(), s.session)
        });
        let state = self.state.downgrade();
        let sink = SpeechSink(Rc::new(move |event, cx| {
            if let Some(state) = state.upgrade() {
                handle(&state, session, event, cx);
            }
        }));
        let request = SpeechRequest {
            lang: options.lang.clone().unwrap_or_else(|| self.lang.clone()),
            continuous: options.continuous,
            interim_results: options.interim_results,
        };
        provider.start(request, sink, cx);
    }

    /// Upstream `stop`.
    pub fn stop(&self, cx: &mut App) {
        if let Some(provider) = provider(cx) {
            provider.stop(cx);
        }
    }

    /// Upstream `abort`: ends the session and the volume meter at once.
    pub fn abort(&self, cx: &mut App) {
        if let Some(provider) = provider(cx) {
            provider.abort(cx);
        }
        self.state.update(cx, |s, _| {
            s.analyser = None;
            s.volume = 0.;
            s.bands = vec![0.; 5];
            s.raw_bands = vec![0.; 5];
        });
    }

    /// Upstream `toggle`.
    pub fn toggle(&self, cx: &mut App) {
        if self.is_listening(cx) {
            self.stop(cx);
        } else {
            self.start(cx);
        }
    }
}

/// A provider that records what it was asked and reports what a test sends, for tests.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct FakeProvider {
    pub sink: RefCell<Option<SpeechSink>>,
    pub calls: RefCell<Vec<String>>,
    pub played: RefCell<usize>,
}

#[cfg(test)]
impl SpeechRecognitionProvider for FakeProvider {
    fn is_supported(&self) -> bool {
        true
    }
    fn start(&self, request: SpeechRequest, sink: SpeechSink, _: &mut App) {
        self.calls
            .borrow_mut()
            .push(format!("start {}", request.lang));
        *self.sink.borrow_mut() = Some(sink);
    }
    fn stop(&self, _: &mut App) {
        self.calls.borrow_mut().push("stop".into());
    }
    fn abort(&self, _: &mut App) {
        self.calls.borrow_mut().push("abort".into());
    }
    fn play_samples(&self, _: &[f32], _: u32, _: &mut App) {
        *self.played.borrow_mut() += 1;
    }
}
