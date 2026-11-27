//! Chart data: rows of named values (upstream `Record<string, unknown>[]`), with
//! JavaScript's coercions where upstream reads them (`typeof v === 'number'`,
//! `Number(v)`, `String(v)`).

use std::rc::Rc;

use astryx_theme::authoring::js::{js_number_to_string, js_parse_float, js_trim};
use gpui::SharedString;

/// One value in a row.
#[derive(Clone, Debug, PartialEq)]
pub enum ChartValue {
    /// `null`.
    Null,
    Bool(bool),
    /// A JavaScript number, NaN and the infinities included.
    Number(f64),
    Text(SharedString),
    /// A JavaScript `Date`: milliseconds since the Unix epoch (NaN for an invalid date).
    Date(f64),
}

impl ChartValue {
    /// A `Date` at `ms` milliseconds since the Unix epoch.
    pub fn date_ms(ms: f64) -> Self {
        ChartValue::Date(ms)
    }

    /// `typeof v === 'number'` (NaN included).
    pub fn is_number(&self) -> bool {
        matches!(self, ChartValue::Number(_))
    }

    /// `Number.isFinite(v)`: a finite number, without coercion.
    pub fn finite(&self) -> Option<f64> {
        match self {
            ChartValue::Number(n) if n.is_finite() => Some(*n),
            _ => None,
        }
    }

    /// `Number(v)`.
    pub fn to_number(&self) -> f64 {
        match self {
            ChartValue::Null => 0.,
            ChartValue::Bool(b) => f64::from(u8::from(*b)),
            ChartValue::Number(n) | ChartValue::Date(n) => *n,
            ChartValue::Text(s) => string_to_number(s),
        }
    }

    /// `String(v)`. A date is its ISO form here (upstream's `Date.prototype.toString`
    /// names the local zone, which no chart text relies on).
    pub fn to_js_string(&self) -> SharedString {
        match self {
            ChartValue::Null => "null".into(),
            ChartValue::Bool(b) => if *b { "true" } else { "false" }.into(),
            ChartValue::Number(n) => js_number_to_string(*n).into(),
            ChartValue::Text(s) => s.clone(),
            ChartValue::Date(ms) => date_string(*ms).into(),
        }
    }
}

/// `String(v)` of a value that may be missing (`undefined`).
pub fn js_string(value: Option<&ChartValue>) -> SharedString {
    value.map_or_else(|| "undefined".into(), ChartValue::to_js_string)
}

/// `Number(v)` of a value that may be missing (`undefined` is NaN).
pub fn js_number(value: Option<&ChartValue>) -> f64 {
    value.map_or(f64::NAN, ChartValue::to_number)
}

/// `Number(string)`: blank is 0, else the whole trimmed text as a number literal
/// (decimal, `Infinity`, or `0x`/`0o`/`0b` integers), else NaN.
fn string_to_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = t.strip_prefix(prefix) {
            return u128::from_str_radix(digits, radix).map_or(f64::NAN, |n| n as f64);
        }
    }
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    if body == "Infinity" {
        return if t.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let valid = !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && body
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.');
    if !valid {
        return f64::NAN;
    }
    let n = js_parse_float(t);
    // parseFloat stops at the first bad character; Number() needs all of it.
    let reparsed: Result<f64, _> = t.parse();
    match reparsed {
        Ok(v) => v,
        Err(_)
            if t.ends_with('.') || t.starts_with('.') || t.contains(".e") || t.contains(".E") =>
        {
            n
        }
        Err(_) => f64::NAN,
    }
}

/// `Date.prototype.toISOString`, or `Invalid Date`.
fn date_string(ms: f64) -> String {
    if !ms.is_finite() {
        return "Invalid Date".into();
    }
    let secs = (ms / 1000.).floor();
    let millis = (ms - secs * 1000.) as i64;
    let days = (secs / 86_400.).floor() as i64;
    let rem = secs as i64 - days * 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
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
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

impl From<f64> for ChartValue {
    fn from(n: f64) -> Self {
        ChartValue::Number(n)
    }
}

impl From<f32> for ChartValue {
    fn from(n: f32) -> Self {
        ChartValue::Number(f64::from(n))
    }
}

impl From<i32> for ChartValue {
    fn from(n: i32) -> Self {
        ChartValue::Number(f64::from(n))
    }
}

impl From<i64> for ChartValue {
    fn from(n: i64) -> Self {
        ChartValue::Number(n as f64)
    }
}

impl From<u32> for ChartValue {
    fn from(n: u32) -> Self {
        ChartValue::Number(f64::from(n))
    }
}

impl From<usize> for ChartValue {
    fn from(n: usize) -> Self {
        ChartValue::Number(n as f64)
    }
}

impl From<bool> for ChartValue {
    fn from(b: bool) -> Self {
        ChartValue::Bool(b)
    }
}

impl From<&'static str> for ChartValue {
    fn from(s: &'static str) -> Self {
        ChartValue::Text(s.into())
    }
}

impl From<String> for ChartValue {
    fn from(s: String) -> Self {
        ChartValue::Text(s.into())
    }
}

impl From<SharedString> for ChartValue {
    fn from(s: SharedString) -> Self {
        ChartValue::Text(s)
    }
}

impl<T: Into<ChartValue>> From<Option<T>> for ChartValue {
    fn from(v: Option<T>) -> Self {
        v.map_or(ChartValue::Null, Into::into)
    }
}

/// One row (upstream a `Record<string, unknown>`): values by field name, in the
/// order they were set. A field it lacks reads as `undefined` (`None`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Datum(Rc<Vec<(SharedString, ChartValue)>>);

impl Datum {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `key` to `value`, replacing an earlier value.
    pub fn with(mut self, key: impl Into<SharedString>, value: impl Into<ChartValue>) -> Self {
        let key = key.into();
        let value = value.into();
        let fields = Rc::make_mut(&mut self.0);
        match fields.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => fields.push((key, value)),
        }
        self
    }

    /// The value of `key`, `None` when the row lacks it (`undefined`).
    pub fn get(&self, key: &str) -> Option<&ChartValue> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// `Number.isFinite(d[key])`: the field as a finite number, without coercion.
    pub fn finite(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(ChartValue::finite)
    }

    /// `typeof d[key] === 'number' && Number.isFinite(d[key]) ? d[key] : NaN`.
    pub fn number_or_nan(&self, key: &str) -> f64 {
        self.finite(key).unwrap_or(f64::NAN)
    }

    /// `String(d[key])`.
    pub fn string(&self, key: &str) -> SharedString {
        js_string(self.get(key))
    }

    /// The fields, in order.
    pub fn fields(&self) -> impl Iterator<Item = (&SharedString, &ChartValue)> {
        self.0.iter().map(|(k, v)| (k, v))
    }
}

/// A row from `key => value` pairs: `datum! { "month" => "Jan", "revenue" => 120 }`.
#[macro_export]
macro_rules! datum {
    ($($key:expr => $value:expr),* $(,)?) => {
        $crate::Datum::new()$(.with($key, $value))*
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_coerces_as_javascript_does() {
        let n = |v: ChartValue| v.to_number();
        assert_eq!(n(ChartValue::Null), 0.);
        assert_eq!(n(true.into()), 1.);
        assert_eq!(n("".into()), 0.);
        assert_eq!(n("  12.5 ".into()), 12.5);
        assert_eq!(n("1e3".into()), 1000.);
        assert_eq!(n("0x10".into()), 16.);
        assert_eq!(n("-Infinity".into()), f64::NEG_INFINITY);
        assert_eq!(n(".5".into()), 0.5);
        assert_eq!(n("5.".into()), 5.);
        assert!(n("12px".into()).is_nan());
        assert!(n("Jan".into()).is_nan());
        assert!(js_number(None).is_nan());
    }

    #[test]
    fn strings_read_as_javascript_writes_them() {
        assert_eq!(ChartValue::from(3.0).to_js_string(), "3");
        assert_eq!(
            ChartValue::from(0.1 + 0.2).to_js_string(),
            "0.30000000000000004"
        );
        assert_eq!(ChartValue::from(1e21).to_js_string(), "1e+21");
        assert_eq!(js_string(None), "undefined");
        assert_eq!(
            ChartValue::Date(0.).to_js_string(),
            "1970-01-01T00:00:00.000Z"
        );
        assert_eq!(
            ChartValue::Date(1_700_000_000_000.).to_js_string(),
            "2023-11-14T22:13:20.000Z"
        );
    }

    #[test]
    fn a_row_reads_its_fields() {
        let d = datum! { "month" => "Jan", "revenue" => 120, "bad" => f64::NAN };
        assert_eq!(d.finite("revenue"), Some(120.));
        assert_eq!(d.finite("bad"), None);
        assert!(d.number_or_nan("month").is_nan());
        assert_eq!(d.string("month"), "Jan");
        assert_eq!(d.get("missing"), None);
        assert_eq!(d.clone().with("revenue", 5).finite("revenue"), Some(5.));
    }
}
