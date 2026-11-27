//! The synthetic-input harness the gesture tests share, upstream
//! `__tests__/harness.ts`: a manual clock, an arena wired to a transition
//! log, fingers that land, move and lift at explicit timestamps, a
//! recording consumer, a scripted scroll probe, and the scripted recognizer
//! the arena tests drive.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::gestures::native_scroll::ScriptedProbe;
use crate::gestures::*;

/// A number as JavaScript prints it, rounded to two decimals.
pub fn js(value: f64) -> String {
    let rounded = (value * 100.).round() / 100.;
    if rounded.fract() == 0. {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

/// The environment a test sets.
pub struct TestEnv {
    pub width: Cell<f64>,
    pub height: Cell<f64>,
    pub rtl: Cell<bool>,
    pub view: Cell<Option<ViewRect>>,
}

impl GestureEnvironment for TestEnv {
    fn viewport_size(&self) -> (f64, f64) {
        (self.width.get(), self.height.get())
    }
    fn is_rtl(&self, _view: Option<ViewKey>) -> bool {
        self.rtl.get()
    }
    fn view_rect(&self, _view: Option<ViewKey>) -> Option<ViewRect> {
        self.view.get()
    }
}

pub struct Harness {
    pub arena: GestureArena,
    pub clock: Rc<ManualClock>,
    pub log: Rc<RefCell<Vec<String>>>,
    pub claims: Rc<RefCell<Vec<u64>>>,
    pub deferred_clicks: Rc<RefCell<Vec<(u64, bool, f64)>>>,
    pub env: Rc<TestEnv>,
    fingers: RefCell<HashMap<u64, (f64, f64, f64)>>,
}

pub fn create_harness() -> Harness {
    let clock = ManualClock::new();
    let env = Rc::new(TestEnv {
        width: Cell::new(390.),
        height: Cell::new(844.),
        rtl: Cell::new(false),
        view: Cell::new(None),
    });
    let arena = GestureArena::new(ArenaOptions {
        clock: Some(clock.clone()),
        environment: Some(env.clone()),
        callbacks: ArenaCallbacks::default(),
    });
    let log = Rc::new(RefCell::new(Vec::new()));
    let claims = Rc::new(RefCell::new(Vec::new()));
    let deferred_clicks = Rc::new(RefCell::new(Vec::new()));
    {
        let mut callbacks = arena.callbacks();
        let weak = arena.downgrade();
        let entries = log.clone();
        callbacks.on_transition = Some(Rc::new(
            move |r: &AnyRecognizer, _from, to: GestureState| {
                let now = weak.upgrade().map_or(0., |a| a.now());
                entries
                    .borrow_mut()
                    .push(format!("{}:{}@{}", r.name(), to, js(now)));
            },
        ));
        let claimed = claims.clone();
        callbacks.on_claim = Some(Rc::new(move |ids: &[u64], _r: &AnyRecognizer| {
            claimed.borrow_mut().extend_from_slice(ids);
        }));
        let weak = arena.downgrade();
        let clicks = deferred_clicks.clone();
        callbacks.on_deferred_click = Some(Rc::new(move |touch: &GestureTouch, allowed| {
            let now = weak.upgrade().map_or(0., |a| a.now());
            clicks.borrow_mut().push((touch.id(), allowed, now));
        }));
    }
    Harness {
        arena,
        clock,
        log,
        claims,
        deferred_clicks,
        env,
        fingers: RefCell::new(HashMap::new()),
    }
}

/// What a finger lands with.
#[derive(Clone)]
pub struct Down {
    pub recognizers: Vec<AnyRecognizer>,
    pub consumer: Option<Rc<dyn TouchConsumer>>,
    pub scroll: Option<ScrollProbe>,
    pub pointer_type: PointerKind,
    pub target: Option<ViewKey>,
}

/// A finger landing on these recognizers, innermost first.
pub fn on(recognizers: &[&AnyRecognizer]) -> Down {
    Down {
        recognizers: recognizers.iter().map(|r| (*r).clone()).collect(),
        consumer: None,
        scroll: None,
        pointer_type: PointerKind::Touch,
        target: None,
    }
}

impl Down {
    pub fn consumer(mut self, consumer: Rc<dyn TouchConsumer>) -> Self {
        self.consumer = Some(consumer);
        self
    }
    pub fn scroll(mut self, probe: ScrollProbe) -> Self {
        self.scroll = Some(probe);
        self
    }
    pub fn pointer(mut self, kind: PointerKind) -> Self {
        self.pointer_type = kind;
        self
    }
}

impl Down {
    pub fn input(&self, id: u64, x: f64, y: f64, time: f64) -> ArenaBeganInput {
        let mut input = TouchBeganInput::new(id, self.pointer_type, x, y, time);
        input.target = self.target;
        input.scroll = self.scroll.clone();
        ArenaBeganInput {
            input,
            recognizers: self.recognizers.clone(),
            consumer: self.consumer.clone(),
        }
    }
}

pub struct Finger<'a> {
    h: &'a Harness,
    pub id: u64,
}

impl Harness {
    pub fn finger(&self, id: u64) -> Finger<'_> {
        Finger { h: self, id }
    }

    /// Advances the clock and runs every timer due by `time`.
    pub fn advance(&self, time: f64) {
        self.clock.set_time(time);
        self.arena.advance_time(time);
    }

    pub fn log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }

    pub fn logged(&self, entry: &str) -> bool {
        self.log.borrow().iter().any(|e| e == entry)
    }

    pub fn claims(&self) -> Vec<u64> {
        self.claims.borrow().clone()
    }

    pub fn deferred(&self) -> Vec<(u64, bool, f64)> {
        self.deferred_clicks.borrow().clone()
    }
}

impl<'a> Finger<'a> {
    fn set(&self, x: f64, y: f64, time: f64) {
        self.h.fingers.borrow_mut().insert(self.id, (x, y, time));
        self.h.clock.set_time(time);
    }

    fn at(&self) -> (f64, f64, f64) {
        self.h
            .fingers
            .borrow()
            .get(&self.id)
            .copied()
            .unwrap_or((0., 0., 0.))
    }

    pub fn down(self, x: f64, y: f64, time: f64, options: Down) -> Self {
        self.set(x, y, time);
        self.h.arena.began(vec![options.input(self.id, x, y, time)]);
        self
    }

    pub fn move_to(self, x: f64, y: f64, time: f64) -> Self {
        self.set(x, y, time);
        self.h
            .arena
            .moved(vec![TouchInput::new(self.id, x, y, time)]);
        self
    }

    /// Moves in `steps` equal steps over `duration` ms.
    pub fn drag(self, dx: f64, dy: f64, duration: f64, steps: u32) -> Self {
        let (x0, y0, t0) = self.at();
        let mut finger = self;
        for i in 1..=steps {
            let k = f64::from(i) / f64::from(steps);
            finger = finger.move_to(x0 + dx * k, y0 + dy * k, t0 + duration * k);
        }
        finger
    }

    pub fn up(&self, time: f64) -> Option<ClickDisposition> {
        let (x, y, _) = self.at();
        self.up_at(time, x, y)
    }

    pub fn up_at(&self, time: f64, x: f64, y: f64) -> Option<ClickDisposition> {
        self.set(x, y, time);
        self.h
            .arena
            .ended(vec![TouchInput::new(self.id, x, y, time)])
            .get(&self.id)
            .copied()
    }

    pub fn cancel(&self, time: f64) {
        self.h.clock.set_time(time);
        self.h.arena.cancelled(&[self.id], time);
    }

    pub fn taken_by_browser(&self, time: f64) {
        self.h.clock.set_time(time);
        self.h.arena.browser_took_over(&[self.id], time);
    }
}

/// A consumer that records what the arena delivered to it.
#[derive(Default)]
pub struct RecordingConsumer {
    pub events: RefCell<Vec<String>>,
    #[allow(clippy::type_complexity)]
    pub veto: RefCell<Option<Rc<dyn Fn(&AnyRecognizer) -> bool>>>,
}

impl RecordingConsumer {
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }
    pub fn events(&self) -> Vec<String> {
        self.events.borrow().clone()
    }
}

impl TouchConsumer for RecordingConsumer {
    fn touch_began(&self, touch: &GestureTouch) {
        self.events
            .borrow_mut()
            .push(format!("began:{}", touch.id()));
    }
    fn touch_moved(&self, touch: &GestureTouch) {
        self.events
            .borrow_mut()
            .push(format!("moved:{}", touch.id()));
    }
    fn touch_ended(&self, touch: &GestureTouch) {
        self.events
            .borrow_mut()
            .push(format!("ended:{}", touch.id()));
    }
    fn touch_cancelled(&self, touch: &GestureTouch) {
        self.events
            .borrow_mut()
            .push(format!("cancelled:{}", touch.id()));
    }
    fn allows_ancestor_recognizer(&self, recognizer: &AnyRecognizer) -> bool {
        self.veto
            .borrow()
            .as_ref()
            .is_none_or(|veto| !veto(recognizer))
    }
}

/// A scroll probe with a scripted answer.
pub fn scroll_probe(
    can_scroll: impl Fn(f64, f64) -> bool + 'static,
    is_inside_scroller: bool,
) -> ScrollProbe {
    Rc::new(ScriptedProbe {
        can_scroll,
        is_inside_scroller,
    })
}

// --- The scripted recognizer of the arena tests --------------------------------

pub type Step = Rc<dyn Fn(&AnyRecognizer, &GestureEventContext)>;

#[derive(Clone, Default)]
pub struct Script {
    pub began: Option<Step>,
    pub moved: Option<Step>,
    pub ended: Option<Step>,
}

pub fn step(f: impl Fn(&AnyRecognizer, &GestureEventContext) + 'static) -> Option<Step> {
    Some(Rc::new(f))
}

/// A recognizer that does what its script says on each touch event, and
/// what the test asks between them. Without a script for the lift, an active
/// gesture ends with its last touch and a possible one fails.
pub struct Scripted {
    discrete: bool,
    script: Script,
    pub elsewhere: Option<Rc<RefCell<Vec<u64>>>>,
    pub observes: bool,
}

const RECOGNIZE: u32 = 1;
const FAIL: u32 = 2;

impl RecognizerBehavior for Scripted {
    fn kind(&self) -> RecognizerKind {
        if self.discrete {
            RecognizerKind::Tap
        } else {
            RecognizerKind::Pan
        }
    }
    fn is_discrete(&self) -> bool {
        self.discrete
    }
    fn touches_began(
        &mut self,
        r: &AnyRecognizer,
        _t: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if let Some(began) = &self.script.began {
            began(r, event);
        }
    }
    fn touches_moved(
        &mut self,
        r: &AnyRecognizer,
        _t: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if let Some(moved) = &self.script.moved {
            moved(r, event);
        } else if r.state().is_active() {
            r.change();
        }
    }
    fn touches_ended(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) {
        if let Some(ended) = &self.script.ended {
            ended(r, event);
            return;
        }
        if r.number_of_touches() > touches.len() {
            return;
        }
        if r.state().is_active() {
            r.set_state(GestureState::Ended);
        } else if r.state() == GestureState::Possible {
            r.fail();
        }
    }
    fn touches_taken_by_browser(
        &mut self,
        r: &AnyRecognizer,
        touches: &[GestureTouch],
        event: &GestureEventContext,
    ) -> bool {
        if self.observes {
            return true;
        }
        if !r.claims_touches() {
            return true;
        }
        self.touches_cancelled(r, touches, event);
        false
    }
    fn touch_began_elsewhere(
        &mut self,
        _r: &AnyRecognizer,
        touches: &[GestureTouch],
        _e: &GestureEventContext,
    ) {
        if let Some(record) = &self.elsewhere {
            record
                .borrow_mut()
                .extend(touches.iter().map(GestureTouch::id));
        }
    }
    fn timer_fired(&mut self, r: &AnyRecognizer, tag: u32) {
        match tag {
            RECOGNIZE => r.recognize(),
            FAIL => r.fail(),
            _ => {}
        }
    }
}

/// The scripted recognizer's commands, on any handle.
pub trait ScriptedExt {
    fn recognize(&self);
    fn change(&self);
    fn fail(&self);
    fn cancel_self(&self);
    fn recognize_at(&self, deadline: f64);
    fn wait_until(&self, deadline: f64);
}

impl ScriptedExt for AnyRecognizer {
    fn recognize(&self) {
        self.set_state(if self.is_discrete() {
            GestureState::Ended
        } else {
            GestureState::Began
        });
    }
    fn change(&self) {
        self.set_state(GestureState::Changed);
    }
    fn fail(&self) {
        self.set_waiting(false);
        self.set_state(GestureState::Failed);
    }
    fn cancel_self(&self) {
        self.set_state(GestureState::Cancelled);
    }
    fn recognize_at(&self, deadline: f64) {
        self.set_timer(deadline, RECOGNIZE);
    }
    fn wait_until(&self, deadline: f64) {
        self.set_waiting(true);
        self.set_timer(deadline, FAIL);
    }
}

#[derive(Default)]
pub struct ScriptOptions {
    pub base: RecognizerOptions,
    pub discrete: bool,
    pub observes: bool,
    pub elsewhere: Option<Rc<RefCell<Vec<u64>>>>,
}

pub fn scripted_with(name: &str, script: Script, mut options: ScriptOptions) -> AnyRecognizer {
    options.base.name = Some(name.to_string());
    Recognizer::with_behavior(
        options.base,
        Scripted {
            discrete: options.discrete,
            script,
            elsewhere: options.elsewhere,
            observes: options.observes,
        },
        "Scripted",
    )
    .any()
    .clone()
}

pub fn scripted(name: &str, script: Script) -> AnyRecognizer {
    scripted_with(name, script, ScriptOptions::default())
}

/// Recognizes on the first move.
pub fn on_move() -> Script {
    Script {
        moved: step(|r, _| {
            if r.state() == GestureState::Possible {
                r.recognize()
            } else {
                r.change()
            }
        }),
        ..Script::default()
    }
}

/// A discrete recognizer that recognizes at the lift.
pub fn at_lift(name: &str, base: RecognizerOptions) -> AnyRecognizer {
    scripted_with(
        name,
        Script {
            ended: step(|r, _| r.recognize()),
            ..Script::default()
        },
        ScriptOptions {
            base,
            discrete: true,
            ..ScriptOptions::default()
        },
    )
}

/// Waits after the lift until `deadline`, as a double tap between taps does.
pub fn waits_after_lift(name: &str, deadline: f64) -> AnyRecognizer {
    scripted_with(
        name,
        Script {
            ended: step(move |r, _| r.wait_until(deadline)),
            ..Script::default()
        },
        ScriptOptions {
            discrete: true,
            ..ScriptOptions::default()
        },
    )
}

/// Strings for an expected log.
pub fn strings(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|e| (*e).to_string()).collect()
}
