//! d3-format's number formats in its default (en-US) locale, as d3-format 3.1.2
//! writes them, and d3-scale's `tickFormat` that picks one for a scale's ticks.

use std::fmt;

use astryx_theme::authoring::js::{js_max, js_min, js_number_to_string, js_round, js_to_fixed};

use super::ticks::{js_pow10, tick_step};

/// The SI prefixes from 10^-24 to 10^24, for the `s` type and [`NumberFormat::prefix`].
const PREFIXES: [&str; 17] = [
    "y", "z", "a", "f", "p", "n", "\u{b5}", "m", "", "k", "M", "G", "T", "P", "E", "Z", "Y",
];

/// The default locale's minus sign (U+2212), which d3-format writes for negatives.
const MINUS: &str = "\u{2212}";

/// A specifier d3-format cannot read (d3-format throws `Error("invalid format: …")`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidFormat(pub String);

impl fmt::Display for InvalidFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid format: {}", self.0)
    }
}

impl std::error::Error for InvalidFormat {}

/// A parsed format specifier, `[[fill]align][sign][symbol][0][width][,][.precision][~][type]`
/// (d3-format `formatSpecifier` and `FormatSpecifier`).
#[derive(Clone, Debug, PartialEq)]
pub struct FormatSpecifier {
    /// The padding character (default a space).
    pub fill: char,
    /// `<`, `>` (the default), `=` (padding after the sign) or `^`.
    pub align: char,
    /// `-` (the default: a sign only for negatives), `+`, `(` or a space.
    pub sign: char,
    /// `""`, `"$"` (currency) or `"#"` (a `0b`/`0o`/`0x` prefix).
    pub symbol: &'static str,
    /// Whether zeros pad between the sign and the digits.
    pub zero: bool,
    /// The minimum width, if any.
    pub width: Option<f64>,
    /// Whether thousands are grouped with commas.
    pub comma: bool,
    /// The precision, if any.
    pub precision: Option<f64>,
    /// Whether insignificant trailing zeros are dropped.
    pub trim: bool,
    /// The type (upstream `type`): one of `e f g r s % p b o d x X c n`, or `None`
    /// for d3's `""`. Any other letter formats as `.12~g`.
    pub kind: Option<char>,
}

impl FormatSpecifier {
    /// Reads a specifier (d3-format `formatSpecifier(specifier)`).
    pub fn parse(specifier: &str) -> Result<Self, InvalidFormat> {
        let chars: Vec<char> = specifier.chars().collect();
        let fill_align = |c: char| matches!(c, '<' | '>' | '=' | '^');
        // d3 reads `(?:(.)?([<>=^]))?` first: a fill and an align, then an align alone,
        // then neither, keeping the first that lets the rest of the specifier read.
        let mut attempts: Vec<(Option<char>, Option<char>, usize)> = Vec::new();
        if chars.len() >= 2 && fill_align(chars[1]) && !is_line_terminator(chars[0]) {
            attempts.push((Some(chars[0]), Some(chars[1]), 2));
        }
        if chars.first().copied().is_some_and(fill_align) {
            attempts.push((None, Some(chars[0]), 1));
        }
        attempts.push((None, None, 0));
        for (fill, align, start) in attempts {
            if let Some(spec) = parse_rest(&chars[start..], fill, align) {
                return Ok(spec);
            }
        }
        Err(InvalidFormat(specifier.to_string()))
    }

    /// The specifier as d3 writes it back (d3-format `FormatSpecifier.toString`), and
    /// reads it again, which every specifier object handed to d3-format goes through:
    /// the width becomes at least 1 and the precision at least 0, both as 32-bit
    /// integers.
    fn normalized(&self) -> Self {
        Self {
            width: self.width.map(|w| f64::from(to_int32(w).max(1))),
            precision: self.precision.map(|p| f64::from(to_int32(p).max(0))),
            ..self.clone()
        }
    }
}

impl fmt::Display for FormatSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}{}", self.fill, self.align, self.sign, self.symbol)?;
        if self.zero {
            f.write_str("0")?;
        }
        if let Some(width) = self.width {
            write!(f, "{}", to_int32(width).max(1))?;
        }
        if self.comma {
            f.write_str(",")?;
        }
        if let Some(precision) = self.precision {
            write!(f, ".{}", to_int32(precision).max(0))?;
        }
        if self.trim {
            f.write_str("~")?;
        }
        if let Some(kind) = self.kind {
            write!(f, "{kind}")?;
        }
        Ok(())
    }
}

/// The characters `.` in a JavaScript regular expression does not match.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `([+\-( ])?([$#])?(0)?(\d+)?(,)?(\.\d+)?(~)?([a-z%])?$` (case-insensitive) over
/// what follows the fill and align.
fn parse_rest(chars: &[char], fill: Option<char>, align: Option<char>) -> Option<FormatSpecifier> {
    let mut at = 0;
    let peek = |at: usize| chars.get(at).copied();
    let mut sign = '-';
    if let Some(c @ ('+' | '-' | '(' | ' ')) = peek(at) {
        sign = c;
        at += 1;
    }
    let mut symbol = "";
    match peek(at) {
        Some('$') => {
            symbol = "$";
            at += 1;
        }
        Some('#') => {
            symbol = "#";
            at += 1;
        }
        _ => {}
    }
    let zero = peek(at) == Some('0');
    if zero {
        at += 1;
    }
    let digits = |at: &mut usize| {
        let begin = *at;
        while peek(*at).is_some_and(|c| c.is_ascii_digit()) {
            *at += 1;
        }
        (*at > begin).then(|| chars[begin..*at].iter().collect::<String>())
    };
    let width = digits(&mut at).map(|w| w.parse::<f64>().unwrap_or(f64::NAN));
    let comma = peek(at) == Some(',');
    if comma {
        at += 1;
    }
    let mut precision = None;
    if peek(at) == Some('.') {
        let mut after = at + 1;
        let p = digits(&mut after)?;
        precision = Some(p.parse::<f64>().unwrap_or(f64::NAN));
        at = after;
    }
    let trim = peek(at) == Some('~');
    if trim {
        at += 1;
    }
    let mut kind = None;
    if let Some(c) = peek(at).filter(|c| c.is_ascii_alphabetic() || *c == '%') {
        kind = Some(c);
        at += 1;
    }
    (at == chars.len()).then_some(FormatSpecifier {
        fill: fill.unwrap_or(' '),
        align: align.unwrap_or('>'),
        sign,
        symbol,
        zero,
        width,
        comma,
        precision,
        trim,
        kind,
    })
}

/// JavaScript's `x | 0` (`ToInt32`).
fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let m = x.trunc().rem_euclid(4_294_967_296.);
    if m >= 2_147_483_648. {
        (m - 4_294_967_296.) as i32
    } else {
        m as i32
    }
}

/// A number format (the function d3-format `format(specifier)` returns, or
/// `formatPrefix(specifier, value)`).
#[derive(Clone, Debug, PartialEq)]
pub struct NumberFormat {
    specifier: FormatSpecifier,
    fill: char,
    align: char,
    sign: char,
    zero: bool,
    width: Option<f64>,
    comma: bool,
    precision: usize,
    trim: bool,
    kind: char,
    prefix: String,
    suffix: String,
    maybe_suffix: bool,
    /// `formatPrefix`'s scale (`10^-e`), applied before formatting.
    scale: Option<f64>,
}

impl NumberFormat {
    /// The format for a specifier (d3-format `format(specifier)`).
    pub fn new(specifier: &str) -> Result<Self, InvalidFormat> {
        Ok(Self::build(FormatSpecifier::parse(specifier)?, "", None))
    }

    /// The format for a specifier object (d3-format `format(specifier)` given a
    /// `FormatSpecifier`, which d3 writes out and reads again).
    pub fn from_specifier(specifier: &FormatSpecifier) -> Self {
        Self::build(specifier.normalized(), "", None)
    }

    /// A fixed-point format that writes values in the SI unit of `value` (d3-format
    /// `formatPrefix(specifier, value)`): `prefix(",.0", 1e3)` writes 1500 as `2k`.
    pub fn prefix(specifier: &FormatSpecifier, value: f64) -> Self {
        let e = exponent(value).map(|x| x.div_euclid(3).clamp(-8, 8) * 3);
        let mut spec = specifier.normalized();
        spec.kind = Some('f');
        let suffix = e.map_or("", |e| PREFIXES[(8 + e / 3) as usize]);
        let scale = e.map_or(f64::NAN, |e| js_pow10(f64::from(-e)));
        Self::build(spec, suffix, Some(scale))
    }

    /// d3-format's `newFormat`: resolves the type, the padding, the affixes and the
    /// precision once.
    fn build(specifier: FormatSpecifier, extra_suffix: &str, scale: Option<f64>) -> Self {
        let sign = specifier.sign;
        let width = specifier.width;
        let mut fill = specifier.fill;
        let mut align = specifier.align;
        let mut zero = specifier.zero;
        let mut comma = specifier.comma;
        let mut precision = specifier.precision;
        let mut trim = specifier.trim;
        let kind = match specifier.kind {
            // The "n" type is an alias for ",g".
            Some('n') => {
                comma = true;
                'g'
            }
            Some(c) if is_format_type(c) => c,
            // The "" type, and any invalid type, is an alias for ".12~g".
            _ => {
                precision.get_or_insert(12.);
                trim = true;
                'g'
            }
        };
        // If zero fill is specified, padding goes after sign and before digits.
        if zero || (fill == '0' && align == '=') {
            zero = true;
            fill = '0';
            align = '=';
        }
        let prefix = match specifier.symbol {
            "$" => "$".to_string(),
            "#" if matches!(kind, 'b' | 'o' | 'x' | 'X') => {
                format!("0{}", kind.to_ascii_lowercase())
            }
            _ => String::new(),
        };
        let suffix = format!(
            "{}{extra_suffix}",
            if specifier.symbol == "$" {
                ""
            } else if matches!(kind, '%' | 'p') {
                "%"
            } else {
                ""
            }
        );
        let maybe_suffix = matches!(kind, 'd' | 'e' | 'f' | 'g' | 'p' | 'r' | 's' | '%');
        // Significant precision must be in [1, 21], fixed precision in [0, 20].
        let precision = match precision {
            None => 6.,
            Some(p) if matches!(kind, 'g' | 'p' | 'r' | 's') => js_max(1., js_min(21., p)),
            Some(p) => js_max(0., js_min(20., p)),
        } as usize;
        Self {
            specifier,
            fill,
            align,
            sign,
            zero,
            width,
            comma,
            precision,
            trim,
            kind,
            prefix,
            suffix,
            maybe_suffix,
            scale,
        }
    }

    /// Writes `value` in this format.
    pub fn format(&self, value: f64) -> String {
        let value = self.scale.map_or(value, |k| k * value);
        let mut value_prefix = self.prefix.clone();
        let mut value_suffix = self.suffix.clone();
        let mut digits: Vec<char>;
        if self.kind == 'c' {
            value_suffix = js_number_to_string(value) + &value_suffix;
            digits = Vec::new();
        } else {
            // Determine the sign. -0 is not less than 0, but 1 / -0 is!
            let mut negative = value < 0. || 1. / value < 0.;
            let (text, prefix_exponent) = if value.is_nan() {
                ("NaN".to_string(), None)
            } else {
                format_type(self.kind, value.abs(), self.precision)
            };
            let text = if self.trim { format_trim(&text) } else { text };
            // If a negative value rounds to zero after formatting, and no explicit
            // positive sign is requested, hide the sign.
            if negative && js_to_number(&text) == 0. && self.sign != '+' {
                negative = false;
            }
            value_prefix = format!(
                "{}{value_prefix}",
                if negative {
                    if self.sign == '(' {
                        "(".to_string()
                    } else {
                        MINUS.to_string()
                    }
                } else if self.sign == '-' || self.sign == '(' {
                    String::new()
                } else {
                    self.sign.to_string()
                }
            );
            let si = match prefix_exponent {
                Some(e) if self.kind == 's' && !js_to_number(&text).is_nan() => {
                    PREFIXES[(8 + e / 3) as usize]
                }
                _ => "",
            };
            value_suffix = format!(
                "{si}{value_suffix}{}",
                if negative && self.sign == '(' {
                    ")"
                } else {
                    ""
                }
            );
            digits = text.chars().collect();
            // Break the formatted value into the integer "value" part that can be
            // grouped, and fractional or exponential "suffix" part that is not.
            if self.maybe_suffix
                && let Some(i) = digits.iter().position(|c| !c.is_ascii_digit())
            {
                let rest: String = if digits[i] == '.' {
                    format!(".{}", digits[i + 1..].iter().collect::<String>())
                } else {
                    digits[i..].iter().collect()
                };
                value_suffix = rest + &value_suffix;
                digits.truncate(i);
            }
        }

        // If the fill character is not "0", grouping is applied before padding.
        if self.comma && !self.zero {
            digits = group(&digits, f64::INFINITY);
        }

        // Compute the padding.
        let length =
            (value_prefix.chars().count() + digits.len() + value_suffix.chars().count()) as f64;
        let mut padding: Vec<char> = match self.width {
            Some(width) if length < width && width - length <= 4_294_967_295. => {
                vec![self.fill; (width - length) as usize]
            }
            _ => Vec::new(),
        };

        // If the fill character is "0", grouping is applied after padding.
        if self.comma && self.zero {
            let width = if padding.is_empty() {
                f64::INFINITY
            } else {
                self.width.unwrap_or(f64::NAN) - value_suffix.chars().count() as f64
            };
            let mut padded = padding.clone();
            padded.extend_from_slice(&digits);
            digits = group(&padded, width);
            padding.clear();
        }

        let digits: String = digits.into_iter().collect();
        let padding: String = padding.into_iter().collect();
        match self.align {
            '<' => format!("{value_prefix}{digits}{value_suffix}{padding}"),
            '=' => format!("{value_prefix}{padding}{digits}{value_suffix}"),
            '^' => {
                let half: String = padding.chars().take(padding.chars().count() >> 1).collect();
                let rest: String = padding.chars().skip(half.chars().count()).collect();
                format!("{half}{value_prefix}{digits}{value_suffix}{rest}")
            }
            _ => format!("{padding}{value_prefix}{digits}{value_suffix}"),
        }
    }
}

impl fmt::Display for NumberFormat {
    /// The specifier (d3-format `format.toString()`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.specifier.fmt(f)
    }
}

/// d3-scale `tickFormat(start, stop, count, specifier)`: a format with the precision
/// the tick step between `start` and `stop` needs, `,f` when no specifier is given.
pub fn tick_format(
    start: f64,
    stop: f64,
    count: f64,
    specifier: Option<&FormatSpecifier>,
) -> NumberFormat {
    let step = tick_step(start, stop, count);
    let mut spec = match specifier {
        Some(spec) => spec.normalized(),
        None => {
            FormatSpecifier::parse(",f").unwrap_or_else(|_| unreachable!(",f is a valid specifier"))
        }
    };
    match spec.kind {
        Some('s') => {
            let value = js_max(start.abs(), stop.abs());
            let precision = precision_prefix(step, value);
            if spec.precision.is_none() && !precision.is_nan() {
                spec.precision = Some(precision);
            }
            return NumberFormat::prefix(&spec, value);
        }
        None | Some('e' | 'g' | 'p' | 'r') => {
            let precision = precision_round(step, js_max(start.abs(), stop.abs()));
            if spec.precision.is_none() && !precision.is_nan() {
                spec.precision = Some(precision - f64::from(u8::from(spec.kind == Some('e'))));
            }
        }
        Some('f' | '%') => {
            let precision = precision_fixed(step);
            if spec.precision.is_none() && !precision.is_nan() {
                spec.precision = Some(precision - f64::from(u8::from(spec.kind == Some('%'))) * 2.);
            }
        }
        _ => {}
    }
    NumberFormat::from_specifier(&spec)
}

/// The precision a fixed-point format needs for values `step` apart (d3-format
/// `precisionFixed`); NaN for a zero or non-finite step.
pub fn precision_fixed(step: f64) -> f64 {
    js_max(0., -exponent_or_nan(step.abs()))
}

/// The precision an SI-prefix format in the unit of `value` needs for values `step`
/// apart (d3-format `precisionPrefix`).
pub fn precision_prefix(step: f64, value: f64) -> f64 {
    let unit = js_max(-8., js_min(8., (exponent_or_nan(value) / 3.).floor())) * 3.;
    js_max(0., unit - exponent_or_nan(step.abs()))
}

/// The significant digits a format needs for values `step` apart up to `max`
/// (d3-format `precisionRound`).
pub fn precision_round(step: f64, max: f64) -> f64 {
    let step = step.abs();
    let max = max.abs() - step;
    js_max(0., exponent_or_nan(max) - exponent_or_nan(step)) + 1.
}

/// The types d3-format knows (`formatTypes`).
fn is_format_type(c: char) -> bool {
    matches!(
        c,
        '%' | 'b' | 'c' | 'd' | 'e' | 'f' | 'g' | 'o' | 'p' | 'r' | 's' | 'X' | 'x'
    )
}

/// Formats a non-negative `x` by type (d3-format `formatTypes`), with the SI exponent
/// the `s` type chose (d3-format's `prefixExponent`).
fn format_type(kind: char, x: f64, p: usize) -> (String, Option<i32>) {
    let text = match kind {
        '%' => js_to_fixed(x * 100., p),
        'b' => to_radix(js_round(x), 2),
        'd' => format_decimal(x),
        'e' => to_exponential(x, Some(p)),
        'f' => js_to_fixed(x, p),
        'g' => to_precision(x, p),
        'o' => to_radix(js_round(x), 8),
        'p' => format_rounded(x * 100., p),
        'r' => format_rounded(x, p),
        's' => return format_prefix_auto(x, p),
        'X' => to_radix(js_round(x), 16).to_uppercase(),
        'x' => to_radix(js_round(x), 16),
        _ => js_number_to_string(x),
    };
    (text, None)
}

/// d3-format `formatDecimal`: the rounded value in plain digits.
fn format_decimal(x: f64) -> String {
    let x = js_round(x);
    if x.abs() >= 1e21 {
        to_locale_digits(x)
    } else {
        js_number_to_string(x)
    }
}

/// `x.toLocaleString("en").replace(/,/g, "")` for an integer of at least 1e21: its
/// shortest digits padded with zeros, and `∞` for an infinity.
fn to_locale_digits(x: f64) -> String {
    let sign = if x < 0. { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}\u{221e}");
    }
    let (digits, e) = shortest_digits(x.abs());
    let zeros = (e + 1) as usize - digits.len();
    format!("{sign}{digits}{}", "0".repeat(zeros))
}

/// d3-format `formatRounded`: `x` to `p` significant digits, in plain notation.
fn format_rounded(x: f64, p: usize) -> String {
    let Some((coefficient, exponent)) = format_decimal_parts(x, p) else {
        return js_number_to_string(x);
    };
    let n = coefficient.chars().count() as i32;
    if exponent < 0 {
        format!("0.{}{coefficient}", "0".repeat((-exponent - 1) as usize))
    } else if n > exponent + 1 {
        let (whole, fraction) = coefficient.split_at((exponent + 1) as usize);
        format!("{whole}.{fraction}")
    } else {
        format!("{coefficient}{}", "0".repeat((exponent - n + 1) as usize))
    }
}

/// d3-format `formatPrefixAuto`: `x` to `p` significant digits in its SI unit, with
/// the unit's power of ten (`None` where d3 leaves `prefixExponent` undefined).
fn format_prefix_auto(x: f64, p: usize) -> (String, Option<i32>) {
    let Some((coefficient, exponent)) = format_decimal_parts(x, p) else {
        return (to_precision(x, p), None);
    };
    let prefix_exponent = exponent.div_euclid(3).clamp(-8, 8);
    let i = exponent - prefix_exponent * 3 + 1;
    let n = coefficient.chars().count() as i32;
    let text = if i == n {
        coefficient
    } else if i > n {
        format!("{coefficient}{}", "0".repeat((i - n) as usize))
    } else if i > 0 {
        let (whole, fraction) = coefficient.split_at(i as usize);
        format!("{whole}.{fraction}")
    } else {
        // Less than 1y!
        let digits = format_decimal_parts(x, (p as i32 + i - 1).max(0) as usize)
            .map_or(String::new(), |d| d.0);
        format!("0.{}{digits}", "0".repeat((-i) as usize))
    };
    (text, Some(prefix_exponent * 3))
}

/// d3-format `formatDecimalParts(x, p)`: the significant digits of `x` and the power
/// of ten of the first, to `p` digits (the shortest that read back for 0); `None`
/// for NaN, the infinities and zero.
fn format_decimal_parts(x: f64, p: usize) -> Option<(String, i32)> {
    if !x.is_finite() || x == 0. {
        return None;
    }
    let text = if p > 0 {
        to_exponential(x, Some(p - 1))
    } else {
        to_exponential(x, None)
    };
    let i = text.find('e')?;
    let coefficient = &text[..i];
    let coefficient = if coefficient.chars().count() > 1 {
        let mut chars = coefficient.chars();
        let first = chars.next().unwrap_or_default();
        chars.next();
        format!("{first}{}", chars.as_str())
    } else {
        coefficient.to_string()
    };
    Some((coefficient, text[i + 1..].parse().unwrap_or(0)))
}

/// d3-format's `exponent`: the power of ten of the first significant digit of `|x|`,
/// `None` for NaN, the infinities and zero.
fn exponent(x: f64) -> Option<i32> {
    format_decimal_parts(x.abs(), 0).map(|(_, e)| e)
}

/// [`exponent`], NaN where d3 gives NaN.
fn exponent_or_nan(x: f64) -> f64 {
    exponent(x).map_or(f64::NAN, f64::from)
}

/// d3-format `formatTrim`: drops insignificant zeros, `1.2000k` to `1.2k`.
fn format_trim(s: &str) -> String {
    let s: Vec<char> = s.chars().collect();
    let n = s.len();
    let (mut i0, mut i1): (isize, usize) = (-1, 0);
    let mut i = 1;
    while i < n {
        match s[i] {
            '.' => {
                i0 = i as isize;
                i1 = i;
            }
            '0' => {
                if i0 == 0 {
                    i0 = i as isize;
                }
                i1 = i;
            }
            c => {
                // `!+s[i]`: anything but a digit from 1 to 9 ends the run.
                if !matches!(c, '1'..='9') {
                    break;
                }
                if i0 > 0 {
                    i0 = 0;
                }
            }
        }
        i += 1;
    }
    if i0 > 0 {
        let mut out: String = s[..i0 as usize].iter().collect();
        out.extend(&s[i1 + 1..]);
        out
    } else {
        s.into_iter().collect()
    }
}

/// d3-format `formatGroup` for the default locale (groups of three, commas), up to
/// `width` characters.
fn group(value: &[char], width: f64) -> Vec<char> {
    let mut i = value.len() as f64;
    let mut parts: Vec<&[char]> = Vec::new();
    let mut g = 3.;
    let mut length = 0.;
    while i > 0. && g > 0. {
        if length + g + 1. > width {
            g = js_max(1., width - length);
        }
        let end = i;
        i -= g;
        parts.push(&value[i.max(0.) as usize..end as usize]);
        length += g + 1.;
        if length > width {
            break;
        }
        g = 3.;
    }
    parts.reverse();
    parts.join(&',')
}

/// JavaScript's unary `+` on a formatted value (only whether it is zero or NaN
/// matters).
fn js_to_number(s: &str) -> f64 {
    s.parse().unwrap_or(f64::NAN)
}

/// The shortest digits that read back as a positive finite `x`, and the power of ten
/// of the first, as JavaScript writes them (`("15", -7)` for 1.5e-7).
fn shortest_digits(x: f64) -> (String, i32) {
    let text = js_number_to_string(x);
    if let Some((mantissa, e)) = text.split_once('e') {
        return (mantissa.replace('.', ""), e.parse().unwrap_or(0));
    }
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    if whole == "0" {
        let zeros = fraction.len() - fraction.trim_start_matches('0').len();
        (fraction[zeros..].to_string(), -(zeros as i32) - 1)
    } else {
        let digits = format!("{whole}{fraction}");
        let digits = digits.trim_end_matches('0');
        (digits.to_string(), whole.len() as i32 - 1)
    }
}

/// `x` (positive and finite) rounded to `p` significant digits, an exact tie going to
/// the larger magnitude as JavaScript's `toPrecision` and `toExponential` do (Rust
/// breaks it to the even digit), with the power of ten of the first digit.
fn round_significant(x: f64, p: usize) -> (String, i32) {
    let split = |text: &str| -> (Vec<u8>, i32) {
        let (mantissa, e) = text.split_once('e').unwrap_or((text, "0"));
        (
            mantissa.bytes().filter(|b| *b != b'.').collect(),
            e.parse().unwrap_or(0),
        )
    };
    // Twenty more digits than kept decide the rounding, unless they read as exactly
    // half, which may be a tie or a hair either side of one: then the exact
    // expansion (at most 767 digits) decides.
    const EXTRA: usize = 20;
    let (mut digits, mut e) = split(&format!("{x:.*e}", p - 1 + EXTRA));
    if digits[p] == b'5' && digits[p + 1..].iter().all(|d| *d == b'0') {
        (digits, e) = split(&format!("{x:.1100e}"));
    }
    let up = digits[p] >= b'5';
    digits.truncate(p);
    if up {
        let mut at = p;
        loop {
            if at == 0 {
                digits.insert(0, b'1');
                digits.truncate(p);
                e += 1;
                break;
            }
            at -= 1;
            if digits[at] == b'9' {
                digits[at] = b'0';
            } else {
                digits[at] += 1;
                break;
            }
        }
    }
    (String::from_utf8(digits).unwrap_or_default(), e)
}

/// `Number.prototype.toExponential(fraction)`, or with `None` the shortest digits that
/// read back (`toExponential()`).
fn to_exponential(x: f64, fraction: Option<usize>) -> String {
    if !x.is_finite() {
        return js_number_to_string(x);
    }
    let sign = if x < 0. { "-" } else { "" };
    let (digits, e) = if x == 0. {
        ("0".repeat(fraction.unwrap_or(0) + 1), 0)
    } else {
        match fraction {
            Some(f) => round_significant(x.abs(), f + 1),
            None => shortest_digits(x.abs()),
        }
    };
    let (first, rest) = digits.split_at(1);
    let mantissa = if rest.is_empty() {
        first.to_string()
    } else {
        format!("{first}.{rest}")
    };
    format!(
        "{sign}{mantissa}e{}{}",
        if e < 0 { '-' } else { '+' },
        e.abs()
    )
}

/// `Number.prototype.toPrecision(p)`.
fn to_precision(x: f64, p: usize) -> String {
    if !x.is_finite() {
        return js_number_to_string(x);
    }
    let sign = if x < 0. { "-" } else { "" };
    let (digits, e) = if x == 0. {
        ("0".repeat(p), 0)
    } else {
        round_significant(x.abs(), p)
    };
    let p = p as i32;
    let body = if e < -6 || e >= p {
        let (first, rest) = digits.split_at(1);
        let mantissa = if rest.is_empty() {
            first.to_string()
        } else {
            format!("{first}.{rest}")
        };
        format!("{mantissa}e{}{}", if e < 0 { '-' } else { '+' }, e.abs())
    } else if e == p - 1 {
        digits
    } else if e >= 0 {
        let (whole, fraction) = digits.split_at((e + 1) as usize);
        format!("{whole}.{fraction}")
    } else {
        format!("0.{}{digits}", "0".repeat((-(e + 1)) as usize))
    };
    format!("{sign}{body}")
}

/// `Number.prototype.toString(radix)` of an integer, as V8 writes it: past 2^53 the
/// digits it cannot represent come out as zeros.
fn to_radix(x: f64, radix: u32) -> String {
    if !x.is_finite() {
        return js_number_to_string(x);
    }
    let negative = x < 0.;
    let radix_f = f64::from(radix);
    let mut integer = x.abs().floor();
    let mut out: Vec<char> = Vec::new();
    // Fill unrepresented digits with zero.
    while integer / radix_f >= 9_007_199_254_740_992. {
        integer /= radix_f;
        out.push('0');
    }
    loop {
        let remainder = integer % radix_f;
        out.push(std::char::from_digit(remainder as u32, radix).unwrap_or('0'));
        integer = (integer - remainder) / radix_f;
        if integer.is_nan() || integer <= 0. {
            break;
        }
    }
    if negative {
        out.push('-');
    }
    out.into_iter().rev().collect()
}
