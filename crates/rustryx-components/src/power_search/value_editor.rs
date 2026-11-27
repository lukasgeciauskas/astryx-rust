//! The value editors, upstream `PowerSearchValueEditor.tsx`: one control per
//! operator value type, sharing `on_change(value, save)` (a pick that
//! completes the filter saves it at once).

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, Window,
};
use rustryx_i18n::current as i18n;

use super::format::{relative_date_options, resolve_date_zone};
use super::model::{
    DateTimeRange, DateTimeRangePart, FilterValue, OperatorValue, PowerSearchEntity,
};
use crate::calendar::Calendar;
use crate::date_input::DateInput;
use crate::date_range_input::{DateRange, DateRangeInput};
use crate::field::EntryErrorBehavior;
use crate::input::{NumberInput, TextInput};
use crate::search::{SearchItem, SearchSource, Searchable, static_source};
use crate::selector::{Selector, SelectorEntry, SelectorOptionData};
use crate::time_input::TimeInput;
use crate::tokenizer::Tokenizer;
use crate::typeahead::Typeahead;
use rustryx_i18n::TimeZoneChoice;
use rustryx_i18n::date::{PlainDate, PlainDateTime, PlainTime};
use rustryx_i18n::datetime::{
    day_of_instant, instant_in_zone, midnight_in_zone, wall_clock_in_zone,
};

pub(crate) type ValueChangeFn = Rc<dyn Fn(FilterValue, bool, &mut Window, &mut App)>;
pub(crate) type EnterFn = Rc<dyn Fn(&mut Window, &mut App)>;
type CustomChangeFn = Rc<dyn Fn(Option<SharedString>, &mut Window, &mut App)>;
type RangeFn = Rc<dyn Fn(DateRange, &mut Window, &mut App)>;

/// Upstream's 150 ms wait before a value editor searches its source.
pub(crate) const SOURCE_DEBOUNCE: Duration = Duration::from_millis(150);

/// What one value editor reads.
pub(crate) struct ValueEditorProps {
    pub id: ElementId,
    pub op: OperatorValue,
    pub value: Option<FilterValue>,
    pub on_change: ValueChangeFn,
    pub on_enter: Option<EnterFn>,
    pub max_menu_items: Option<usize>,
    pub disabled: bool,
    /// The sheet presentation: full-width controls, sheet tokenizers.
    pub sheet: bool,
    /// The sheet names a list editor by its field.
    pub field_label: Option<SharedString>,
    /// The sheet unstages a number that stops parsing (FR17).
    pub on_unstage: Option<EnterFn>,
    /// A "More" row's typed text, as a search editor's opening query
    /// (FR29, FR30, BaseTypeahead FR18, Tokenizer FR31).
    pub opening_query: Option<SharedString>,
    /// Upstream `timezoneID`: the zone date values read and write in.
    pub timezone_id: Option<SharedString>,
}

/// A source that matches nothing (a list editor without one: free text).
struct NoSource;

impl SearchSource<SearchItem> for NoSource {
    fn search(
        &self,
        _: &str,
        _: &crate::search::SearchContext<SearchItem>,
        _: &mut App,
    ) -> crate::search::SearchAnswer<SearchItem> {
        Vec::new().into()
    }

    fn bootstrap(&self, _: &mut App) -> crate::search::SearchAnswer<SearchItem> {
        Vec::new().into()
    }
}

fn id(base: &ElementId, part: &str) -> ElementId {
    ElementId::Name(format!("{base}-{part}").into())
}

/// The editor for `props.op`, or none (`empty` and `nested` have none here).
pub(crate) fn value_editor(
    p: ValueEditorProps,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let t18 = i18n(cx);
    let full = p.sheet;
    let on_change = p.on_change.clone();
    let presentation = if p.sheet {
        crate::overlay::PopoverPresentation::BottomSheet
    } else {
        crate::overlay::PopoverPresentation::Popover
    };
    let el: AnyElement = match &p.op {
        OperatorValue::Empty | OperatorValue::Nested => return None,
        OperatorValue::String { search_source, .. } => {
            let current = match &p.value {
                Some(FilterValue::String(s)) => s.clone(),
                _ => SharedString::default(),
            };
            match search_source {
                Some(source) => {
                    let selected = (!current.is_empty())
                        .then(|| SearchItem::new(current.clone(), current.clone()));
                    let mut ta = Typeahead::from_rc(
                        id(&p.id, "value"),
                        t18.t("@astryx.powersearch.valueEditor.value"),
                        source.clone(),
                    )
                    .label_hidden(true)
                    .disabled(p.disabled)
                    .value(selected)
                    .placeholder(t18.t("@astryx.powersearch.valueEditor.searchPlaceholder"))
                    .debounce(SOURCE_DEBOUNCE)
                    .on_change(move |item, window, cx| match item {
                        Some(item) => on_change(
                            FilterValue::String(Searchable::label(&item)),
                            true,
                            window,
                            cx,
                        ),
                        None => on_change(
                            FilterValue::String(SharedString::default()),
                            false,
                            window,
                            cx,
                        ),
                    });
                    if let Some(n) = p.max_menu_items {
                        ta = ta.max_menu_items(n);
                    }
                    if let Some(q) = p.opening_query.clone() {
                        ta = ta.opening_query(q);
                    }
                    if full {
                        ta = ta.w_full();
                    }
                    ta.into_any_element()
                }
                None => {
                    let mut input = TextInput::new(
                        id(&p.id, "value"),
                        t18.t("@astryx.powersearch.valueEditor.value"),
                        current,
                    )
                    .label_hidden(true)
                    .disabled(p.disabled)
                    .placeholder(t18.t("@astryx.powersearch.valueEditor.enterValuePlaceholder"))
                    .on_change(move |text, window, cx| {
                        on_change(FilterValue::String(text), false, window, cx)
                    });
                    if let Some(enter) = p.on_enter.clone() {
                        input = input.on_enter(move |window, cx| enter(window, cx));
                    }
                    if full {
                        input = input.w_full();
                    }
                    input.into_any_element()
                }
            }
        }
        OperatorValue::StringList {
            search_source,
            arbitrary_allowed,
            ..
        } => {
            let current: Vec<SearchItem> = match &p.value {
                Some(FilterValue::StringList(items)) => items
                    .iter()
                    .map(|v| SearchItem::new(v.clone(), v.clone()))
                    .collect(),
                _ => Vec::new(),
            };
            let source: Rc<dyn SearchSource<SearchItem>> =
                search_source.clone().unwrap_or_else(|| Rc::new(NoSource));
            let creates = *arbitrary_allowed || search_source.is_none();
            let label = match (&p.field_label, p.sheet) {
                (Some(field), true) => field.clone(),
                _ => t18.t("@astryx.powersearch.valueEditor.values"),
            };
            let mut tk = Tokenizer::from_rc(id(&p.id, "values"), label, source, current)
                .label_hidden(true)
                .disabled(p.disabled)
                .placeholder(t18.t("@astryx.powersearch.valueEditor.addValuesPlaceholder"))
                .debounce(if search_source.is_some() {
                    SOURCE_DEBOUNCE
                } else {
                    Duration::ZERO
                })
                .presentation(presentation)
                .on_change(move |items: Vec<SearchItem>, _, window, cx| {
                    on_change(
                        FilterValue::StringList(items.iter().map(Searchable::label).collect()),
                        false,
                        window,
                        cx,
                    )
                });
            if creates {
                tk = tk.create(|text| SearchItem::new(text.to_string(), text.to_string()));
            }
            if let Some(n) = p.max_menu_items {
                tk = tk.max_menu_items(n);
            }
            if let Some(q) = p.opening_query.clone() {
                tk = tk.opening_query(q);
            }
            if full {
                tk = tk.w_full();
            }
            tk.into_any_element()
        }
        OperatorValue::Integer { min, max, units } => number_editor(
            &p,
            true,
            min.map(|v| v as f64),
            max.map(|v| v as f64),
            units.clone(),
            cx,
        ),
        OperatorValue::Float { min, max, units } => {
            number_editor(&p, false, *min, *max, units.clone(), cx)
        }
        // Upstream `TimeEditor`.
        OperatorValue::Time { min, max } => {
            let current = match &p.value {
                Some(FilterValue::Time(t)) => PlainTime::from_iso(t),
                _ => None,
            };
            let mut input = TimeInput::new(
                id(&p.id, "value"),
                t18.t("@astryx.powersearch.valueEditor.time"),
                current,
            )
            .label_hidden(true)
            .entry_error_behavior(EntryErrorBehavior::Complete)
            .disabled(p.disabled)
            .on_change(move |value, window, cx| {
                if let Some(value) = value {
                    on_change(
                        FilterValue::Time(value.to_iso(false).into()),
                        false,
                        window,
                        cx,
                    )
                }
            });
            if let Some(min) = min.as_deref().and_then(PlainTime::from_iso) {
                input = input.min(min);
            }
            if let Some(max) = max.as_deref().and_then(PlainTime::from_iso) {
                input = input.max(max);
            }
            if full {
                input = input.width(gpui::relative(1.));
            }
            input.into_any_element()
        }
        // Upstream `DateAbsoluteEditor`: a calendar day in its zone (FR25),
        // or an instant's wall clock beside it (FR26).
        OperatorValue::DateAbsolute { date_only } => {
            let label = t18.t("@astryx.powersearch.valueEditor.date");
            let unix = match &p.value {
                Some(FilterValue::DateAbsolute { unix_seconds }) => Some(*unix_seconds),
                _ => None,
            };
            if *date_only == Some(false) {
                let zone = resolve_date_zone(p.timezone_id.as_deref(), false);
                date_time_editor(&p, label, unix, zone, window, cx)
            } else {
                let zone = resolve_date_zone(p.timezone_id.as_deref(), true);
                let value = unix.map(|s| day_of_instant(s, &zone));
                let mut input = DateInput::new(id(&p.id, "value"), label, value)
                    .label_hidden(true)
                    .entry_error_behavior(EntryErrorBehavior::Complete)
                    .disabled(p.disabled)
                    .on_change(move |value, window, cx| {
                        if let Some(day) = value {
                            let unix_seconds = midnight_in_zone(day, &zone);
                            on_change(
                                FilterValue::DateAbsolute { unix_seconds },
                                false,
                                window,
                                cx,
                            )
                        }
                    });
                if full {
                    input = input.width(gpui::relative(1.));
                }
                input.into_any_element()
            }
        }
        // Upstream `DateRangeEditor`: each part a calendar day (FR25).
        OperatorValue::DateRange { .. } => {
            let zone = resolve_date_zone(p.timezone_id.as_deref(), true);
            let now = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map_or(0., |d| d.as_secs_f64());
            let current = match &p.value {
                Some(FilterValue::DateRange(range)) => Some(DateRange::new(
                    day_of_instant(range.start.resolve(now), &zone),
                    day_of_instant(range.end.resolve(now), &zone),
                )),
                _ => None,
            };
            let set: RangeFn = {
                let (on_change, zone) = (on_change.clone(), zone.clone());
                Rc::new(move |range: DateRange, window: &mut Window, cx: &mut App| {
                    let part = |day| DateTimeRangePart::Absolute {
                        unix_seconds: midnight_in_zone(day, &zone),
                    };
                    on_change(
                        FilterValue::DateRange(DateTimeRange {
                            start: part(range.start),
                            end: part(range.end),
                        }),
                        false,
                        window,
                        cx,
                    )
                })
            };
            if p.sheet {
                // A sheet opens no popover over itself: an inline calendar,
                // taking no input while disabled.
                let mut calendar = Calendar::new(id(&p.id, "value")).range_value(current);
                if !p.disabled {
                    calendar =
                        calendar.on_range_change(move |range, window, cx| set(range, window, cx));
                }
                calendar.into_any_element()
            } else {
                gpui::div()
                    // The range input's own Enter must not reach the
                    // editor's save shortcut first.
                    .on_key_down(|event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" {
                            cx.stop_propagation();
                        }
                    })
                    .child(
                        DateRangeInput::new(
                            id(&p.id, "value"),
                            t18.t("@astryx.powersearch.valueEditor.dateRange"),
                            current,
                        )
                        .label_hidden(true)
                        .disabled(p.disabled)
                        .clear(false)
                        .on_change(move |range, window, cx| {
                            if let Some(range) = range {
                                set(range, window, cx)
                            }
                        }),
                    )
                    .into_any_element()
            }
        }
        OperatorValue::DateRelative {
            past_allowed,
            future_allowed,
        } => {
            let options: Vec<SelectorEntry> =
                relative_date_options(*past_allowed, *future_allowed, &t18)
                    .into_iter()
                    .map(|(value, label)| {
                        SelectorEntry::Option(SelectorOptionData::new(value).label(label))
                    })
                    .collect();
            let current = match &p.value {
                Some(FilterValue::DateRelative(v)) => Some(v.clone()),
                _ => None,
            };
            Selector::new(
                id(&p.id, "value"),
                t18.t("@astryx.powersearch.valueEditor.relativeDate"),
                options,
            )
            .label_hidden(true)
            .disabled(p.disabled)
            .value(current)
            .on_change(move |value, window, cx| {
                if let Some(value) = value {
                    on_change(FilterValue::DateRelative(value), true, window, cx)
                }
            })
            .into_any_element()
        }
        OperatorValue::Enum { values } => {
            let options: Vec<SelectorEntry> = values
                .iter()
                .map(|v| {
                    SelectorEntry::Option(
                        SelectorOptionData::new(v.value.clone()).label(v.label.clone()),
                    )
                })
                .collect();
            let current = match &p.value {
                Some(FilterValue::Enum(v)) => Some(v.clone()),
                _ => None,
            };
            Selector::new(
                id(&p.id, "value"),
                t18.t("@astryx.powersearch.valueEditor.value"),
                options,
            )
            .label_hidden(true)
            .disabled(p.disabled)
            .value(current)
            .on_change(move |value, window, cx| {
                if let Some(value) = value {
                    on_change(FilterValue::Enum(value), true, window, cx)
                }
            })
            .into_any_element()
        }
        OperatorValue::EnumList { values } => {
            let items: Vec<SearchItem> = values
                .iter()
                .map(|v| SearchItem::new(v.value.clone(), v.label.clone()))
                .collect();
            let current: Vec<SearchItem> = match &p.value {
                Some(FilterValue::EnumList(chosen)) => chosen
                    .iter()
                    .map(|v| {
                        let label = values
                            .iter()
                            .find(|e| &e.value == v)
                            .map_or(v.clone(), |e| e.label.clone());
                        SearchItem::new(v.clone(), label)
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let count = items.len();
            Tokenizer::from_rc(
                id(&p.id, "values"),
                t18.t("@astryx.powersearch.valueEditor.values"),
                Rc::new(static_source(items)),
                current,
            )
            .label_hidden(true)
            .disabled(p.disabled)
            .placeholder(t18.t("@astryx.powersearch.valueEditor.selectValuesPlaceholder"))
            .entries_on_focus(true)
            .debounce(Duration::ZERO)
            .max_menu_items(count.max(1))
            .presentation(crate::overlay::PopoverPresentation::Popover)
            .on_change(move |items: Vec<SearchItem>, _, window, cx| {
                on_change(
                    FilterValue::EnumList(items.iter().map(Searchable::id).collect()),
                    false,
                    window,
                    cx,
                )
            })
            .into_any_element()
        }
        OperatorValue::EntityList {
            search_source,
            render_item,
            ..
        } => {
            let current: Vec<SearchItem> = match &p.value {
                Some(FilterValue::EntityList(entities)) => entities
                    .iter()
                    .map(|e| {
                        let item = SearchItem::new(e.id.clone(), e.label.clone());
                        match &e.photo {
                            Some(photo) => item.data(EntityPhoto(photo.clone())),
                            None => item,
                        }
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let source: Rc<dyn SearchSource<SearchItem>> =
                search_source.clone().unwrap_or_else(|| Rc::new(NoSource));
            let label = match (&p.field_label, p.sheet) {
                (Some(field), true) => field.clone(),
                _ => t18.t("@astryx.powersearch.valueEditor.entities"),
            };
            let mut tk = Tokenizer::from_rc(id(&p.id, "entities"), label, source, current)
                .label_hidden(true)
                .disabled(p.disabled)
                .placeholder(t18.t("@astryx.powersearch.valueEditor.searchPlaceholder"))
                .debounce(if search_source.is_some() {
                    SOURCE_DEBOUNCE
                } else {
                    Duration::ZERO
                })
                .presentation(presentation)
                .on_change(move |items: Vec<SearchItem>, _, window, cx| {
                    let entities = items
                        .iter()
                        .map(|i| PowerSearchEntity {
                            photo: i.data_as::<EntityPhoto>().map(|p| p.0.clone()),
                            id: Searchable::id(i),
                            label: Searchable::label(i),
                        })
                        .collect();
                    on_change(FilterValue::EntityList(entities), false, window, cx)
                });
            if let Some(render) = render_item.clone() {
                tk = tk.render_item(move |item, window, cx| render(item, window, cx));
            }
            if let Some(n) = p.max_menu_items {
                tk = tk.max_menu_items(n);
            }
            if let Some(q) = p.opening_query.clone() {
                tk = tk.opening_query(q);
            }
            if full {
                tk = tk.w_full();
            }
            tk.into_any_element()
        }
        OperatorValue::Custom { editor, .. } => {
            let current = match &p.value {
                Some(FilterValue::Custom(v)) => Some(v.clone()),
                _ => None,
            };
            let change: CustomChangeFn = Rc::new(move |value, window, cx| {
                if let Some(value) = value {
                    on_change(FilterValue::Custom(value), false, window, cx)
                }
            });
            editor(
                current,
                change,
                t18.t("@astryx.powersearch.valueEditor.enterValuePlaceholder"),
                p.disabled,
                window,
                cx,
            )
        }
    };
    Some(el)
}

/// An entity's photo, carried on its search item.
#[derive(Clone, Debug)]
pub(crate) struct EntityPhoto(pub SharedString);

fn number_editor(
    p: &ValueEditorProps,
    integer: bool,
    min: Option<f64>,
    max: Option<f64>,
    units: Option<SharedString>,
    cx: &mut App,
) -> AnyElement {
    let t18 = i18n(cx);
    let current = match &p.value {
        Some(FilterValue::Integer(n)) if integer => Some(*n as f64),
        Some(FilterValue::Float(n)) if !integer => Some(*n),
        _ => None,
    };
    let to_value = move |n: f64| {
        if integer {
            FilterValue::Integer(n.round() as i64)
        } else {
            FilterValue::Float(n)
        }
    };
    let on_change = p.on_change.clone();
    let mut input = NumberInput::new(
        id(&p.id, "value"),
        t18.t("@astryx.powersearch.valueEditor.value"),
        current,
    )
    .label_hidden(true)
    .disabled(p.disabled)
    .placeholder(t18.t("@astryx.powersearch.valueEditor.enterNumberPlaceholder"))
    .integer_only(integer)
    .on_change(move |value, window, cx| {
        if let Some(n) = value {
            on_change(to_value(n), false, window, cx)
        }
    });
    if let Some(min) = min {
        input = input.min(min);
    }
    if let Some(max) = max {
        input = input.max(max);
    }
    if let Some(units) = units {
        input = input.units(units);
    }
    if let Some(enter) = p.on_enter.clone() {
        input = input.on_enter(move |window, cx| enter(window, cx));
    }
    if p.sheet {
        // FR17: every parseable keystroke stages; an empty field unstages.
        let (stage, unstage) = (p.on_change.clone(), p.on_unstage.clone());
        input = input
            .w_full()
            .on_input(move |value, window, cx| match value {
                Some(n) => stage(to_value(n), false, window, cx),
                None => {
                    if let Some(f) = &unstage {
                        f(window, cx)
                    }
                }
            });
    }
    input.into_any_element()
}

/// The parts of an instant a date-and-time editor holds until both exist.
#[derive(Clone, Copy, Default)]
struct PendingParts {
    date: Option<PlainDate>,
    time: Option<PlainTime>,
}

/// Upstream `DateTimeEditor`: the instant's wall clock in `zone`, as a date
/// beside a time; a part picked first waits for the other (FR26).
fn date_time_editor(
    p: &ValueEditorProps,
    label: SharedString,
    unix_seconds: Option<i64>,
    zone: TimeZoneChoice,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let t18 = i18n(cx);
    let theme = rustryx_theme::current(window, cx);
    let parts = unix_seconds.map(|s| wall_clock_in_zone(s, &zone));
    let pending = window.use_keyed_state(id(&p.id, "pending"), cx, |_, _| PendingParts::default());
    let held = *pending.read(cx);
    let date = parts.map(|v| v.date).or(held.date);
    let time = parts.map(|v| v.time).or(held.time);
    let commit = {
        let on_change = p.on_change.clone();
        move |date: Option<PlainDate>,
              time: Option<PlainTime>,
              window: &mut Window,
              cx: &mut App| {
            if let (Some(date), Some(time)) = (date, time) {
                let unix_seconds = instant_in_zone(PlainDateTime::new(date, time), &zone);
                on_change(
                    FilterValue::DateAbsolute { unix_seconds },
                    false,
                    window,
                    cx,
                );
            }
        }
    };
    let (date_commit, time_commit) = (commit.clone(), commit);
    let (date_pending, time_pending) = (pending.clone(), pending);
    let mut date_input = DateInput::new(id(&p.id, "date"), label, date)
        .label_hidden(true)
        .entry_error_behavior(EntryErrorBehavior::Complete)
        .disabled(p.disabled)
        .on_change(move |value, window, cx| {
            date_pending.update(cx, |s, _| s.date = value);
            date_commit(value, time, window, cx);
        });
    let mut time_input = TimeInput::new(
        id(&p.id, "time"),
        t18.t("@astryx.powersearch.valueEditor.time"),
        time,
    )
    .label_hidden(true)
    .entry_error_behavior(EntryErrorBehavior::Complete)
    .disabled(p.disabled)
    .on_change(move |value, window, cx| {
        time_pending.update(cx, |s, _| s.time = value);
        time_commit(date, value, window, cx);
    });
    if p.sheet {
        date_input = date_input.width(gpui::relative(1.));
        time_input = time_input.width(gpui::relative(1.));
    }
    gpui::div()
        .flex()
        .flex_wrap()
        .gap(theme.tokens().spacing.s2)
        .child(date_input)
        .child(time_input)
        .into_any_element()
}
