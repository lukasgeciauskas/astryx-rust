//! Times of day, upstream `utils/timeParser.ts` and `utils/timeFormat.ts`.
//!
//! The hour cycle, digits, separator and day-period words come from the
//! locale's time format, so a field reads like Timestamp; a `-u-hc-`
//! extension on the locale carries a person's hour cycle. The parser reads
//! back what the formatter writes, and the rest goes through the
//! English-tolerant parser, so "2:30 PM" and "14:30" work in every locale.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use icu_datetime::fieldsets::T;
use icu_datetime::input::Time;
use icu_datetime::preferences::HourCycle;
use icu_datetime::{DateTimeFormatterPreferences, NoCalendarFormatter};

use super::format::{DatePart, PartKind, parts_of};
use crate::number::{is_invisible, nfkc, to_ascii_digits};

/// A time of day, upstream `ParsedTime` (and the `ISOTimeString` it reads
/// and writes). Ordered by time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlainTime {
    /// 0 through 23.
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl PlainTime {
    /// A time, or `None` for an hour, minute or second out of range.
    pub fn new(hour: u8, minute: u8, second: u8) -> Option<Self> {
        (hour <= 23 && minute <= 59 && second <= 59).then_some(Self {
            hour,
            minute,
            second,
        })
    }

    /// Reads `HH:MM` or `HH:MM:SS` as upstream's `parseISOTime` does: each
    /// field read up to its first non-digit.
    pub fn from_iso(text: &str) -> Option<Self> {
        let parts: Vec<&str> = text.split(':').collect();
        if !(2..=3).contains(&parts.len()) {
            return None;
        }
        let int = |s: &str| -> Option<u32> {
            let s = s.trim_start();
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        };
        let hour = int(parts[0])?;
        let minute = int(parts[1])?;
        let second = if parts.len() == 3 { int(parts[2])? } else { 0 };
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        Some(Self {
            hour: hour as u8,
            minute: minute as u8,
            second: second as u8,
        })
    }

    /// Reads a machine time strictly, upstream `createISOTimeString`: digits
    /// only, `H:M` or `H:M:S`, surrounding space and a dropped fraction of a
    /// second allowed. Returns the time and whether it had seconds.
    pub fn parse_strict(text: &str) -> Option<(Self, bool)> {
        let trimmed = text.trim();
        let mut parts = trimmed.split(':');
        let (h, m) = (parts.next()?, parts.next()?);
        let s = parts.next();
        if parts.next().is_some() {
            return None;
        }
        let digits = |p: &str| (1..=2).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit());
        if !digits(h) || !digits(m) {
            return None;
        }
        if let Some(s) = s {
            let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
            if !digits(whole)
                || (s.contains('.')
                    && (fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit())))
            {
                return None;
            }
        }
        let time = Self::from_iso(trimmed)?;
        Some((time, s.is_some()))
    }

    /// `HH:MM`, or `HH:MM:SS` with seconds (`formatISOTime`).
    pub fn to_iso(self, include_seconds: bool) -> String {
        if include_seconds {
            format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
        } else {
            format!("{:02}:{:02}", self.hour, self.minute)
        }
    }

    /// Seconds since midnight.
    pub fn seconds_of_day(self) -> u32 {
        self.hour as u32 * 3600 + self.minute as u32 * 60 + self.second as u32
    }

    /// The time of day in the system's time zone, or the time
    /// [`set_time_now`] holds.
    pub fn now() -> Self {
        if let Some(now) = NOW.with(std::cell::Cell::get) {
            return now;
        }
        let seconds = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let timestamp =
            jiff::Timestamp::from_second(seconds).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
        let time = timestamp
            .to_zoned_datetime(jiff::tz::TimeZone::system())
            .time();
        Self {
            hour: time.hour() as u8,
            minute: time.minute() as u8,
            second: time.second() as u8,
        }
    }

    /// The time without its seconds when `include_seconds` is false, as a
    /// round trip through `formatISOTime` leaves it.
    pub fn truncated(self, include_seconds: bool) -> Self {
        if include_seconds {
            self
        } else {
            Self { second: 0, ..self }
        }
    }
}

impl fmt::Display for PlainTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.second == 0 {
            write!(f, "{:02}:{:02}", self.hour, self.minute)
        } else {
            write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
        }
    }
}

thread_local! {
    static NOW: std::cell::Cell<Option<PlainTime>> = const { std::cell::Cell::new(None) };
}

/// Holds the time [`PlainTime::now`] answers on this thread, or releases it
/// with `None`: for tests and screenshots.
pub fn set_time_now(now: Option<PlainTime>) {
    NOW.with(|cell| cell.set(now));
}

/// A date and a time of day, upstream `ISODateTimeString`
/// (`2026-03-21T14:30` or with seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlainDateTime {
    pub date: super::PlainDate,
    pub time: PlainTime,
}

impl PlainDateTime {
    pub fn new(date: super::PlainDate, time: PlainTime) -> Self {
        Self { date, time }
    }

    /// Reads `YYYY-MM-DDTHH:MM` or `YYYY-MM-DDTHH:MM:SS`.
    pub fn from_iso(text: &str) -> Option<Self> {
        let (date, time) = text.trim().split_once('T')?;
        Some(Self {
            date: super::PlainDate::from_iso(date)?,
            time: PlainTime::from_iso(time)?,
        })
    }

    /// `YYYY-MM-DDTHH:MM`, or with seconds.
    pub fn to_iso(self, include_seconds: bool) -> String {
        format!("{}T{}", self.date, self.time.to_iso(include_seconds))
    }
}

impl fmt::Display for PlainDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}T{}", self.date, self.time)
    }
}

/// "2:30 PM" for 14:30 (`formatDisplayTime12h`).
pub fn format_display_time_12h(time: PlainTime, include_seconds: bool) -> String {
    let hour = match time.hour {
        0 => 12,
        h if h > 12 => h - 12,
        h => h,
    };
    let meridiem = if time.hour < 12 { "AM" } else { "PM" };
    if include_seconds {
        format!("{hour}:{:02}:{:02} {meridiem}", time.minute, time.second)
    } else {
        format!("{hour}:{:02} {meridiem}", time.minute)
    }
}

/// "14:30" (`formatDisplayTime24h`).
pub fn format_display_time_24h(time: PlainTime, include_seconds: bool) -> String {
    time.to_iso(include_seconds)
}

/// Reads typed text as a time (`parseTimeInput`): "2:30 PM", "2:30pm",
/// "2 p.m.", "14:30", "1430", "143000", "2pm". Without `include_seconds` the
/// seconds are dropped.
pub fn parse_time_input(input: &str, include_seconds: bool) -> Option<PlainTime> {
    if input.is_empty() {
        return None;
    }
    let trimmed = input.trim().to_lowercase();
    let (body, meridiem) = strip_meridiem(&trimmed);
    let body = body.trim();
    let to_24 = |hour: u32| -> Option<u32> {
        match meridiem {
            None => Some(hour),
            Some(_) if !(1..=12).contains(&hour) => None,
            Some(true) if hour != 12 => Some(hour + 12),
            Some(false) if hour == 12 => Some(0),
            Some(_) => Some(hour),
        }
    };
    let make = |h: u32, m: u32, s: u32| {
        PlainTime::new(h as u8, m as u8, s as u8)
            .filter(|_| h <= 23 && m <= 59 && s <= 59)
            .map(|t| t.truncated(include_seconds))
    };
    let all_digits = !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit());
    if all_digits && body.len() <= 2 {
        let hour: u32 = body.parse().ok()?;
        return make(to_24(hour)?, 0, 0);
    }
    if all_digits && body.len() == 4 {
        return make(to_24(body[..2].parse().ok()?)?, body[2..4].parse().ok()?, 0);
    }
    if all_digits && body.len() == 6 {
        return make(
            to_24(body[..2].parse().ok()?)?,
            body[2..4].parse().ok()?,
            body[4..6].parse().ok()?,
        );
    }
    let parts: Vec<&str> = body.split(':').collect();
    if (2..=3).contains(&parts.len()) {
        let int = |s: &str| -> Option<u32> {
            let s = s.trim_start();
            let (sign, rest) = match s.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, s.strip_prefix('+').unwrap_or(s)),
            };
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            let value: u32 = digits.parse().ok()?;
            if sign && value != 0 {
                Some(u32::MAX)
            } else {
                Some(value)
            }
        };
        let hour = int(parts[0])?;
        let minute = int(parts[1])?;
        let second = if parts.len() == 3 { int(parts[2])? } else { 0 };
        if minute > 59 || second > 59 {
            return None;
        }
        let hour = match meridiem {
            Some(_) if !(1..=12).contains(&hour) => return None,
            Some(_) => to_24(hour)?,
            None if hour > 23 => return None,
            None => hour,
        };
        return make(hour, minute, second);
    }
    None
}

/// Splits a trailing day period ("pm", "p.m.", "p", "am") off typed text:
/// `Some(true)` for PM.
fn strip_meridiem(text: &str) -> (&str, Option<bool>) {
    // /\s*[ap]\.?m?\.?\s*$/
    let trimmed = text.trim_end();
    let bytes = trimmed.as_bytes();
    let mut end = bytes.len();
    if end > 0 && bytes[end - 1] == b'.' {
        end -= 1;
    }
    if end > 0 && bytes[end - 1] == b'm' {
        end -= 1;
    }
    if end > 0 && bytes[end - 1] == b'.' {
        end -= 1;
    }
    if end == 0 {
        return (text, None);
    }
    let period = match bytes[end - 1] {
        b'p' => true,
        b'a' => false,
        _ => return (text, None),
    };
    (trimmed[..end - 1].trim_end(), Some(period))
}

/// Orders two optional times, an absent time first (`compareTime`).
pub fn compare_time(a: Option<PlainTime>, b: Option<PlainTime>) -> Ordering {
    a.map(PlainTime::seconds_of_day)
        .cmp(&b.map(PlainTime::seconds_of_day))
}

/// Whether a time lies within the bounds, both included (`isTimeInRange`).
pub fn is_time_in_range(time: PlainTime, min: Option<PlainTime>, max: Option<PlainTime>) -> bool {
    min.is_none_or(|min| time >= min) && max.is_none_or(|max| time <= max)
}

/// A time clamped into the bounds (`clampTime`). Without seconds, a bound
/// between whole minutes gives the nearest whole minute inside the range,
/// or the bound itself when no whole minute lies inside it.
pub fn clamp_time(
    time: PlainTime,
    min: Option<PlainTime>,
    max: Option<PlainTime>,
    include_seconds: bool,
) -> PlainTime {
    if let Some(min) = min
        && time < min
    {
        return on_whole_minute(min, true, include_seconds, max);
    }
    if let Some(max) = max
        && time > max
    {
        return on_whole_minute(max, false, include_seconds, min);
    }
    time
}

fn on_whole_minute(
    bound: PlainTime,
    up: bool,
    include_seconds: bool,
    other: Option<PlainTime>,
) -> PlainTime {
    if include_seconds || bound.second == 0 {
        return bound;
    }
    let minutes = bound.hour as u32 * 60 + bound.minute as u32 + u32::from(up);
    if minutes > 23 * 60 + 59 {
        return bound;
    }
    let rounded = PlainTime {
        hour: (minutes / 60) as u8,
        minute: (minutes % 60) as u8,
        second: 0,
    };
    match other {
        Some(other) if (up && rounded > other) || (!up && rounded < other) => bound,
        _ => rounded,
    }
}

/// A time moved by whole minutes, wrapping at midnight (`adjustTime`).
pub fn adjust_time(time: PlainTime, delta_minutes: i64, include_seconds: bool) -> PlainTime {
    let total = (time.hour as i64 * 60 + time.minute as i64 + delta_minutes).rem_euclid(24 * 60);
    PlainTime {
        hour: (total / 60) as u8,
        minute: (total % 60) as u8,
        second: if include_seconds { time.second } else { 0 },
    }
}

/// A time field's hour format, upstream `HourFormat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HourFormat {
    /// The locale's day period: "2:30 PM".
    H12,
    /// "14:30".
    H24,
}

struct LocaleTimeSymbols {
    hour_format: HourFormat,
    separators: Vec<String>,
    edge_words: Vec<String>,
    day_periods: Vec<(String, bool)>,
    twelve_hour_is_ambiguous: bool,
}

type TimeFormatterKey = (String, Option<HourFormat>, bool);
type TimeFormatterCache = HashMap<TimeFormatterKey, Option<Rc<NoCalendarFormatter<T>>>>;

thread_local! {
    static SYMBOLS: RefCell<HashMap<String, Rc<LocaleTimeSymbols>>> = RefCell::new(HashMap::new());
    static TIME_FORMATTERS: RefCell<TimeFormatterCache> = RefCell::new(HashMap::new());
}

fn time_formatter(
    locale: &str,
    hour_format: Option<HourFormat>,
    include_seconds: bool,
) -> Option<Rc<NoCalendarFormatter<T>>> {
    let key = (locale.to_string(), hour_format, include_seconds);
    TIME_FORMATTERS.with(|cache| {
        cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let parsed = crate::locale::parse_locale(locale);
                let mut prefs = DateTimeFormatterPreferences::from(&parsed);
                if let Some(format) = hour_format {
                    prefs.hour_cycle = Some(match format {
                        HourFormat::H12 => HourCycle::H12,
                        HourFormat::H24 => HourCycle::H23,
                    });
                }
                let set = if include_seconds { T::hms() } else { T::hm() };
                NoCalendarFormatter::try_new(prefs, set)
                    .or_else(|_| NoCalendarFormatter::try_new(Default::default(), set))
                    .ok()
                    .map(Rc::new)
            })
            .clone()
    })
}

fn time_parts(
    locale: &str,
    hour_format: Option<HourFormat>,
    include_seconds: bool,
    time: PlainTime,
) -> Vec<DatePart> {
    let Some(formatter) = time_formatter(locale, hour_format, include_seconds) else {
        return Vec::new();
    };
    let Ok(input) = Time::try_new(time.hour, time.minute, time.second, 0) else {
        return Vec::new();
    };
    parts_of(&formatter.format(&input))
}

fn normalize_time_text(text: &str) -> String {
    let folded: String = nfkc(text)
        .chars()
        .filter(|&ch| !is_invisible(ch))
        .map(|ch| {
            if matches!(ch, '\u{00a0}' | '\u{2007}' | '\u{202f}') {
                ' '
            } else {
                ch
            }
        })
        .collect();
    to_ascii_digits(&folded).unwrap_or(folded).to_lowercase()
}

fn symbols(locale: &str) -> Rc<LocaleTimeSymbols> {
    if let Some(s) = SYMBOLS.with(|c| c.borrow().get(locale).cloned()) {
        return s;
    }
    let at = |h, m, s| PlainTime {
        hour: h,
        minute: m,
        second: s,
    };
    let hour_format = if time_parts(locale, None, false, at(13, 0, 0))
        .iter()
        .any(|p| p.kind == PartKind::DayPeriod)
    {
        HourFormat::H12
    } else {
        HourFormat::H24
    };
    let is_number =
        |p: &DatePart| matches!(p.kind, PartKind::Hour | PartKind::Minute | PartKind::Second);
    let mut separators: Vec<String> = Vec::new();
    let mut edge_words: Vec<String> = Vec::new();
    for cycle in [HourFormat::H12, HourFormat::H24] {
        for with_seconds in [false, true] {
            let parts = time_parts(locale, Some(cycle), with_seconds, at(13, 30, 5));
            let (Some(first), Some(last)) = (
                parts.iter().position(is_number),
                parts.iter().rposition(is_number),
            ) else {
                continue;
            };
            for (index, part) in parts.iter().enumerate() {
                if part.kind != PartKind::Literal {
                    continue;
                }
                let text = normalize_time_text(&part.value).trim().to_string();
                if text.is_empty() || text == ":" {
                    continue;
                }
                let set = if index > first && index < last {
                    &mut separators
                } else {
                    &mut edge_words
                };
                if !set.contains(&text) {
                    set.push(text);
                }
            }
        }
    }
    edge_words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    let mut sides: Vec<(String, Vec<bool>)> = Vec::new();
    for hour in 0..24 {
        let parts = time_parts(locale, Some(HourFormat::H12), false, at(hour, 0, 0));
        let Some(word) = parts.iter().find(|p| p.kind == PartKind::DayPeriod) else {
            continue;
        };
        let key = normalize_time_text(&word.value).trim().to_string();
        if key.is_empty() {
            continue;
        }
        let pm = hour >= 12;
        match sides.iter_mut().find(|(w, _)| *w == key) {
            Some((_, seen)) if !seen.contains(&pm) => seen.push(pm),
            Some(_) => {}
            None => sides.push((key, vec![pm])),
        }
    }
    let mut day_periods: Vec<(String, bool)> = sides
        .into_iter()
        .filter(|(_, seen)| seen.len() == 1)
        .map(|(word, seen)| (word, seen[0]))
        .collect();
    day_periods.sort_by_key(|(w, _)| std::cmp::Reverse(w.chars().count()));
    let text = |t| {
        normalize_time_text(&super::format::join_parts(&time_parts(
            locale,
            Some(HourFormat::H12),
            false,
            t,
        )))
    };
    let twelve_hour_is_ambiguous = text(at(2, 0, 0)) == text(at(14, 0, 0));
    let symbols = Rc::new(LocaleTimeSymbols {
        hour_format,
        separators,
        edge_words,
        day_periods,
        twelve_hour_is_ambiguous,
    });
    SYMBOLS.with(|c| c.borrow_mut().insert(locale.to_string(), symbols.clone()));
    symbols
}

/// The hour format a field shows: the caller's choice, else the locale's
/// hour cycle, a `-u-hc-` extension included (`resolveHourFormat`).
pub fn resolve_hour_format(locale: &str, hour_format: Option<HourFormat>) -> HourFormat {
    hour_format.unwrap_or_else(|| symbols(locale).hour_format)
}

/// A time as the locale writes it in an hour format (`formatLocaleTime`):
/// "2:30 PM" in `en-US`, "14:30" in `de-DE`, "오후 2:30" in `ko-KR`.
pub fn format_locale_time(
    time: PlainTime,
    locale: &str,
    hour_format: HourFormat,
    include_seconds: bool,
) -> String {
    let format_locale =
        if hour_format == HourFormat::H12 && symbols(locale).twelve_hour_is_ambiguous {
            "en"
        } else {
            locale
        };
    super::format::join_parts(&time_parts(
        format_locale,
        Some(hour_format),
        include_seconds,
        time,
    ))
    .replace(['\u{00a0}', '\u{2007}', '\u{202f}'], " ")
}

/// Typed text in the English-tolerant parser's form (`readLocaleTime`).
fn read_locale_time(input: &str, locale: &str) -> Option<String> {
    if input.is_empty() {
        return None;
    }
    let symbols = symbols(locale);
    let mut text = normalize_time_text(input).trim().to_string();
    let boundary = |c: Option<char>| c.is_none_or(|c| c.is_ascii_digit() || c.is_whitespace());
    let mut is_pm: Option<bool> = None;
    let strip = |text: &mut String, word: &str| -> bool {
        if text.starts_with(word) && boundary(text[word.len()..].chars().next()) {
            *text = text[word.len()..].trim().to_string();
            return true;
        }
        if text.ends_with(word) && boundary(text[..text.len() - word.len()].chars().last()) {
            *text = text[..text.len() - word.len()].trim().to_string();
            return true;
        }
        false
    };
    loop {
        let mut peeled = false;
        if is_pm.is_none()
            && let Some((_, pm)) = symbols
                .day_periods
                .iter()
                .find(|(word, _)| strip(&mut text, word))
        {
            is_pm = Some(*pm);
            peeled = true;
        }
        if symbols.edge_words.iter().any(|word| strip(&mut text, word)) {
            peeled = true;
        }
        if !peeled {
            break;
        }
    }
    for separator in &symbols.separators {
        text = replace_separator(&text, separator);
    }
    if let Some(pm) = is_pm {
        text = format!("{text} {}", if pm { "pm" } else { "am" });
    }
    Some(text)
}

/// `(\d)\s*SEP\s*(?=\d)` → `$1:`.
fn replace_separator(text: &str, separator: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let sep: Vec<char> = separator.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        out.push(chars[i]);
        if chars[i].is_ascii_digit() {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if chars[j..].starts_with(&sep) {
                let mut k = j + sep.len();
                while k < chars.len() && chars[k].is_whitespace() {
                    k += 1;
                }
                if k < chars.len() && chars[k].is_ascii_digit() {
                    out.push(':');
                    i = k;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Reads typed text as a time in the locale's digits, separator and day
/// periods, or anything [`parse_time_input`] reads (`parseLocaleTime`).
pub fn parse_locale_time(input: &str, locale: &str, include_seconds: bool) -> Option<PlainTime> {
    parse_time_input(&read_locale_time(input, locale)?, include_seconds)
}

/// Whether typed text holds every part of a time (`isLocaleTimeComplete`):
/// the hour, both minute digits, both second digits with seconds, and in a
/// 12-hour field a day period unless the hour reads 0 or 13 through 23.
pub fn is_locale_time_complete(
    input: &str,
    locale: &str,
    hour_format: HourFormat,
    include_seconds: bool,
) -> bool {
    let Some(text) = read_locale_time(input, locale) else {
        return false;
    };
    let (body, meridiem) = strip_meridiem(&text);
    let digits = body.trim();
    let clock = || -> Option<(u32, bool)> {
        let parts: Vec<&str> = digits.split(':').collect();
        let ok = |p: &str, lens: &[usize]| {
            lens.contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit())
        };
        match parts.as_slice() {
            [h, m] if ok(h, &[1, 2]) && ok(m, &[2]) => Some((h.parse().ok()?, false)),
            [h, m, s] if ok(h, &[1, 2]) && ok(m, &[2]) && ok(s, &[2]) => {
                Some((h.parse().ok()?, true))
            }
            [compact] if ok(compact, &[4]) => Some((compact[..2].parse().ok()?, false)),
            [compact] if ok(compact, &[6]) => Some((compact[..2].parse().ok()?, true)),
            _ => None,
        }
    };
    let Some((hour, has_seconds)) = clock() else {
        return false;
    };
    if include_seconds && !has_seconds {
        return false;
    }
    if hour_format == HourFormat::H24 || meridiem.is_some() {
        return true;
    }
    hour == 0 || (13..=23).contains(&hour)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u8, m: u8, s: u8) -> PlainTime {
        PlainTime::new(h, m, s).unwrap()
    }

    #[test]
    fn iso_times() {
        assert_eq!(PlainTime::from_iso("14:30"), Some(t(14, 30, 0)));
        assert_eq!(PlainTime::from_iso("09:15:07"), Some(t(9, 15, 7)));
        assert_eq!(PlainTime::from_iso("24:00"), None);
        assert_eq!(
            PlainTime::parse_strict("14:30:15.250"),
            Some((t(14, 30, 15), true))
        );
        assert_eq!(PlainTime::parse_strict("2:30 PM"), None);
        assert_eq!(PlainTime::parse_strict("14:30abc"), None);
        assert_eq!(t(9, 5, 0).to_iso(false), "09:05");
        assert_eq!(t(9, 5, 3).to_iso(true), "09:05:03");
    }

    #[test]
    fn typed_times() {
        let p = |s| parse_time_input(s, false);
        assert_eq!(p("2:30 PM"), Some(t(14, 30, 0)));
        assert_eq!(p("2:30pm"), Some(t(14, 30, 0)));
        assert_eq!(p("2:30 p.m."), Some(t(14, 30, 0)));
        assert_eq!(p("12:15 am"), Some(t(0, 15, 0)));
        assert_eq!(p("14:30"), Some(t(14, 30, 0)));
        assert_eq!(p("1430"), Some(t(14, 30, 0)));
        assert_eq!(p("2pm"), Some(t(14, 0, 0)));
        assert_eq!(p("13pm"), None);
        assert_eq!(p("25:00"), None);
        assert_eq!(parse_time_input("143005", true), Some(t(14, 30, 5)));
        assert_eq!(parse_time_input("14:30:05", false), Some(t(14, 30, 0)));
    }

    #[test]
    fn ranges_clamps_and_steps() {
        assert!(is_time_in_range(
            t(9, 0, 0),
            Some(t(9, 0, 0)),
            Some(t(17, 0, 0))
        ));
        assert!(!is_time_in_range(t(8, 59, 0), Some(t(9, 0, 0)), None));
        assert_eq!(
            clamp_time(t(8, 0, 0), Some(t(9, 0, 30)), None, false),
            t(9, 1, 0)
        );
        assert_eq!(
            clamp_time(t(8, 0, 0), Some(t(9, 0, 30)), Some(t(9, 0, 45)), false),
            t(9, 0, 30)
        );
        assert_eq!(
            clamp_time(t(18, 0, 0), None, Some(t(17, 0, 30)), false),
            t(17, 0, 0)
        );
        assert_eq!(adjust_time(t(23, 50, 0), 15, false), t(0, 5, 0));
        assert_eq!(adjust_time(t(0, 5, 0), -10, false), t(23, 55, 0));
        assert_eq!(compare_time(None, Some(t(0, 0, 0))), Ordering::Less);
    }

    #[test]
    fn locale_times_follow_the_locale() {
        assert_eq!(resolve_hour_format("en-US", None), HourFormat::H12);
        assert_eq!(resolve_hour_format("en-GB", None), HourFormat::H24);
        assert_eq!(resolve_hour_format("de-DE", None), HourFormat::H24);
        assert_eq!(resolve_hour_format("en-US-u-hc-h23", None), HourFormat::H24);
        assert_eq!(
            resolve_hour_format("en-US", Some(HourFormat::H24)),
            HourFormat::H24
        );
        assert_eq!(
            format_locale_time(t(14, 30, 0), "en-US", HourFormat::H12, false),
            "2:30 PM"
        );
        assert_eq!(
            format_locale_time(t(14, 30, 0), "de-DE", HourFormat::H24, false),
            "14:30"
        );
        assert_eq!(
            format_locale_time(t(14, 30, 0), "ko-KR", HourFormat::H12, false),
            "오후 2:30"
        );
        assert_eq!(
            parse_locale_time("오후 2:30", "ko-KR", false),
            Some(t(14, 30, 0))
        );
        assert_eq!(
            parse_locale_time("2:30 PM", "de-DE", false),
            Some(t(14, 30, 0))
        );
        assert_eq!(
            parse_locale_time("13 h 30", "fr-CA", false),
            Some(t(13, 30, 0))
        );
        assert_eq!(parse_locale_time("١٤:٣٠", "ar", false), Some(t(14, 30, 0)));
    }

    #[test]
    fn complete_times() {
        assert!(is_locale_time_complete(
            "2:30 PM",
            "en-US",
            HourFormat::H12,
            false
        ));
        assert!(!is_locale_time_complete(
            "2:30",
            "en-US",
            HourFormat::H12,
            false
        ));
        assert!(is_locale_time_complete(
            "14:30",
            "en-US",
            HourFormat::H12,
            false
        ));
        assert!(is_locale_time_complete(
            "2:30",
            "de-DE",
            HourFormat::H24,
            false
        ));
        assert!(!is_locale_time_complete(
            "2:3",
            "de-DE",
            HourFormat::H24,
            false
        ));
        assert!(!is_locale_time_complete(
            "14:30",
            "de-DE",
            HourFormat::H24,
            true
        ));
    }
}
