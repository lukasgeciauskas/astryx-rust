//! Upstream's `toDate` and the parts of a JavaScript `Date` it leans on: the
//! time value (`TimeClip`), the local-time conversions a `Date` makes in the
//! system's time zone, the `new Date(year, month, day)` constructor and
//! `Date.parse` for strings.
//!
//! `Date.parse` is V8's date parser, the one Node and Chrome run: the ES date-time
//! string format first (`2024-01-05`, `2024-01-05T10:00:00.000+05:30`,
//! `+275760-09-13T00:00:00Z`), then the legacy forms browsers accept, RFC 2822
//! (`Fri, 05 Jan 2024 10:00:00 GMT`) among them, along with `Jan 5 2024`,
//! `1/5/2024 10:00 PM` and `2024-01-05 10:00`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use astryx_i18n::date::PlainDate;
use astryx_theme::authoring::js::js_trim;
use icu_calendar::Gregorian;
use icu_datetime::fieldsets::{MD, YM};
use icu_datetime::input::Date;
use icu_datetime::pattern::{DateTimePattern, FixedCalendarDateTimeNames};
use icu_datetime::{DateTimeFormatterPreferences, FixedCalendarDateTimeFormatter};
use icu_locale::Locale;
use icu_locale::extensions::unicode::{key, value};
use jiff::tz::TimeZone;
use writeable::TryWriteable as _;

use crate::ChartValue;

const MS_PER_DAY: f64 = 86_400_000.;
/// The largest time value a `Date` holds: 100,000,000 days either side of the epoch.
const MAX_TIME: f64 = 8.64e15;
/// V8's `kMaxTimeBeforeUTCInMs`: the widest local time it converts to UTC.
const MAX_TIME_BEFORE_UTC: f64 = MAX_TIME + 864_000_000.;

/// `TimeClip`: a time value a `Date` can hold, whole milliseconds, or NaN.
pub(super) fn time_clip(t: f64) -> f64 {
    if !t.is_finite() || t.abs() > MAX_TIME {
        return f64::NAN;
    }
    // Adding zero turns -0 into +0, as `ToIntegerOrInfinity(t) + 0` does.
    t.trunc() + 0.
}

/// Upstream `toDate`: a value as a time value, NaN for an invalid date.
///
/// Accepts a `Date`, epoch milliseconds (a number or a numeric string), or a
/// date string (ISO 8601, RFC 2822, ...). Anything else is an invalid date, so
/// callers can pass the original value through unchanged.
///
/// Date-only ISO strings (`YYYY-MM-DD`) are constructed at local midnight rather
/// than the spec's UTC midnight, so the calendar day survives local formatting in
/// negative-offset time zones (`"2024-01-05"` stays "Jan 5", not "Jan 4").
pub(super) fn to_date(value: &ChartValue) -> f64 {
    match value {
        ChartValue::Date(ms) | ChartValue::Number(ms) => time_clip(*ms),
        ChartValue::Text(text) => {
            let s = js_trim(text);
            if s.is_empty() {
                return f64::NAN;
            }
            if let Some((year, month, day)) = date_only(s) {
                let local = local_date_value(year, month - 1, day);
                // Reject out-of-range parts (e.g. "2024-02-31") that the Date
                // constructor would otherwise silently roll into a different day;
                // treat them as invalid so the raw value passes through unchanged.
                if local_fields(local) != Some((year, month, day)) {
                    return f64::NAN;
                }
                return local;
            }
            let as_number = ChartValue::Text(s.into()).to_number();
            if as_number.is_finite() {
                time_clip(as_number)
            } else {
                date_parse(s)
            }
        }
        ChartValue::Null | ChartValue::Bool(_) => f64::NAN,
    }
}

/// `/^(\d{4})-(\d{2})-(\d{2})$/`: the year, month and day of a date-only ISO string.
fn date_only(s: &str) -> Option<(i64, i64, i64)> {
    let b = s.as_bytes();
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        b[range].iter().try_fold(0i64, |n, &c| {
            c.is_ascii_digit().then(|| n * 10 + i64::from(c - b'0'))
        })
    };
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    Some((digits(0..4)?, digits(5..7)?, digits(8..10)?))
}

/// The local year, month (1 to 12) and day of a time value, as `getFullYear`,
/// `getMonth() + 1` and `getDate` read it.
fn local_fields(t: f64) -> Option<(i64, i64, i64)> {
    if t.is_nan() {
        return None;
    }
    let local = local_time(t);
    let (y, m, d) = civil_from_days((local / MS_PER_DAY).floor() as i64);
    Some((y, i64::from(m), i64::from(d)))
}

/// The local calendar date of a valid time value, upstream's `plainDateFromDate`.
pub(super) fn local_plain_date(t: f64) -> PlainDate {
    let local = local_time(t);
    let (year, month, day) = civil_from_days((local / MS_PER_DAY).floor() as i64);
    PlainDate {
        year: year as i32,
        month: month as u8,
        day: day as u8,
    }
}

/// `new Date(year, monthIndex, day)`: local midnight of that day, a year from 0
/// to 99 read as 1900 to 1999 and out-of-range parts rolled over.
fn local_date_value(year: i64, month_index: i64, day: i64) -> f64 {
    let year = if (0..=99).contains(&year) {
        1900 + year
    } else {
        year
    };
    let day_number = make_day(year as f64, month_index as f64, day as f64);
    let local = day_number * MS_PER_DAY;
    if !local.is_finite() || local.abs() > MAX_TIME_BEFORE_UTC {
        return f64::NAN;
    }
    time_clip(utc_of_local(local))
}

/// The proleptic Gregorian date `days` after 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The days from 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `MakeDay`: the day number of a year, a month index that may overflow, and a
/// day of the month that may overflow; NaN outside V8's year range.
fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !(-1_000_000. ..=1_000_000.).contains(&year)
        || !(-10_000_000. ..=10_000_000.).contains(&month)
        || !date.is_finite()
    {
        return f64::NAN;
    }
    let (y, m) = (year as i64, month as i64);
    let y = y + m.div_euclid(12);
    let m = m.rem_euclid(12);
    (days_from_civil(y, m + 1, 1) - 1) as f64 + date.trunc()
}

/// jiff's instants span years -9999 to 9999; a `Date` spans 275,760 years either
/// side of 1970. Instants beyond jiff's take the offset at its nearest edge.
const JIFF_MIN_MS: f64 = -377_705_116_800_000.;
const JIFF_MAX_MS: f64 = 253_402_207_200_000.;

/// The system zone's UTC offset at an instant, in milliseconds.
fn offset_at(utc: f64, tz: &TimeZone) -> f64 {
    let clamped = utc.clamp(JIFF_MIN_MS, JIFF_MAX_MS) as i64;
    let at = jiff::Timestamp::from_millisecond(clamped).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
    f64::from(tz.to_offset(at).seconds()) * 1000.
}

/// `LocalTime(t)`: the wall-clock milliseconds of an instant in the system zone.
fn local_time(utc: f64) -> f64 {
    utc + offset_at(utc, &TimeZone::system())
}

/// `UTC(t)`: the instant a local wall-clock time names in the system zone. A
/// time a transition repeats is its earlier occurrence; one a transition skips
/// reads in the offset before it, which moves it forward by the gap.
fn utc_of_local(local: f64) -> f64 {
    let tz = TimeZone::system();
    let days = (local / MS_PER_DAY).floor();
    let in_day = (local - days * MS_PER_DAY) as i64;
    let (year, month, day) = civil_from_days(days as i64);
    let civil = i16::try_from(year).ok().and_then(|year| {
        jiff::civil::DateTime::new(
            year,
            month as i8,
            day as i8,
            (in_day / 3_600_000) as i8,
            (in_day / 60_000 % 60) as i8,
            (in_day / 1000 % 60) as i8,
            (in_day % 1000 * 1_000_000) as i32,
        )
        .ok()
    });
    match civil.and_then(|dt| tz.to_ambiguous_timestamp(dt).compatible().ok()) {
        Some(at) => at.as_millisecond() as f64,
        None => local - offset_at(local, &tz),
    }
}

/// One locale's pattern for upstream's Gregorian `monthYear`, with the names it needs.
struct MonthYearFormat {
    names: FixedCalendarDateTimeNames<Gregorian, YM>,
    pattern: DateTimePattern,
}

thread_local! {
    static MONTH_YEAR: RefCell<HashMap<String, Option<Rc<MonthYearFormat>>>> =
        RefCell::new(HashMap::new());
}

/// Whether a pattern names the month (`MMM`, `LLL` and wider), or, with `words`,
/// carries words (`M月d日`) rather than only digits and separators.
fn writes_words(pattern: &str, words: bool) -> bool {
    let mut quoted = false;
    let mut run = (' ', 0);
    for c in pattern.chars() {
        if c == '\'' {
            quoted = !quoted;
            run = (' ', 0);
            continue;
        }
        if !quoted && matches!(c, 'M' | 'L') {
            run = if run.0 == c { (c, run.1 + 1) } else { (c, 1) };
            if run.1 >= 3 {
                return true;
            }
            continue;
        }
        run = (' ', 0);
        if words && !quoted && !c.is_ascii() && c.is_alphabetic() {
            return true;
        }
    }
    false
}

/// The pattern with its wide month names (`MMMM`, `LLLL`) abbreviated.
fn abbreviate_months(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len());
    let (mut i, mut quoted) = (0, false);
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            quoted = !quoted;
        } else if !quoted && matches!(c, 'M' | 'L') {
            let run = chars[i..].iter().take_while(|&&m| m == c).count();
            out.extend(std::iter::repeat_n(c, if run == 4 { 3 } else { run }));
            i += run;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn month_year_format(locale: &str) -> Option<MonthYearFormat> {
    let mut parsed: Locale = locale.parse().unwrap_or(Locale::UNKNOWN);
    parsed
        .extensions
        .unicode
        .keywords
        .set(key!("ca"), value!("gregory"));
    let prefs = DateTimeFormatterPreferences::from(&parsed);
    let sample = Date::try_new_gregorian(2024, 1, 5).ok()?;
    let pattern_of = |fields: YM| {
        FixedCalendarDateTimeFormatter::<Gregorian, YM>::try_new(prefs, fields)
            .ok()
            .map(|f| f.format(&sample).pattern())
    };
    let medium = pattern_of(YM::medium())?;
    let pattern = if writes_words(&medium.to_string(), false) {
        medium
    } else {
        let month_day =
            FixedCalendarDateTimeFormatter::<Gregorian, MD>::try_new(prefs, MD::medium())
                .ok()?
                .format(&sample)
                .pattern();
        match pattern_of(YM::long()) {
            Some(long) if writes_words(&month_day.to_string(), true) => {
                abbreviate_months(&long.to_string())
                    .parse()
                    .unwrap_or(medium)
            }
            _ => medium,
        }
    };
    let mut names = FixedCalendarDateTimeNames::<Gregorian, YM>::try_new(prefs).ok()?;
    names.include_for_pattern(&pattern).ok()?;
    Some(MonthYearFormat { names, pattern })
}

/// A Gregorian month and year as upstream's `monthYear` writes them,
/// `Intl.DateTimeFormat(locale, {month: 'short', year: 'numeric'})`: CLDR's `yMMM`
/// pattern. `None` when the date is beyond ICU4X's years or the locale has no data.
///
/// ICU4X's compiled data has no `yMMM`. Its medium year-and-month takes the month
/// width of the locale's medium date, which is numeric in German, Japanese, Arabic
/// and Korean ("01/2024") where `yMMM` names the month ("Jan. 2024"). So the
/// medium pattern stands where it names the month; where it is numeric but the
/// locale's month and day (`MMMd`) names the month or carries words, the long
/// pattern with abbreviated months stands in; elsewhere (Lithuanian, Slovak) the
/// numeric medium pattern is `yMMM` too. The pattern comes from a modern date and
/// is used for every year, as `yMMM` writes no era.
pub(super) fn gregorian_month_year(date: PlainDate, locale: &str) -> Option<String> {
    let format = MONTH_YEAR.with(|cache| {
        cache
            .borrow_mut()
            .entry(locale.to_string())
            .or_insert_with(|| month_year_format(locale).map(Rc::new))
            .clone()
    })?;
    let date = Date::try_new_iso(date.year, date.month, date.day)
        .ok()?
        .to_calendar(Gregorian);
    format
        .names
        .with_pattern_unchecked(&format.pattern)
        .format(&date)
        .try_write_to_string()
        .ok()
        .map(|text| text.into_owned())
}

/// `Date.parse(s)`: the time value a date string names, or NaN.
pub fn date_parse(s: &str) -> f64 {
    let units: Vec<u16> = s.encode_utf16().collect();
    let Some(out) = parse(&units) else {
        return f64::NAN;
    };
    let day = make_day(
        f64::from(out.year),
        f64::from(out.month),
        f64::from(out.day),
    );
    let time = f64::from(out.hour) * 3_600_000.
        + f64::from(out.minute) * 60_000.
        + f64::from(out.second) * 1000.
        + f64::from(out.millisecond);
    let date = day * MS_PER_DAY + time;
    match out.utc_offset {
        None => {
            if date.is_nan() || date.abs() > MAX_TIME_BEFORE_UTC {
                return f64::NAN;
            }
            time_clip(utc_of_local(date))
        }
        Some(offset) => {
            let date = date - f64::from(offset) * 1000.;
            if date.is_nan() || date.abs() > MAX_TIME {
                return f64::NAN;
            }
            time_clip(date)
        }
    }
}

/// V8's `kNone`: a slot no part has filled.
const NONE: i32 = i32::MAX;
/// Digits past the ninth of a number are skipped.
const MAX_SIGNIFICANT_DIGITS: usize = 9;

/// The fields a date string names.
struct Parsed {
    year: i32,
    /// 0 to 11.
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: i32,
    millisecond: i32,
    /// Seconds east of UTC; `None` for local time.
    utc_offset: Option<i32>,
}

/// ES `WhiteSpace`: tab, vertical tab, form feed, U+FEFF and the space separators.
fn is_white_space(c: u32) -> bool {
    matches!(
        c,
        0x09 | 0x0B | 0x0C | 0x20 | 0xA0 | 0x1680 | 0x2000
            ..=0x200A | 0x202F | 0x205F | 0x3000 | 0xFEFF
    )
}

/// ES `WhiteSpace` or `LineTerminator`.
fn is_white_space_or_line_terminator(c: u32) -> bool {
    is_white_space(c) || matches!(c, 0x0A | 0x0D | 0x2028 | 0x2029)
}

/// V8's `InputReader`: the string's UTF-16 units, one at a time; a NUL ends it.
struct Reader<'a> {
    units: &'a [u16],
    index: usize,
    ch: u32,
}

impl<'a> Reader<'a> {
    fn new(units: &'a [u16]) -> Self {
        let mut reader = Self {
            units,
            index: 0,
            ch: 0,
        };
        reader.next();
        reader
    }

    fn position(&self) -> usize {
        self.index
    }

    fn next(&mut self) {
        self.ch = self.units.get(self.index).map_or(0, |&u| u32::from(u));
        self.index += 1;
    }

    /// A run of digits as an unsigned number, capped at nine significant digits.
    fn read_unsigned_numeral(&mut self) -> i32 {
        let mut n = 0i32;
        let mut i = 0;
        while self.ch == u32::from(b'0') {
            self.next();
        }
        while self.is_ascii_digit() {
            if i < MAX_SIGNIFICANT_DIGITS {
                n = n * 10 + (self.ch - u32::from(b'0')) as i32;
            }
            i += 1;
            self.next();
        }
        n
    }

    /// A word (characters from 'A' up, no white space): its lower-cased
    /// three-character prefix, zero-padded, and its length.
    fn read_word(&mut self) -> ([u32; 3], usize) {
        let mut prefix = [0u32; 3];
        let mut len = 0;
        while self.is_ascii_alpha_or_above() && !self.is_white_space_char() {
            if len < prefix.len() {
                prefix[len] = self.ch | 0x20;
            }
            len += 1;
            self.next();
        }
        (prefix, len)
    }

    fn skip(&mut self, c: u8) -> bool {
        if self.ch == u32::from(c) {
            self.next();
            true
        } else {
            false
        }
    }

    fn skip_white_space(&mut self) -> bool {
        if is_white_space_or_line_terminator(self.ch) {
            self.next();
            true
        } else {
            false
        }
    }

    fn skip_parentheses(&mut self) -> bool {
        if self.ch != u32::from(b'(') {
            return false;
        }
        let mut balance = 0;
        loop {
            if self.ch == u32::from(b')') {
                balance -= 1;
            } else if self.ch == u32::from(b'(') {
                balance += 1;
            }
            self.next();
            if !(balance > 0 && self.ch != 0) {
                return true;
            }
        }
    }

    fn is_end(&self) -> bool {
        self.ch == 0
    }

    fn is_ascii_digit(&self) -> bool {
        (u32::from(b'0')..=u32::from(b'9')).contains(&self.ch)
    }

    fn is_ascii_alpha_or_above(&self) -> bool {
        self.ch >= u32::from(b'A')
    }

    fn is_white_space_char(&self) -> bool {
        is_white_space(self.ch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeywordType {
    Invalid,
    MonthName,
    TimeZoneName,
    TimeSeparator,
    AmPm,
}

/// V8's `KeywordTable`: a prefix, its type and value (a month's number, an hour
/// offset, a zone's hours east of UTC).
const KEYWORDS: [([u8; 3], KeywordType, i32); 27] = [
    (*b"jan", KeywordType::MonthName, 1),
    (*b"feb", KeywordType::MonthName, 2),
    (*b"mar", KeywordType::MonthName, 3),
    (*b"apr", KeywordType::MonthName, 4),
    (*b"may", KeywordType::MonthName, 5),
    (*b"jun", KeywordType::MonthName, 6),
    (*b"jul", KeywordType::MonthName, 7),
    (*b"aug", KeywordType::MonthName, 8),
    (*b"sep", KeywordType::MonthName, 9),
    (*b"oct", KeywordType::MonthName, 10),
    (*b"nov", KeywordType::MonthName, 11),
    (*b"dec", KeywordType::MonthName, 12),
    (*b"am\0", KeywordType::AmPm, 0),
    (*b"pm\0", KeywordType::AmPm, 12),
    (*b"ut\0", KeywordType::TimeZoneName, 0),
    (*b"utc", KeywordType::TimeZoneName, 0),
    (*b"z\0\0", KeywordType::TimeZoneName, 0),
    (*b"gmt", KeywordType::TimeZoneName, 0),
    (*b"cdt", KeywordType::TimeZoneName, -5),
    (*b"cst", KeywordType::TimeZoneName, -6),
    (*b"edt", KeywordType::TimeZoneName, -4),
    (*b"est", KeywordType::TimeZoneName, -5),
    (*b"mdt", KeywordType::TimeZoneName, -6),
    (*b"mst", KeywordType::TimeZoneName, -7),
    (*b"pdt", KeywordType::TimeZoneName, -7),
    (*b"pst", KeywordType::TimeZoneName, -8),
    (*b"t\0\0", KeywordType::TimeSeparator, 0),
];

/// A word's keyword: the entry its prefix matches, where a word longer than the
/// keyword only matches a month name.
fn keyword(prefix: [u32; 3], len: usize) -> (KeywordType, i32) {
    KEYWORDS
        .iter()
        .find(|(word, kind, _)| {
            word.iter().zip(prefix).all(|(&w, p)| u32::from(w) == p)
                && (len <= 3 || *kind == KeywordType::MonthName)
        })
        .map_or((KeywordType::Invalid, 0), |&(_, kind, value)| (kind, value))
}

/// V8's `DateToken`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    Invalid,
    Unknown,
    Number {
        value: i32,
        length: usize,
    },
    Symbol(u8),
    WhiteSpace,
    Keyword {
        kind: KeywordType,
        value: i32,
        length: usize,
    },
    EndOfInput,
}

impl Token {
    fn is_number(self) -> bool {
        matches!(self, Token::Number { .. })
    }

    fn number(self) -> i32 {
        match self {
            Token::Number { value, .. } => value,
            _ => 0,
        }
    }

    fn is_fixed_length_number(self, n: usize) -> bool {
        matches!(self, Token::Number { length, .. } if length == n)
    }

    fn is_symbol(self, c: u8) -> bool {
        self == Token::Symbol(c)
    }

    fn is_ascii_sign(self) -> bool {
        self.is_symbol(b'+') || self.is_symbol(b'-')
    }

    /// 1 for '+', -1 for '-'.
    fn ascii_sign(self) -> i32 {
        if self.is_symbol(b'-') { -1 } else { 1 }
    }

    fn is_keyword_type(self, kind: KeywordType) -> bool {
        matches!(self, Token::Keyword { kind: k, .. } if k == kind)
    }

    fn is_keyword_z(self) -> bool {
        matches!(
            self,
            Token::Keyword {
                kind: KeywordType::TimeZoneName,
                value: 0,
                length: 1
            }
        )
    }
}

/// V8's `DateStringTokenizer`: tokens with one of lookahead.
struct Tokenizer<'a> {
    input: Reader<'a>,
    next: Token,
}

impl<'a> Tokenizer<'a> {
    fn new(units: &'a [u16]) -> Self {
        let mut input = Reader::new(units);
        let next = Self::scan(&mut input);
        Self { input, next }
    }

    fn next(&mut self) -> Token {
        let result = self.next;
        self.next = Self::scan(&mut self.input);
        result
    }

    fn peek(&self) -> Token {
        self.next
    }

    fn skip_symbol(&mut self, c: u8) -> bool {
        if self.next.is_symbol(c) {
            self.next = Self::scan(&mut self.input);
            true
        } else {
            false
        }
    }

    fn scan(input: &mut Reader<'_>) -> Token {
        let pre = input.position();
        if input.is_end() {
            return Token::EndOfInput;
        }
        if input.is_ascii_digit() {
            let value = input.read_unsigned_numeral();
            return Token::Number {
                value,
                length: input.position() - pre,
            };
        }
        for &c in b":-+.)" {
            if input.skip(c) {
                return Token::Symbol(c);
            }
        }
        if input.is_ascii_alpha_or_above() && !input.is_white_space_char() {
            let (prefix, length) = input.read_word();
            let (kind, value) = keyword(prefix, length);
            return Token::Keyword {
                kind,
                value,
                length,
            };
        }
        if input.skip_white_space() {
            return Token::WhiteSpace;
        }
        if input.skip_parentheses() {
            return Token::Unknown;
        }
        input.next();
        Token::Unknown
    }
}

fn between(x: i32, lo: i32, hi: i32) -> bool {
    (lo..=hi).contains(&x)
}

/// V8's `DayComposer`: up to three numbers and a named month.
struct DayComposer {
    comp: [i32; 3],
    index: usize,
    named_month: i32,
    is_iso_date: bool,
}

impl DayComposer {
    fn is_empty(&self) -> bool {
        self.index == 0
    }

    fn add(&mut self, n: i32) -> bool {
        if self.index == self.comp.len() {
            return false;
        }
        self.comp[self.index] = n;
        self.index += 1;
        true
    }

    fn is_month(x: i32) -> bool {
        between(x, 1, 12)
    }

    fn is_day(x: i32) -> bool {
        between(x, 1, 31)
    }

    /// The year, month index and day.
    fn write(&mut self) -> Option<(i32, i32, i32)> {
        if self.index < 1 {
            return None;
        }
        // Day and month default to 1.
        while self.index < self.comp.len() {
            self.comp[self.index] = 1;
            self.index += 1;
        }
        // The default year is 0 (2000) for KJS compatibility.
        let (mut year, month, day);
        if self.named_month == NONE {
            if self.is_iso_date || !Self::is_day(self.comp[0]) {
                (year, month, day) = (self.comp[0], self.comp[1], self.comp[2]);
            } else {
                (year, month, day) = (self.comp[2], self.comp[0], self.comp[1]);
            }
        } else {
            month = self.named_month;
            if !Self::is_day(self.comp[0]) {
                (year, day) = (self.comp[0], self.comp[1]);
            } else {
                (year, day) = (self.comp[1], self.comp[0]);
            }
        }
        if !self.is_iso_date {
            if between(year, 0, 49) {
                year += 2000;
            } else if between(year, 50, 99) {
                year += 1900;
            }
        }
        if !Self::is_month(month) || !Self::is_day(day) {
            return None;
        }
        Some((year, month - 1, day))
    }
}

/// V8's `TimeComposer`: hour, minute, second and millisecond, and an AM/PM
/// offset.
struct TimeComposer {
    comp: [i32; 4],
    index: usize,
    hour_offset: i32,
}

impl TimeComposer {
    fn is_empty(&self) -> bool {
        self.index == 0
    }

    fn is_expecting(&self, n: i32) -> bool {
        (self.index == 1 && Self::is_minute(n))
            || (self.index == 2 && Self::is_second(n))
            || (self.index == 3 && between(n, 0, 999))
    }

    fn add(&mut self, n: i32) -> bool {
        if self.index < self.comp.len() {
            self.comp[self.index] = n;
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn add_final(&mut self, n: i32) -> bool {
        if !self.add(n) {
            return false;
        }
        while self.index < self.comp.len() {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        true
    }

    fn is_minute(x: i32) -> bool {
        between(x, 0, 59)
    }

    fn is_hour(x: i32) -> bool {
        between(x, 0, 23)
    }

    fn is_second(x: i32) -> bool {
        between(x, 0, 59)
    }

    fn write(&mut self) -> Option<[i32; 4]> {
        // Every time slot defaults to 0.
        while self.index < self.comp.len() {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        let [mut hour, minute, second, millisecond] = self.comp;
        if self.hour_offset != NONE {
            if !between(hour, 0, 12) {
                return None;
            }
            hour %= 12;
            hour += self.hour_offset;
        }
        if !Self::is_hour(hour)
            || !Self::is_minute(minute)
            || !Self::is_second(second)
            || !between(millisecond, 0, 999)
        {
            // A 24th hour is allowed if minutes, seconds and milliseconds are 0.
            if hour != 24 || minute != 0 || second != 0 || millisecond != 0 {
                return None;
            }
        }
        Some([hour, minute, second, millisecond])
    }
}

/// V8's `TimeZoneComposer`: a sign, hours and minutes east of UTC.
struct TimeZoneComposer {
    sign: i32,
    hour: i32,
    minute: i32,
}

impl TimeZoneComposer {
    fn set(&mut self, offset_in_hours: i32) {
        self.sign = if offset_in_hours < 0 { -1 } else { 1 };
        self.hour = offset_in_hours * self.sign;
        self.minute = 0;
    }

    fn set_sign(&mut self, sign: i32) {
        self.sign = if sign < 0 { -1 } else { 1 };
    }

    fn is_expecting(&self, n: i32) -> bool {
        self.hour != NONE && self.minute == NONE && TimeComposer::is_minute(n)
    }

    fn is_utc(&self) -> bool {
        self.hour == 0 && self.minute == 0
    }

    fn is_empty(&self) -> bool {
        self.hour == NONE
    }

    /// The offset in seconds, `Some(None)` for local time, `None` when invalid.
    fn write(&mut self) -> Option<Option<i32>> {
        if self.sign == NONE {
            return Some(None);
        }
        if self.hour == NONE {
            self.hour = 0;
        }
        if self.minute == NONE {
            self.minute = 0;
        }
        // V8 sums in unsigned 32-bit arithmetic and caps at its largest small integer.
        let total = (self.hour as u32)
            .wrapping_mul(3600)
            .wrapping_add((self.minute as u32).wrapping_mul(60));
        let total = i32::try_from(total).ok()?;
        Some(Some(if self.sign < 0 { -total } else { total }))
    }
}

/// V8's `ReadMilliseconds`: the first three significant digits of a fraction.
fn read_milliseconds(token: Token) -> i32 {
    let Token::Number { value, length } = token else {
        return 0;
    };
    match length {
        1 => value * 100,
        2 => value * 10,
        3 => value,
        _ => {
            let length = length.min(MAX_SIGNIFICANT_DIGITS);
            value / 10i32.pow((length - 3) as u32)
        }
    }
}

/// V8's `ParseES5DateTime`: the ES date-time string format,
/// `[('-'|'+')yy]yyyy[-MM[-DD]][THH:mm[:ss[.sss]][Z|(+|-)hh:mm]]`. Returns the
/// first token it leaves to the legacy parser, `EndOfInput` when it read the whole
/// string, or `Invalid`.
fn parse_es5_date_time(
    scanner: &mut Tokenizer<'_>,
    day: &mut DayComposer,
    time: &mut TimeComposer,
    tz: &mut TimeZoneComposer,
) -> Token {
    // The mandatory date: [('-'|'+')yy]yyyy[-MM[-DD]].
    if scanner.peek().is_ascii_sign() {
        // Keep the sign token, so the legacy parser reads the rest.
        let sign_token = scanner.next();
        if !scanner.peek().is_fixed_length_number(6) {
            return sign_token;
        }
        let sign = sign_token.ascii_sign();
        let year = scanner.next().number();
        if sign < 0 && year == 0 {
            return sign_token;
        }
        day.add(sign * year);
    } else if scanner.peek().is_fixed_length_number(4) {
        day.add(scanner.next().number());
    } else {
        return scanner.next();
    }
    if scanner.skip_symbol(b'-') {
        if !scanner.peek().is_fixed_length_number(2)
            || !DayComposer::is_month(scanner.peek().number())
        {
            return scanner.next();
        }
        day.add(scanner.next().number());
        if scanner.skip_symbol(b'-') {
            if !scanner.peek().is_fixed_length_number(2)
                || !DayComposer::is_day(scanner.peek().number())
            {
                return scanner.next();
            }
            day.add(scanner.next().number());
        }
    }
    // The optional time: 'T'HH':'mm[':'ss['.'sss]]Z.
    if !scanner.peek().is_keyword_type(KeywordType::TimeSeparator) {
        if scanner.peek() != Token::EndOfInput {
            return scanner.next();
        }
    } else {
        scanner.next();
        if !scanner.peek().is_fixed_length_number(2) || !between(scanner.peek().number(), 0, 24) {
            return Token::Invalid;
        }
        // 24:00[:00[.000]] is allowed, no other time starting with 24.
        let hour_is_24 = scanner.peek().number() == 24;
        time.add(scanner.next().number());
        if !scanner.skip_symbol(b':') {
            return Token::Invalid;
        }
        if !scanner.peek().is_fixed_length_number(2)
            || !TimeComposer::is_minute(scanner.peek().number())
            || (hour_is_24 && scanner.peek().number() > 0)
        {
            return Token::Invalid;
        }
        time.add(scanner.next().number());
        if scanner.skip_symbol(b':') {
            if !scanner.peek().is_fixed_length_number(2)
                || !TimeComposer::is_second(scanner.peek().number())
                || (hour_is_24 && scanner.peek().number() > 0)
            {
                return Token::Invalid;
            }
            time.add(scanner.next().number());
            if scanner.skip_symbol(b'.') {
                if !scanner.peek().is_number() || (hour_is_24 && scanner.peek().number() > 0) {
                    return Token::Invalid;
                }
                // More or fewer than the mandated three digits are allowed.
                time.add(read_milliseconds(scanner.next()));
            }
        }
        // The optional zone: 'Z' | ('+'|'-')hh':'mm.
        if scanner.peek().is_keyword_z() {
            scanner.next();
            tz.set(0);
        } else if scanner.peek().is_ascii_sign() {
            tz.set_sign(scanner.next().ascii_sign());
            if scanner.peek().is_fixed_length_number(4) {
                // The hhmm extension.
                let hourmin = scanner.next().number();
                let (hour, min) = (hourmin / 100, hourmin % 100);
                if !TimeComposer::is_hour(hour) || !TimeComposer::is_minute(min) {
                    return Token::Invalid;
                }
                tz.hour = hour;
                tz.minute = min;
            } else {
                if !scanner.peek().is_fixed_length_number(2)
                    || !TimeComposer::is_hour(scanner.peek().number())
                {
                    return Token::Invalid;
                }
                tz.hour = scanner.next().number();
                if !scanner.skip_symbol(b':') {
                    return Token::Invalid;
                }
                if !scanner.peek().is_fixed_length_number(2)
                    || !TimeComposer::is_minute(scanner.peek().number())
                {
                    return Token::Invalid;
                }
                tz.minute = scanner.next().number();
            }
        }
        if scanner.peek() != Token::EndOfInput {
            return Token::Invalid;
        }
    }
    // "When the time zone offset is absent, date-only forms are interpreted as
    // a UTC time and date-time forms are interpreted as a local time."
    if tz.is_empty() && time.is_empty() {
        tz.set(0);
    }
    day.is_iso_date = true;
    Token::EndOfInput
}

/// V8's `DateParser::Parse`: the ES format, then the legacy rules for what is left.
///
/// Legacy dates: any unrecognized word before the first number is ignored, and
/// parenthesized text is ignored. An unsigned number followed by ':' is a time
/// value; followed by '.' it is a time that must be followed by a second number.
/// A signed number after a time value (or after UTC) is a zone offset. Other
/// numbers are the day's parts, read as month-day-year unless the first cannot
/// be a day, and a month name stands in for the month.
fn parse(units: &[u16]) -> Option<Parsed> {
    let mut scanner = Tokenizer::new(units);
    let mut day = DayComposer {
        comp: [0; 3],
        index: 0,
        named_month: NONE,
        is_iso_date: false,
    };
    let mut time = TimeComposer {
        comp: [0; 4],
        index: 0,
        hour_offset: NONE,
    };
    let mut tz = TimeZoneComposer {
        sign: NONE,
        hour: NONE,
        minute: NONE,
    };
    let unhandled = parse_es5_date_time(&mut scanner, &mut day, &mut time, &mut tz);
    if unhandled == Token::Invalid {
        return None;
    }
    let mut has_read_number = !day.is_empty();
    let mut token = unhandled;
    while token != Token::EndOfInput {
        match token {
            Token::Number { value: n, .. } => {
                has_read_number = true;
                if scanner.skip_symbol(b':') {
                    if scanner.skip_symbol(b':') {
                        // n + "::"
                        if !time.is_empty() {
                            return None;
                        }
                        time.add(n);
                        time.add(0);
                    } else {
                        // n + ":"
                        if !time.add(n) {
                            return None;
                        }
                        if scanner.peek().is_symbol(b'.') {
                            scanner.next();
                        }
                    }
                } else if scanner.skip_symbol(b'.') && time.is_expecting(n) {
                    time.add(n);
                    if !scanner.peek().is_number() {
                        return None;
                    }
                    let ms = read_milliseconds(scanner.next());
                    time.add_final(ms);
                } else if tz.is_expecting(n) {
                    tz.minute = n;
                } else if time.is_expecting(n) {
                    time.add_final(n);
                    // End, white space, "Z", "+" or "-" must follow a finished time.
                    let peek = scanner.peek();
                    if peek != Token::EndOfInput
                        && peek != Token::WhiteSpace
                        && !peek.is_keyword_z()
                        && !peek.is_ascii_sign()
                    {
                        return None;
                    }
                } else {
                    if !day.add(n) {
                        return None;
                    }
                    scanner.skip_symbol(b'-');
                }
            }
            Token::Keyword { kind, value, .. } => {
                if kind == KeywordType::AmPm && !time.is_empty() {
                    time.hour_offset = value;
                } else if kind == KeywordType::MonthName {
                    day.named_month = value;
                    scanner.skip_symbol(b'-');
                } else if kind == KeywordType::TimeZoneName && has_read_number {
                    tz.set(value);
                } else {
                    // Garbage words are illegal once a number has been read, and
                    // the first number must be set apart from them.
                    if has_read_number || scanner.peek().is_number() {
                        return None;
                    }
                }
            }
            _ if token.is_ascii_sign() && (tz.is_utc() || !time.is_empty()) => {
                // A UTC offset, only after UTC or a time.
                tz.set_sign(token.ascii_sign());
                // The number may be missing.
                let (mut n, mut length) = (0, 0);
                if scanner.peek().is_number() {
                    let Token::Number { value, length: len } = scanner.next() else {
                        unreachable!("peeked a number");
                    };
                    (n, length) = (value, len);
                }
                has_read_number = true;
                if scanner.peek().is_symbol(b':') {
                    tz.hour = n;
                    tz.minute = NONE;
                } else if length == 1 || length == 2 {
                    // Zones like GMT-8.
                    tz.hour = n;
                    tz.minute = 0;
                } else if length == 3 || length == 4 {
                    // Zones like GMT-0800.
                    tz.hour = n / 100;
                    tz.minute = n % 100;
                } else {
                    return None;
                }
            }
            _ if (token.is_ascii_sign() || token.is_symbol(b')')) && has_read_number => {
                // An extra sign or ')' is illegal once a number has been read.
                return None;
            }
            // Other characters and white space are ignored.
            _ => {}
        }
        token = scanner.next();
    }
    let (year, month, day) = day.write()?;
    let [hour, minute, second, millisecond] = time.write()?;
    let utc_offset = tz.write()?;
    Some(Parsed {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
        utc_offset,
    })
}
