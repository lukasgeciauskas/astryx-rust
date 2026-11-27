//! JavaScript's number and string rules, which upstream's theme code runs on:
//! `Math.round`, `Math.min`/`Math.max` with NaN, `Math.hypot`, `trim`,
//! `parseFloat`, `parseInt(…, 16)`, `toString(16)`, `String(n)` and `toFixed`.
//! The authoring modules write their values through these, so a token value is the
//! string upstream's template literal writes (`14px`, `0.0313`), where Rust's own
//! formatting breaks ties and picks digits differently.

/// JavaScript's `Math.round`: the nearest integer, a half rounding up (toward
/// positive infinity, so `-2.5` is `-2`), where Rust's [`f64::round`] rounds a
/// half away from zero. A result of zero keeps the sign of `value`, as
/// `Math.round(-0.2)` is `-0`.
pub fn js_round(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let floor = value.floor();
    let rounded = if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    if rounded == 0.0 {
        0.0_f64.copysign(value)
    } else {
        rounded
    }
}

/// `Math.min` of two numbers: NaN if either is NaN (where [`f64::min`]
/// ignores a NaN), and -0 below +0.
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        if a.is_sign_negative() { a } else { b }
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.max` of two numbers: NaN if either is NaN (where [`f64::max`]
/// ignores a NaN), and +0 above -0.
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        if a.is_sign_negative() { b } else { a }
    } else if a > b {
        a
    } else {
        b
    }
}

/// `Math.hypot` of two numbers as V8 computes it: each scaled by the larger
/// and the squares summed with Kahan compensation. It can differ from
/// [`f64::hypot`] in the last bit.
pub fn js_hypot(a: f64, b: f64) -> f64 {
    let mut absolute = [0.0; 2];
    let mut any_nan = false;
    let mut max: f64 = 0.0;
    for (index, value) in [a, b].into_iter().enumerate() {
        if value.is_nan() {
            any_nan = true;
        } else {
            absolute[index] = value.abs();
            if absolute[index] > max {
                max = absolute[index];
            }
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if any_nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum: f64 = 0.0;
    let mut compensation = 0.0;
    for value in absolute {
        let n = value / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// Whether `String.prototype.trim` and a regex `\s` count a character as
/// whitespace: JavaScript's WhiteSpace and LineTerminator sets, which differ
/// from [`char::is_whitespace`] at U+0085 and U+FEFF.
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{D}'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// `String.prototype.trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

/// `parseFloat`: the longest prefix of the string (after leading whitespace)
/// that reads as a decimal number or `Infinity`, or NaN when there is none
/// (`"12px"` is 12, `".5"` is 0.5, `"px"` is NaN).
pub fn js_parse_float(s: &str) -> f64 {
    let s = s.trim_start_matches(is_js_whitespace);
    let bytes = s.as_bytes();
    let digits_from = |mut at: usize| {
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        at
    };
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    if s[end..].starts_with("Infinity") {
        return if bytes[0] == b'-' {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let integer_end = digits_from(end);
    let mut digits = integer_end - end;
    end = integer_end;
    if bytes.get(end) == Some(&b'.') {
        let fraction_end = digits_from(end + 1);
        digits += fraction_end - (end + 1);
        if digits > 0 {
            end = fraction_end;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut at = end + 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let exponent_end = digits_from(at);
        if exponent_end > at {
            end = exponent_end;
        }
    }
    s[..end].parse().unwrap_or(f64::NAN)
}

/// `parseInt(s, 16)` over UTF-16 code units, as JavaScript strings are:
/// leading whitespace, a sign and a `0x` prefix are skipped, then the longest
/// run of hex digits is read; NaN when there is none.
pub fn js_parse_int_hex(units: &[u16]) -> f64 {
    let char_at = |at: usize| {
        units
            .get(at)
            .and_then(|unit| char::from_u32(u32::from(*unit)))
    };
    let mut at = 0;
    while char_at(at).is_some_and(is_js_whitespace) {
        at += 1;
    }
    let mut sign = 1.0;
    if let Some(c @ ('+' | '-')) = char_at(at) {
        if c == '-' {
            sign = -1.0;
        }
        at += 1;
    }
    if char_at(at) == Some('0') && matches!(char_at(at + 1), Some('x' | 'X')) {
        at += 2;
    }
    let mut value = 0.0;
    let mut digits = 0;
    while let Some(digit) = char_at(at).and_then(|c| c.to_digit(16)) {
        value = value * 16.0 + f64::from(digit);
        digits += 1;
        at += 1;
    }
    if digits == 0 { f64::NAN } else { sign * value }
}

/// `Number.prototype.toString(16)` of a whole number, lowercase (-127 is
/// `"-7f"`; NaN and the infinities print as JavaScript prints them). Callers
/// round first, so fractions never reach it.
pub fn js_to_hex_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n < 0.0 {
        return format!("-{}", js_to_hex_string(-n));
    }
    if n < 18_446_744_073_709_551_616.0 {
        return format!("{:x}", n as u64);
    }
    // From 2^64 up the number is its 53-bit mantissa times a power of two of
    // at least 2^12, so its digits are the mantissa's, shifted, then zeros.
    let bits = n.to_bits();
    let mantissa = (bits & ((1 << 52) - 1)) | (1 << 52);
    let shift = ((bits >> 52) & 0x7ff) - 1075;
    format!(
        "{:x}{}",
        mantissa << (shift % 4),
        "0".repeat((shift / 4) as usize)
    )
}

/// `Number.prototype.toString()` (and `String(n)`): the shortest digits that
/// read back as `n`, in plain notation from 1e-6 up to 1e21 and in exponent
/// notation (`1e+21`, `1.5e-7`) outside that. -0 prints as `0`.
pub fn js_number_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n == 0.0 {
        return "0".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n < 0.0 {
        return format!("-{}", js_number_to_string(-n));
    }
    let (digits, exponent) = shortest_digits(n);
    let k = digits.len() as i64;
    let point = exponent + 1;
    if k <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - k) as usize))
    } else if 0 < point && point <= 21 {
        let (whole, fraction) = digits.split_at(point as usize);
        format!("{whole}.{fraction}")
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else {
        let exponent = point - 1;
        let sign = if exponent >= 0 { '+' } else { '-' };
        let (first, rest) = digits.split_at(1);
        if rest.is_empty() {
            format!("{first}e{sign}{}", exponent.abs())
        } else {
            format!("{first}.{rest}e{sign}{}", exponent.abs())
        }
    }
}

/// The shortest digits that read back as a positive finite `n`, and the
/// power of ten of the first: `("15", -7)` for 1.5e-7. Rust's `{:e}` finds
/// the same digits as JavaScript except at an exact tie between two
/// candidates, which Rust breaks upward and JavaScript breaks to the even one
/// (869631767272949.25 prints as `869631767272949.2`).
fn shortest_digits(n: f64) -> (String, i64) {
    let split = |formatted: &str| {
        let (mantissa, exponent) = formatted.split_once('e').unwrap_or((formatted, "0"));
        let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
        (digits, exponent.parse::<i64>().unwrap_or(0))
    };
    let (mut digits, exponent) = split(&format!("{n:e}"));
    let last = digits.as_bytes()[digits.len() - 1];
    if last % 2 == 1 {
        // A tie means `n` is exactly the midpoint: the digits one lower,
        // then a 5. Exact digits run to at most 767 places.
        let mut lower = digits.clone().into_bytes();
        let end = lower.len() - 1;
        lower[end] -= 1;
        let lower = String::from_utf8(lower).unwrap_or_default();
        let (exact, exact_exponent) = split(&format!("{n:.1100e}"));
        let is_tie =
            exact_exponent == exponent && exact.trim_end_matches('0') == format!("{lower}5");
        let reads_back = format!("{lower}e{}", exponent - (digits.len() as i64 - 1))
            .parse::<f64>()
            .is_ok_and(|value| value == n);
        if is_tie && reads_back {
            digits = lower.trim_end_matches('0').to_string();
        }
    }
    (digits, exponent)
}

/// `Number.prototype.toFixed(digits)`: the value rounded to `digits`
/// decimals, an exact tie going to the larger magnitude (0.03125 to four
/// places is `"0.0313"`, where Rust's `{:.4}` gives `"0.0312"`). From 1e21
/// up it prints as [`js_number_to_string`] does.
pub fn js_to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return js_number_to_string(x);
    }
    let magnitude = x.abs();
    let body = if magnitude >= 1e21 {
        js_number_to_string(magnitude)
    } else if is_fixed_tie(magnitude, digits) {
        // The value has exactly `digits + 1` decimals, the last a 5: drop it
        // and round what is left up.
        let exact = format!("{magnitude:.*}", digits + 1);
        let mut kept = exact[..exact.len() - 1]
            .trim_end_matches('.')
            .as_bytes()
            .to_vec();
        let mut at = kept.len();
        loop {
            if at == 0 {
                kept.insert(0, b'1');
                break;
            }
            at -= 1;
            match kept[at] {
                b'.' => {}
                b'9' => kept[at] = b'0',
                digit => {
                    kept[at] = digit + 1;
                    break;
                }
            }
        }
        String::from_utf8(kept).unwrap_or_default()
    } else {
        format!("{magnitude:.*}", digits)
    };
    if x < 0.0 { format!("-{body}") } else { body }
}

/// Whether a positive finite `x` lies exactly halfway between two
/// `digits`-decimal numbers: `2·x·10^digits` is an odd integer, which holds
/// exactly when `x` is an odd integer times `2^-(digits + 1)`.
fn is_fixed_tie(x: f64, digits: usize) -> bool {
    if x == 0.0 {
        return false;
    }
    let bits = x.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i64;
    let fraction = bits & ((1 << 52) - 1);
    let (mantissa, exponent) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), biased - 1075)
    };
    exponent + i64::from(mantissa.trailing_zeros()) == -(digits as i64 + 1)
}
