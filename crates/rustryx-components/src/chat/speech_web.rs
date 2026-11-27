//! The browser's speech provider, on the web build: upstream's own Web Speech API
//! (`SpeechRecognition`, or `webkitSpeechRecognition`) and Web Audio for the volume meter
//! and the dictation sounds, so dictation works in a browser without an app provider.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use gpui::App;
use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use super::speech::{
    SpeechEvent, SpeechRecognitionProvider, SpeechRequest, SpeechResult, SpeechSink,
};

fn get(target: &JsValue, key: &str) -> JsValue {
    Reflect::get(target, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn set(target: &JsValue, key: &str, value: &JsValue) {
    let _ = Reflect::set(target, &JsValue::from_str(key), value);
}

fn call(target: &JsValue, method: &str, args: &[JsValue]) -> JsValue {
    let Ok(function) = get(target, method).dyn_into::<Function>() else {
        return JsValue::UNDEFINED;
    };
    let array: Array = args.iter().cloned().collect();
    function.apply(target, &array).unwrap_or(JsValue::UNDEFINED)
}

/// The recognizer's constructor, if the browser has one.
fn constructor() -> Option<Function> {
    let global = js_sys::global();
    ["SpeechRecognition", "webkitSpeechRecognition"]
        .iter()
        .find_map(|name| get(&global, name).dyn_into::<Function>().ok())
}

/// One running session's browser objects.
#[derive(Default)]
struct Session {
    recognition: Option<JsValue>,
    /// The microphone stream, its source and analyser, for the volume meter.
    audio: Option<(JsValue, JsValue, JsValue)>,
    /// Keeps the event handlers alive for the session (dropping them would free the
    /// functions the browser calls).
    _handlers: Vec<Closure<dyn FnMut(JsValue)>>,
    /// Events from the browser, drained into GPUI a frame at a time.
    queue: Rc<RefCell<VecDeque<SpeechEvent>>>,
    ended: bool,
}

/// The browser provider.
#[derive(Default)]
pub(crate) struct WebSpeechProvider {
    session: Rc<RefCell<Session>>,
    audio_context: RefCell<Option<JsValue>>,
}

impl WebSpeechProvider {
    /// Upstream `getDefaultAudioContext`: one shared context, resumed when suspended.
    fn audio_context(&self) -> Option<JsValue> {
        let mut slot = self.audio_context.borrow_mut();
        let closed = slot
            .as_ref()
            .is_some_and(|ctx| get(ctx, "state").as_string().as_deref() == Some("closed"));
        if slot.is_none() || closed {
            let ctor = get(&js_sys::global(), "AudioContext")
                .dyn_into::<Function>()
                .ok()?;
            *slot = Reflect::construct(&ctor, &Array::new()).ok();
        }
        let ctx = slot.clone()?;
        if get(&ctx, "state").as_string().as_deref() == Some("suspended") {
            call(&ctx, "resume", &[]);
        }
        Some(ctx)
    }

    fn stop_audio(session: &mut Session) {
        if let Some((stream, source, _)) = session.audio.take() {
            call(&source, "disconnect", &[]);
            let tracks = call(&stream, "getTracks", &[]);
            if let Ok(tracks) = tracks.dyn_into::<Array>() {
                for track in tracks.iter() {
                    call(&track, "stop", &[]);
                }
            }
        }
    }
}

/// The results of a browser `result` event.
fn results(event: &JsValue) -> SpeechEvent {
    let index = get(event, "resultIndex").as_f64().unwrap_or(0.) as usize;
    let list = get(event, "results");
    let length = get(&list, "length").as_f64().unwrap_or(0.) as usize;
    let results = (index..length)
        .map(|i| {
            let result = Reflect::get_u32(&list, i as u32).unwrap_or(JsValue::UNDEFINED);
            let first = Reflect::get_u32(&result, 0).unwrap_or(JsValue::UNDEFINED);
            SpeechResult {
                transcript: get(&first, "transcript").as_string().unwrap_or_default(),
                is_final: get(&result, "isFinal").as_bool().unwrap_or(false),
            }
        })
        .collect();
    SpeechEvent::Result {
        result_index: 0,
        results,
    }
}

impl SpeechRecognitionProvider for WebSpeechProvider {
    fn is_supported(&self) -> bool {
        constructor().is_some()
    }

    fn start(&self, request: SpeechRequest, sink: SpeechSink, cx: &mut App) {
        let Some(ctor) = constructor() else { return };
        let Ok(recognition) = Reflect::construct(&ctor, &Array::new()) else {
            return;
        };
        set(&recognition, "lang", &JsValue::from_str(&request.lang));
        set(
            &recognition,
            "continuous",
            &JsValue::from_bool(request.continuous),
        );
        set(
            &recognition,
            "interimResults",
            &JsValue::from_bool(request.interim_results),
        );
        let queue: Rc<RefCell<VecDeque<SpeechEvent>>> = Rc::default();
        let mut handlers = Vec::new();
        let mut on = |name: &str, f: Box<dyn FnMut(JsValue)>| {
            let closure = Closure::wrap(f);
            set(&recognition, name, closure.as_ref());
            handlers.push(closure);
        };
        let q = queue.clone();
        on(
            "onstart",
            Box::new(move |_| q.borrow_mut().push_back(SpeechEvent::Start)),
        );
        let q = queue.clone();
        on(
            "onend",
            Box::new(move |_| q.borrow_mut().push_back(SpeechEvent::End)),
        );
        let q = queue.clone();
        on(
            "onspeechstart",
            Box::new(move |_| q.borrow_mut().push_back(SpeechEvent::SpeechStart)),
        );
        let q = queue.clone();
        on(
            "onspeechend",
            Box::new(move |_| q.borrow_mut().push_back(SpeechEvent::SpeechEnd)),
        );
        let q = queue.clone();
        on(
            "onresult",
            Box::new(move |event| q.borrow_mut().push_back(results(&event))),
        );
        let q = queue.clone();
        on(
            "onerror",
            Box::new(move |event| {
                q.borrow_mut().push_back(SpeechEvent::Error {
                    error: get(&event, "error").as_string().unwrap_or_default(),
                    message: get(&event, "message").as_string().filter(|m| !m.is_empty()),
                })
            }),
        );
        let q = queue.clone();
        on(
            "onnomatch",
            Box::new(move |_| q.borrow_mut().push_back(SpeechEvent::NoMatch)),
        );
        {
            let mut session = self.session.borrow_mut();
            if let Some(old) = session.recognition.take() {
                call(&old, "abort", &[]);
            }
            Self::stop_audio(&mut session);
            *session = Session {
                recognition: Some(recognition.clone()),
                audio: None,
                _handlers: handlers,
                queue: queue.clone(),
                ended: false,
            };
        }
        // The microphone for the meter (upstream `createVolumeAnalyser`).
        if let Some(ctx) = self.audio_context() {
            let devices = get(&get(&js_sys::global(), "navigator"), "mediaDevices");
            let constraints = Object::new();
            set(&constraints, "audio", &JsValue::TRUE);
            let promise = call(&devices, "getUserMedia", &[constraints.into()]);
            if let Ok(promise) = promise.dyn_into::<js_sys::Promise>() {
                let session = self.session.clone();
                let ready = Closure::once(Box::new(move |stream: JsValue| {
                    let source = call(&ctx, "createMediaStreamSource", &[stream.clone()]);
                    let analyser = call(&ctx, "createAnalyser", &[]);
                    set(&analyser, "fftSize", &JsValue::from_f64(256.));
                    set(&analyser, "smoothingTimeConstant", &JsValue::from_f64(0.5));
                    call(&source, "connect", &[analyser.clone()]);
                    let mut session = session.borrow_mut();
                    if session.ended {
                        call(&source, "disconnect", &[]);
                    } else {
                        session.audio = Some((stream, source, analyser));
                    }
                }) as Box<dyn FnOnce(JsValue)>);
                let _ = promise.then(&ready);
                ready.forget();
            }
        }
        call(&recognition, "start", &[]);
        // Drain the browser's events into GPUI a frame at a time, with the
        // microphone's frequency data while the session runs.
        let session = self.session.clone();
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let (events, frame, done) = {
                    let session = session.borrow();
                    if !Rc::ptr_eq(&session.queue, &queue) {
                        break;
                    }
                    let events: Vec<_> = session.queue.borrow_mut().drain(..).collect();
                    let frame = session.audio.as_ref().map(|(_, _, analyser)| {
                        let bins =
                            get(analyser, "frequencyBinCount").as_f64().unwrap_or(128.) as u32;
                        let data = Uint8Array::new_with_length(bins);
                        call(analyser, "getByteFrequencyData", &[data.clone().into()]);
                        data.to_vec()
                    });
                    let done = events.iter().any(|e| *e == SpeechEvent::End);
                    (events, frame, done)
                };
                let sink = sink.clone();
                cx.update(|cx| {
                    if let Some(frame) = frame {
                        sink.send(SpeechEvent::AudioFrame(frame), cx);
                    }
                    for event in events {
                        sink.send(event, cx);
                    }
                });
                if done {
                    let mut session = session.borrow_mut();
                    session.ended = true;
                    WebSpeechProvider::stop_audio(&mut session);
                    break;
                }
            }
        })
        .detach();
    }

    fn stop(&self, _: &mut App) {
        if let Some(recognition) = self.session.borrow().recognition.as_ref() {
            call(recognition, "stop", &[]);
        }
    }

    fn abort(&self, _: &mut App) {
        let mut session = self.session.borrow_mut();
        if let Some(recognition) = session.recognition.as_ref() {
            call(recognition, "abort", &[]);
        }
        Self::stop_audio(&mut session);
    }

    fn play_samples(&self, samples: &[f32], sample_rate: u32, _: &mut App) {
        let Some(ctx) = self.audio_context() else {
            return;
        };
        let buffer = call(
            &ctx,
            "createBuffer",
            &[
                JsValue::from_f64(1.),
                JsValue::from_f64(samples.len() as f64),
                JsValue::from_f64(sample_rate as f64),
            ],
        );
        let data = js_sys::Float32Array::from(samples);
        call(
            &buffer,
            "copyToChannel",
            &[data.into(), JsValue::from_f64(0.)],
        );
        let source = call(&ctx, "createBufferSource", &[]);
        set(&source, "buffer", &buffer);
        call(&source, "connect", &[get(&ctx, "destination")]);
        call(&source, "start", &[]);
    }
}
