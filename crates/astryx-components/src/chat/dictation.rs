//! useChatDictation and ChatDictationButton, upstream `Chat/useChatDictation.ts` and
//! `Chat/ChatDictationButton.tsx`: speech into the composer, with the interim words
//! shown in place, optional start and stop sounds, and a microphone button whose bars
//! follow the voice.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_primitives::ElementSize;
use astryx_theme::color::mix_srgb;
use astryx_theme::current;
use gpui::Refineable as _;
use gpui::prelude::*;
use gpui::{App, ElementId, SharedString, StyleRefinement, Window, div, px};

use super::speech::{
    SpeechRecognition, UseSpeechRecognitionOptions, provider, use_speech_recognition,
};
use crate::button::{Button, ButtonVariant};
use crate::icon::{Icon, IconSize, IconSource};

/// Where dictation writes: upstream's `inputRef` (a ChatComposerInput handle). The
/// composer's input implements it; implement it to dictate into another field.
pub trait DictationTarget {
    /// Focuses the field.
    fn focus(&self, window: Option<&mut Window>, cx: &mut App);
    /// Inserts final text at the caret.
    fn insert_text(&self, text: &str, cx: &mut App);
    /// Shows the words heard so far after the text, dimmed and italic, or clears them.
    fn set_interim(&self, interim: Option<SharedString>, cx: &mut App);
}

/// Upstream `UseChatDictationOptions`: the speech options, plus sounds and a target.
#[derive(Clone, Default)]
pub struct UseChatDictationOptions {
    pub speech: UseSpeechRecognitionOptions,
    /// Upstream `hasSounds`: a rising plop on start and a falling one on stop.
    pub sounds: bool,
    /// Upstream `inputRef`.
    pub target: Option<Rc<dyn DictationTarget>>,
}

/// What dictation remembers between frames: recent volume, for shouting.
#[derive(Default)]
struct DictationState {
    history: Vec<f32>,
}

/// Upstream `useChatDictation`: speech recognition that writes into a composer. A
/// transcript spoken loudly (an average volume of 0.15 over the last 10 or more frames)
/// is upper-cased, as upstream does.
pub fn use_chat_dictation(
    id: impl Into<ElementId>,
    options: UseChatDictationOptions,
    window: &mut Window,
    cx: &mut App,
) -> SpeechRecognition {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-dictation").into()),
        cx,
        |_, _| Rc::new(RefCell::new(DictationState::default())),
    );
    let history = state.read(cx).clone();
    let user_transform = options.speech.transform_transcript.clone();
    let loudness = history.clone();
    let mut speech = options.speech.clone().transform_transcript(move |text| {
        let mut text = match &user_transform {
            Some(f) => f(text),
            None => text.to_string(),
        };
        let history = &loudness.borrow().history;
        let average = if history.is_empty() {
            0.
        } else {
            history.iter().sum::<f32>() / history.len() as f32
        };
        if average >= 0.15 && history.len() >= 10 {
            text = text.to_uppercase();
        }
        text
    });
    let target = options.target.clone();
    let on_transcript = options.speech.on_transcript.clone();
    let on_result = options.speech.on_result.clone();
    speech.on_result = if target.is_some() {
        None
    } else {
        on_result.clone()
    };
    speech.on_transcript = Some(Rc::new(move |text: &str, is_final: bool, cx: &mut App| {
        if let Some(target) = &target {
            if is_final {
                target.set_interim(None, cx);
                target.focus(None, cx);
                target.insert_text(&format!("{text} "), cx);
                if let Some(f) = &on_result {
                    f(text, cx);
                }
            } else {
                target.set_interim(Some(text.to_string().into()), cx);
            }
        }
        if let Some(f) = &on_transcript {
            f(text, is_final, cx);
        }
    }));
    let (on_start, on_end, sounds) = (
        options.speech.on_start.clone(),
        options.speech.on_end.clone(),
        options.sounds,
    );
    let (start_target, end_target) = (options.target.clone(), options.target.clone());
    speech.on_start = Some(Rc::new(move |cx: &mut App| {
        if sounds {
            play_cue(&[392., 523.], cx);
        }
        if let Some(target) = &start_target {
            target.set_interim(Some(SharedString::default()), cx);
        }
        if let Some(f) = &on_start {
            f(cx);
        }
    }));
    let end_history = history.clone();
    speech.on_end = Some(Rc::new(move |cx: &mut App| {
        end_history.borrow_mut().history.clear();
        if sounds {
            play_cue(&[523., 392.], cx);
        }
        if let Some(target) = &end_target {
            target.set_interim(None, cx);
        }
        if let Some(f) = &on_end {
            f(cx);
        }
    }));
    let speech = use_speech_recognition(id, speech, window, cx);
    // The volume history, a frame at a time (upstream keeps the last 30).
    if speech.is_listening(cx) {
        let volume = speech.volume(cx);
        let mut s = history.borrow_mut();
        s.history.push(volume);
        if s.history.len() > 30 {
            s.history.remove(0);
        }
    }
    speech
}

const SAMPLE_RATE: u32 = 44_100;

/// Upstream `playPlop`: a short sine whose pitch dips, its gain falling exponentially.
pub(crate) fn plop(freq: f32, delay: f32, volume: f32) -> Vec<f32> {
    let duration = if freq < 200. { 0.18 } else { 0.06 };
    let total = ((delay + duration) * SAMPLE_RATE as f32) as usize;
    let mut phase = 0f32;
    (0..total)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            if t < delay {
                return 0.;
            }
            let local = t - delay;
            // 1.3f to f over 10ms, then on to 0.93f by the end (exponential ramps).
            let pitch = if local < 0.01 {
                freq * 1.3 * (1. / 1.3f32).powf(local / 0.01)
            } else {
                freq * 0.93f32.powf((local - 0.01) / (duration - 0.01))
            };
            phase += std::f32::consts::TAU * pitch / SAMPLE_RATE as f32;
            let attack = duration * 0.12;
            let gain = if local < attack {
                volume * 0.2f32.powf(local / attack)
            } else {
                volume * 0.2 * (0.001 / (volume * 0.2)).powf((local - attack) / (duration - attack))
            };
            phase.sin() * gain
        })
        .collect()
}

/// Upstream's start (392 then 523 Hz) and stop (523 then 392 Hz) sounds, 70ms apart.
fn play_cue(freqs: &[f32; 2], cx: &mut App) {
    let Some(provider) = provider(cx) else { return };
    let first = plop(freqs[0], 0., 0.25);
    let second = plop(freqs[1], 0.07, 0.25);
    let mut samples = vec![0.; first.len().max(second.len())];
    for (i, s) in first.iter().enumerate() {
        samples[i] += s;
    }
    for (i, s) in second.iter().enumerate() {
        samples[i] += s;
    }
    provider.play_samples(&samples, SAMPLE_RATE, cx);
}

/// Upstream `size` on ChatDictationButton.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatDictationButtonSize {
    Sm,
    #[default]
    Md,
}

/// Upstream `ChatDictationButton`.
///
/// ```ignore
/// let dictation = use_chat_dictation("dictate", Default::default(), window, cx);
/// ChatDictationButton::new("mic", dictation)
/// ```
#[derive(IntoElement)]
pub struct ChatDictationButton {
    id: ElementId,
    dictation: SpeechRecognition,
    size: ChatDictationButtonSize,
    hidden_when_unsupported: bool,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl ChatDictationButton {
    /// Upstream `dictation`: the value of `use_speech_recognition` or `use_chat_dictation`.
    pub fn new(id: impl Into<ElementId>, dictation: SpeechRecognition) -> Self {
        ChatDictationButton {
            id: id.into(),
            dictation,
            size: ChatDictationButtonSize::Md,
            hidden_when_unsupported: true,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `size`.
        size: ChatDictationButtonSize,
        /// Upstream `isHiddenWhenUnsupported`: true by default.
        hidden_when_unsupported: bool,
        /// Upstream `label`: else Start or Stop dictation.
        ?label: SharedString,
    }
}

impl Styled for ChatDictationButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Upstream's bars: five, from 8% of their height, sized by size.
const BAR_COUNT: usize = 5;
const BAR_MIN_SCALE: f32 = 0.08;

impl RenderOnce for ChatDictationButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let supported = self.dictation.is_supported();
        if self.hidden_when_unsupported && !supported {
            return div().into_any_element();
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let i18n = astryx_i18n::current(cx);
        let listening = self.dictation.is_listening(cx);
        let label = self.label.clone().unwrap_or_else(|| {
            i18n.t(if listening {
                "@astryx.chatDictationButton.stopDictation"
            } else {
                "@astryx.chatDictationButton.startDictation"
            })
        });
        let (bar_width, bar_gap, bar_height, element, icon) = match self.size {
            ChatDictationButtonSize::Sm => (2., 1.5, 14., ElementSize::Sm, IconSize::Sm),
            ChatDictationButtonSize::Md => (2.5, 2., 18., ElementSize::Md, IconSize::Md),
        };
        let volume = self.dictation.volume(cx);
        let clipping = volume >= 0.2;
        let color = if clipping {
            let strength = ((volume - 0.2) / 0.1).min(1.);
            mix_srgb(t.color.accent, t.color.error, strength)
        } else {
            t.color.accent
        };
        let bars = listening.then(|| {
            let bands = self.dictation.bands(cx);
            div()
                .absolute()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(bar_gap))
                .h(px(bar_height))
                .children(bands.iter().take(BAR_COUNT).map(|band| {
                    let level = (band / 0.2).powf(0.5).min(1.);
                    let scale = BAR_MIN_SCALE + level * (1. - BAR_MIN_SCALE);
                    div()
                        .w(px(bar_width))
                        .h(px(bar_height * scale))
                        .rounded_full()
                        .bg(color)
                }))
        });
        let dictation = self.dictation.clone();
        let mut button = Button::new(ElementId::Name(format!("{}-button", self.id).into()), label)
            .variant(ButtonVariant::Ghost)
            .size(element)
            .icon_only(true)
            .disabled(!supported)
            .on_click(move |_, window, cx| {
                dictation.toggle(cx);
                window.refresh();
            });
        if !listening {
            button = button.icon(Icon::new(IconSource::Named("microphone".into())).size(icon));
        }
        let mut el = div()
            .id(self.id.clone())
            .relative()
            .inline_flex()
            .items_center()
            .justify_center();
        theme.refine(el.style(), "chat-dictation-button", &[], None);
        el.style().refine(&self.style);
        el.children(bars).child(button).into_any_element()
    }
}
