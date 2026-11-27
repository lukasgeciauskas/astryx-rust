//! Speech recognition and dictation tests, against a fake provider.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, InteractiveElement as _, IntoElement, Modifiers, ParentElement, SharedString, Styled,
    TestAppContext, VisualTestContext, Window, div, px,
};

use super::speech::{FakeProvider, VolumeAnalyser};
use super::*;
use crate::segmented::tests::host;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

/// A target that records what dictation writes.
#[derive(Default)]
struct Field {
    log: RefCell<Vec<String>>,
}

impl DictationTarget for Field {
    fn focus(&self, _: Option<&mut Window>, _: &mut App) {
        self.log.borrow_mut().push("focus".into());
    }
    fn insert_text(&self, text: &str, _: &mut App) {
        self.log.borrow_mut().push(format!("insert {text:?}"));
    }
    fn set_interim(&self, interim: Option<SharedString>, _: &mut App) {
        self.log
            .borrow_mut()
            .push(format!("interim {:?}", interim.map(|s| s.to_string())));
    }
}

#[derive(IntoElement)]
struct Dictating {
    field: Rc<Field>,
    sounds: bool,
}

impl gpui::RenderOnce for Dictating {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let dictation = use_chat_dictation(
            "dictate",
            UseChatDictationOptions {
                sounds: self.sounds,
                target: Some(self.field.clone()),
                ..Default::default()
            },
            window,
            cx,
        );
        div().p(px(20.)).child(
            div()
                .debug_selector(|| "mic".into())
                .child(ChatDictationButton::new("mic", dictation)),
        )
    }
}

#[gpui::test]
fn without_a_provider_the_button_hides(cx: &mut TestAppContext) {
    let field = Rc::new(Field::default());
    let cx = host(cx, move || {
        Dictating {
            field: field.clone(),
            sounds: false,
        }
        .into_any_element()
    });
    settle(cx);
    assert_eq!(cx.debug_bounds("mic").unwrap().size.width, px(0.));
}

#[gpui::test]
fn dictation_writes_final_words_and_shows_interim_ones(cx: &mut TestAppContext) {
    let provider = Rc::new(FakeProvider::default());
    cx.update(|cx| set_speech_recognition_provider(provider.clone(), cx));
    let field = Rc::new(Field::default());
    let view_field = field.clone();
    let cx = host(cx, move || {
        Dictating {
            field: view_field.clone(),
            sounds: true,
        }
        .into_any_element()
    });
    settle(cx);
    let mic = cx.debug_bounds("mic").unwrap();
    assert!(mic.size.width > px(0.));
    cx.simulate_click(mic.center(), Modifiers::none());
    settle(cx);
    assert_eq!(provider.calls.borrow().last().unwrap(), "start en");
    let sink = provider.sink.borrow().clone().expect("a session");
    cx.update(|_, cx| {
        sink.send(SpeechEvent::Start, cx);
        sink.send(
            SpeechEvent::Result {
                result_index: 0,
                results: vec![SpeechResult {
                    transcript: "hello wor".into(),
                    is_final: false,
                }],
            },
            cx,
        );
        sink.send(
            SpeechEvent::Result {
                result_index: 0,
                results: vec![SpeechResult {
                    transcript: "hello world".into(),
                    is_final: true,
                }],
            },
            cx,
        );
        sink.send(SpeechEvent::End, cx);
    });
    settle(cx);
    assert_eq!(
        *field.log.borrow(),
        [
            "interim Some(\"\")",
            "interim Some(\"hello wor\")",
            "interim None",
            "focus",
            "insert \"hello world \"",
            "interim None",
        ]
    );
    assert_eq!(*provider.played.borrow(), 2, "the start and stop sounds");
}

#[test]
fn the_analyser_calibrates_then_reads_above_the_noise_floor() {
    let mut analyser = VolumeAnalyser::default();
    for _ in 0..60 {
        analyser.frame(vec![20; 128]);
        assert_eq!(analyser.volume(), 0., "calibrating");
    }
    analyser.frame(vec![20; 128]);
    assert_eq!(analyser.volume(), 0., "noise reads as silence");
    analyser.frame(vec![200; 128]);
    assert!(analyser.volume() > 0.6);
    let bands = analyser.bands(5);
    assert_eq!(bands.len(), 5);
    assert!(bands.iter().all(|b| *b > 0.6));
    assert!((analyser.raw_bands(5)[0] - 200. / 255.).abs() < 1e-4);
}

#[test]
fn a_plop_is_its_duration_after_its_delay() {
    let high = super::dictation::plop(523., 0.07, 0.25);
    assert_eq!(high.len(), ((0.07 + 0.06) * 44_100.) as usize);
    assert!(
        high[..3000].iter().all(|s| *s == 0.),
        "silent through the delay"
    );
    let low = super::dictation::plop(180., 0., 0.25);
    assert_eq!(low.len(), (0.18 * 44_100.) as usize);
}
