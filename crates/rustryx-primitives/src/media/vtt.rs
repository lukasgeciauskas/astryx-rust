//! WebVTT parsing for caption and subtitle tracks (the cues a browser reads from a
//! `<track>`): each cue's times and its text, tags removed and entities decoded.

use gpui::SharedString;

/// One cue: shown from `start` until `end`, in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct VttCue {
    pub start: f64,
    pub end: f64,
    pub text: SharedString,
}

/// A WebVTT timestamp: `hh:mm:ss.ttt` or `mm:ss.ttt`.
fn timestamp(text: &str) -> Option<f64> {
    let text = text.trim();
    let (clock, fraction) = text.split_once('.')?;
    if fraction.len() != 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<&str> = clock.split(':').collect();
    let number = |s: &str| -> Option<f64> {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let (hours, minutes, seconds) = match parts.as_slice() {
        [m, s] => (0., number(m)?, number(s)?),
        [h, m, s] => (number(h)?, number(m)?, number(s)?),
        _ => return None,
    };
    if minutes >= 60. || seconds >= 60. {
        return None;
    }
    Some(hours * 3600. + minutes * 60. + seconds + number(fraction)? / 1000.)
}

/// A cue's text: tags (`<i>`, `<c.red>`, `<00:01.000>`) removed, entities decoded.
fn cue_text(lines: &[&str]) -> String {
    let joined = lines.join("\n");
    let mut out = String::with_capacity(joined.len());
    let mut in_tag = false;
    for c in joined.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", "\u{A0}")
        .replace("&lrm;", "\u{200E}")
        .replace("&rlm;", "\u{200F}")
        .replace("&amp;", "&")
}

/// The cues of a WebVTT file, in order. Blocks without a valid timing line (the
/// header, `NOTE`, `STYLE` and `REGION` blocks) are skipped.
pub fn parse_vtt(text: &str) -> Vec<VttCue> {
    let text = text
        .strip_prefix('\u{FEFF}')
        .unwrap_or(text)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        let Some(at) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let (start, rest) = lines[at].split_once("-->").expect("an arrow");
        let end = rest.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (timestamp(start), timestamp(end)) else {
            continue;
        };
        cues.push(VttCue {
            start,
            end,
            text: cue_text(&lines[at + 1..]).into(),
        });
    }
    cues
}
