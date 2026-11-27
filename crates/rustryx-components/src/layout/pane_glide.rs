//! The glide of what sits beside a sliding LayoutPanel (LayoutPanel FR7, Resizable FR8,
//! AST-105 FR9): a Layout's row of panels and content is a glide area (fork patch R19).
//! When a panel slides shut or open, its layout snaps to the new size at once and the
//! content beside it, LayoutContent and the ResizeHandle included, moves from its old place
//! to its new one at its final width, on the same curve as the panel's edge: the theme's
//! move curve, or a lift's `springs.settle` from its release velocity. A pointer that
//! catches a slide lands the glides where the layout puts them, so the pointer drives the
//! edge from there.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gpui::{App, ElementId, LayoutGlide, Pixels, Window, px};
use web_time::Instant;

use crate::animated_layout::LayoutMotion;
use crate::resizable::{ResizableProps, SlideCurve};

/// Every Layout's area is named apart.
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The area's glides last this long; each plays its own curve inside it and then rests.
const SPAN: Duration = Duration::from_secs(2);

struct Inner {
    base: usize,
    generation: Cell<u32>,
    armed: Rc<Cell<bool>>,
    running: Rc<Cell<bool>>,
    curve: Rc<RefCell<Option<SlideCurve>>>,
    /// The slide a panel last reported, and the catches it last saw.
    reported: Cell<Option<Instant>>,
    catches: Cell<u32>,
    /// The regions of the panels lent in the row, start and end, as they last rendered.
    regions: RefCell<[Option<ResizableProps>; 2]>,
}

/// A Layout's glide area for the panes that slide in it.
#[derive(Clone)]
pub(crate) struct PaneGlide(Rc<Inner>);

/// The area of the Layout rendering now, kept across frames.
pub(crate) fn use_pane_glide(window: &mut Window, cx: &mut App) -> PaneGlide {
    window
        .use_keyed_state(ElementId::Name("layout-pane-glide".into()), cx, |_, _| {
            PaneGlide(Rc::new(Inner {
                base: NEXT.fetch_add(1, Ordering::Relaxed),
                generation: Cell::new(0),
                armed: Rc::default(),
                running: Rc::default(),
                curve: Rc::default(),
                reported: Cell::new(None),
                catches: Cell::new(0),
                regions: RefCell::new([None, None]),
            }))
        })
        .read(cx)
        .clone()
}

impl PaneGlide {
    /// The area's configuration this frame.
    pub(crate) fn glide(&self, window: &Window, cx: &mut App) -> LayoutGlide {
        let motion = LayoutMotion::read(window, cx);
        let curve = self.0.curve.clone();
        LayoutGlide {
            id: ElementId::Name(
                format!("layout-panes-{}-{}", self.0.base, self.0.generation.get()).into(),
            ),
            armed: self.0.armed.clone(),
            duration: SPAN,
            ease: Rc::new(move |t| {
                let seconds = t * SPAN.as_secs_f32();
                curve.borrow().as_ref().map_or(1., |c| c.at(seconds))
            }),
            cross_fade: motion.cross_fade,
            running: self.0.running.clone(),
        }
    }

    /// A panel's slide this frame: a new one arms the area on its curve, before the frame
    /// that shows it; a pointer's catch lands every running glide.
    pub(crate) fn follow(
        &self,
        start: Option<Instant>,
        curve: Option<&SlideCurve>,
        catches: u32,
        window: &Window,
        cx: &mut App,
    ) {
        if self.0.catches.get() != catches {
            self.0.catches.set(catches);
            self.0
                .generation
                .set(self.0.generation.get().wrapping_add(1));
        }
        if let (Some(start), Some(curve)) = (start, curve)
            && self.0.reported.get() != Some(start)
        {
            self.0.reported.set(Some(start));
            if LayoutMotion::read(window, cx).animates() {
                *self.0.curve.borrow_mut() = Some(curve.clone());
                self.0.armed.set(true);
            }
        }
    }

    /// A panel at the row's start (or end) renders with `region`.
    pub(crate) fn lend(&self, at_end: bool, region: &ResizableProps) {
        self.0.regions.borrow_mut()[usize::from(at_end)] = Some(region.clone());
    }

    /// How far LayoutContent's trailing edge reaches past its final box this frame, so no
    /// gap opens beside a panel's moving edge (AST-105 FR9, upstream `playLayoutFrames`'
    /// cover): an opening end panel's edge has yet to reach it, and an opening start
    /// panel's glide carries the content's final box short of the row's end.
    pub(crate) fn trailing_reach(&self, window: &Window, cx: &mut App) -> Pixels {
        let regions = self.0.regions.borrow().clone();
        px(regions
            .iter()
            .flatten()
            .map(|r| r.opening_gap(window, cx))
            .sum())
    }
}
