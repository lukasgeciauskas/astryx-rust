//! UIDatePicker's time wheels under a finger, upstream
//! `TimeInput/TimeWheels.tsx`: a column's rest commits the time it makes,
//! brought into the bounds, and turns every other column whose row changed
//! (the AM/PM column follows the hour across 11 and 12; a bound brings the
//! rest back). Those turns are the wheels' own, so their arrival commits
//! nothing. TimeInput's and DateTimeInput's sheets show them under a finger.

use std::rc::Rc;

use gpui::{App, ElementId, Entity, IntoElement, RenderOnce, Window, div, prelude::*};
use rustryx_i18n::current as i18n;
use rustryx_i18n::date::{HourFormat, PlainTime};

use super::wheel_rules::{
    TimeBounds, TimeColumn, column_value, columns_to_turn, displayed_time, hour_rows, meridiem_of,
    meridiem_rows, minute_rows, resting_time, second_rows, turn_column, wheel_bounds,
};
use crate::wheel::{Wheel, WheelHandle, WheelOption};

/// The committed time, already inside the bounds.
pub(crate) type TimeCommit = Rc<dyn Fn(PlainTime, &mut Window, &mut App)>;

/// The time wheels under a finger.
#[derive(IntoElement)]
pub(crate) struct TimeWheels {
    pub id: ElementId,
    pub time: PlainTime,
    pub hour_format: HourFormat,
    pub seconds: bool,
    pub min: Option<PlainTime>,
    pub max: Option<PlainTime>,
    pub active: bool,
    pub on_commit: TimeCommit,
}

/// The columns' handles and the rows they turn onto by themselves.
#[derive(Default)]
struct Spins {
    handles: [WheelHandle; 4],
    /// Per column: the value a turn heads for, and the turn's id.
    turning: [Option<(i64, u64)>; 4],
    next: u64,
}

#[derive(Clone)]
struct Ctx {
    spins: Entity<Spins>,
    shown: PlainTime,
    hour_format: HourFormat,
    seconds: bool,
    bounds: TimeBounds,
    rows: [Rc<[WheelOption]>; 4],
    on_commit: TimeCommit,
}

impl Ctx {
    /// A column came to rest on `value` (upstream `restOn`).
    fn rest_on(&self, column: TimeColumn, value: i64, window: &mut Window, cx: &mut App) {
        let own_turn = self.spins.read(cx).turning[column.index()].is_some_and(|(v, _)| v == value);
        if own_turn {
            return;
        }
        let from = self.shown;
        let next = resting_time(
            turn_column(from, column, value, self.hour_format),
            self.seconds,
            &self.bounds,
        );
        // Turn the other columns first, so the commit finds them moving.
        for (turn, target) in
            columns_to_turn(from, column, value, next, self.hour_format, self.seconds)
        {
            let i = turn.index();
            let Some(index) = self.rows[i].iter().position(|row| row.value == target) else {
                continue;
            };
            let (handle, id) = self.spins.update(cx, |s, _| {
                s.next += 1;
                s.turning[i] = Some((target, s.next));
                (s.handles[i].clone(), s.next)
            });
            let spins = self.spins.clone();
            handle.spin_to(
                index,
                window,
                cx,
                Box::new(move |_, _, cx| {
                    spins.update(cx, |s, _| {
                        if s.turning[i].is_some_and(|(_, turn)| turn == id) {
                            s.turning[i] = None;
                        }
                    });
                }),
            );
        }
        if next != from {
            (self.on_commit)(next, window, cx);
        }
    }
}

impl RenderOnce for TimeWheels {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let i18n = i18n(cx);
        let bounds = wheel_bounds(self.min, self.max);
        let shown = displayed_time(self.time, self.seconds);
        let rows: [Rc<[WheelOption]>; 4] = [
            hour_rows(shown.hour, self.hour_format, &bounds).into(),
            minute_rows(shown.hour, self.seconds, &bounds).into(),
            second_rows(shown.hour, shown.minute, &bounds).into(),
            meridiem_rows(
                &i18n.t("@astryx.dateTimeInput.meridiemAM"),
                &i18n.t("@astryx.dateTimeInput.meridiemPM"),
                &bounds,
            )
            .into(),
        ];
        let spins = window.use_keyed_state(
            ElementId::Name(format!("{:?}-time-wheels", self.id).into()),
            cx,
            |_, _| Spins::default(),
        );
        let ctx = Ctx {
            spins: spins.clone(),
            shown,
            hour_format: self.hour_format,
            seconds: self.seconds,
            bounds,
            rows: rows.clone(),
            on_commit: self.on_commit.clone(),
        };
        let handles = spins.read(cx).handles.clone();
        let wheel = |column: TimeColumn, key: &str, label: &str, value: i64| {
            let ctx = ctx.clone();
            Wheel::new(
                ElementId::Name(format!("{}-{key}", self.id).into()),
                i18n.t(label),
                rows[column.index()].clone(),
                value,
                move |value, window, cx| ctx.rest_on(column, value, window, cx),
            )
            .active(self.active)
            .handle(handles[column.index()].clone())
        };
        let mut wheels = div().display_contents().child(wheel(
            TimeColumn::Hour,
            "hour",
            "@astryx.dateTimeInput.hourWheel",
            column_value(shown, TimeColumn::Hour, self.hour_format),
        ));
        wheels = wheels.child(
            wheel(
                TimeColumn::Minute,
                "minute",
                "@astryx.dateTimeInput.minuteWheel",
                i64::from(shown.minute),
            )
            .looping(true),
        );
        if self.seconds {
            wheels = wheels.child(wheel(
                TimeColumn::Second,
                "second",
                "@astryx.dateTimeInput.secondWheel",
                i64::from(shown.second),
            ));
        }
        if self.hour_format == HourFormat::H12 {
            wheels = wheels.child(wheel(
                TimeColumn::Meridiem,
                "meridiem",
                "@astryx.dateTimeInput.meridiemWheel",
                meridiem_of(shown.hour),
            ));
        }
        wheels
    }
}
