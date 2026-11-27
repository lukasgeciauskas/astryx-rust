//! Upstream lab `DiffViewer/parseUnifiedDiff.ts`: a unified diff (`git diff`,
//! `diff -u`, or a bare hunk) into one file per file. Tolerant by design: unknown header
//! lines are skipped, a hunk whose counts run short ends where its lines end, and input
//! with no recognizable file or hunk gives none. It never fails. Offsets that upstream
//! counts in UTF-16 code units (the middle of a `diff --git` line) are counted so here.

use gpui::SharedString;

use super::types::{
    DiffViewerFile, DiffViewerFileStatus, DiffViewerHunk, DiffViewerLine, DiffViewerLineKind,
};

const GIT_HEADER_PREFIX: &str = "diff --git ";
const BINARY_PREFIX: &str = "Binary files ";
const BINARY_SUFFIX: &str = " differ";
const BINARY_SEPARATOR: &str = " and ";

/// Upstream `parseBinaryLine`: the two paths of a `Binary files X and Y differ` line,
/// split at the last ` and `.
fn parse_binary_line(line: &str) -> Option<(String, String)> {
    if !line.starts_with(BINARY_PREFIX)
        || !line.ends_with(BINARY_SUFFIX)
        || line.len() < BINARY_PREFIX.len() + BINARY_SUFFIX.len()
    {
        return None;
    }
    let middle = &line[BINARY_PREFIX.len()..line.len() - BINARY_SUFFIX.len()];
    let split = middle.rfind(BINARY_SEPARATOR)?;
    if split == 0 || split + BINARY_SEPARATOR.len() >= middle.len() {
        return None;
    }
    Some((
        middle[..split].to_string(),
        middle[split + BINARY_SEPARATOR.len()..].to_string(),
    ))
}

fn c_escape(unit: u16) -> Option<u8> {
    Some(match unit {
        0x61 => 0x07,
        0x62 => 0x08,
        0x74 => 0x09,
        0x6E => 0x0A,
        0x76 => 0x0B,
        0x66 => 0x0C,
        0x72 => 0x0D,
        0x22 => 0x22,
        0x5C => 0x5C,
        _ => return None,
    })
}

/// Upstream `readQuoted`: a git C-style quoted path from the opening quote at `start`
/// (UTF-16 offsets), its octal escapes decoded as UTF-8 bytes; the path and the offset
/// past the closing quote, or `None` when the quote never closes.
fn read_quoted(text: &[u16], start: usize) -> Option<(String, usize)> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut i = start + 1;
    while i < text.len() {
        let ch = text[i];
        if ch == u16::from(b'"') {
            return Some((String::from_utf8_lossy(&bytes).into_owned(), i + 1));
        }
        if ch == u16::from(b'\\') && i + 1 < text.len() {
            let next = text[i + 1];
            let octal = text
                .get(i + 1..(i + 4).min(text.len()))
                .filter(|s| s.len() == 3 && s.iter().all(|u| (0x30..=0x37).contains(u)));
            if let Some(octal) = octal {
                let value = octal.iter().fold(0u32, |n, u| n * 8 + u32::from(u - 0x30));
                bytes.push((value & 0xFF) as u8);
                i += 4;
                continue;
            }
            if let Some(byte) = c_escape(next) {
                bytes.push(byte);
                i += 2;
                continue;
            }
        }
        // Any other character stands for itself (as UTF-8 bytes); a lone surrogate
        // encodes as U+FFFD, as TextEncoder writes it.
        let (c, width) = match char::decode_utf16(text[i..].iter().copied()).next() {
            Some(Ok(c)) => (c, c.len_utf16()),
            _ => ('\u{FFFD}', 1),
        };
        let mut buf = [0u8; 4];
        bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        i += width;
    }
    None
}

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn from_utf16(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// Upstream `unquotePath`: the path written in `raw`, decoded when git quoted it.
fn unquote_path(raw: &str) -> String {
    if raw.starts_with('"')
        && let Some((value, _)) = read_quoted(&utf16(raw), 0)
    {
        return value;
    }
    raw.to_string()
}

fn strip_prefix<'a>(path: &'a str, prefix: &str) -> &'a str {
    path.strip_prefix(prefix).unwrap_or(path)
}

/// JavaScript's `lastIndexOf(needle, from)` over UTF-16 units.
fn last_index_of(haystack: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    let mut i = from.min(haystack.len() - needle.len());
    loop {
        if haystack[i..i + needle.len()] == *needle {
            return Some(i);
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

/// Upstream `parseGitHeader`: the two paths of a `diff --git` line.
fn parse_git_header(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix(GIT_HEADER_PREFIX)?;
    let paths = |left: &str, right: &str| {
        (
            strip_prefix(left, "a/").to_string(),
            strip_prefix(right, "b/").to_string(),
        )
    };
    let units = utf16(rest);
    if rest.starts_with('"')
        && let Some((left, end)) = read_quoted(&units, 0)
        && units.get(end) == Some(&u16::from(b' '))
    {
        return Some(paths(&left, &unquote_path(&from_utf16(&units[end + 1..]))));
    }
    if rest.ends_with('"') {
        // Only the new side is quoted: it starts at the last ` "` that reads to the
        // end of the line as one quoted path.
        let needle = utf16(" \"");
        let mut split = last_index_of(&units, &needle, units.len());
        while let Some(at) = split.filter(|at| *at > 0) {
            if let Some((right, end)) = read_quoted(&units, at + 1)
                && end == units.len()
            {
                return Some(paths(&from_utf16(&units[..at]), &right));
            }
            split = last_index_of(&units, &needle, at - 1);
        }
    }
    if units.len() % 2 == 1 {
        let middle = (units.len() - 1) / 2;
        let left = from_utf16(&units[..middle]);
        let right = from_utf16(&units[middle + 1..]);
        if units[middle] == u16::from(b' ')
            && strip_prefix(&left, "a/") == strip_prefix(&right, "b/")
        {
            return Some(paths(&left, &right));
        }
    }
    let space = units.iter().position(|u| *u == u16::from(b' '))?;
    if space == 0 {
        return None;
    }
    Some(paths(
        &from_utf16(&units[..space]),
        &from_utf16(&units[space + 1..]),
    ))
}

#[derive(Default)]
struct MutableFile {
    old_path: Option<String>,
    new_path: Option<String>,
    status: Option<DiffViewerFileStatus>,
    is_binary: bool,
    hunks: Vec<DiffViewerHunk>,
}

/// JavaScript's `trim()` whitespace.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Upstream `cleanPath`: drops a trailing tab and timestamp, decodes a quoted path,
/// strips the `a/` or `b/` prefix; `/dev/null` is no path.
fn clean_path(raw: &str, prefix: &str) -> Option<String> {
    let first = raw.split('\t').next().unwrap_or("");
    let path = unquote_path(first.trim_matches(is_js_space));
    if path == "/dev/null" {
        return None;
    }
    Some(
        path.strip_prefix(prefix)
            .map(str::to_string)
            .unwrap_or(path),
    )
}

fn finalize(file: MutableFile) -> DiffViewerFile {
    let status = file
        .status
        .unwrap_or(match (&file.old_path, &file.new_path) {
            (None, Some(_)) => DiffViewerFileStatus::Added,
            (Some(_), None) => DiffViewerFileStatus::Deleted,
            // Only git's rename lines say a file moved; two different paths without
            // them are an edit between two files, so both paths are kept.
            _ => DiffViewerFileStatus::Modified,
        });
    DiffViewerFile {
        old_path: if status == DiffViewerFileStatus::Added {
            None
        } else {
            file.old_path.map(Into::into)
        },
        new_path: if status == DiffViewerFileStatus::Deleted {
            None
        } else {
            file.new_path.map(Into::into)
        },
        status,
        is_binary: file.is_binary,
        hunks: file.hunks,
    }
}

fn has_content(file: &MutableFile) -> bool {
    !file.hunks.is_empty()
        || file.is_binary
        || file.old_path.is_some()
        || file.new_path.is_some()
        || file.status.is_some()
}

/// `Number()` of a run of digits, saturating where JavaScript's number would lose
/// precision past `u64`.
fn number(digits: &str) -> u64 {
    digits.parse::<f64>().map_or(0, |n| n as u64)
}

/// Upstream `HUNK_HEADER` (`/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/`).
fn hunk_header(line: &str) -> Option<(u64, Option<u64>, u64, Option<u64>)> {
    let rest = line.strip_prefix("@@ -")?;
    let digits = |s: &str| -> (usize, Option<u64>) {
        let n = s.bytes().take_while(u8::is_ascii_digit).count();
        (n, (n > 0).then(|| number(&s[..n])))
    };
    let (n, old_start) = digits(rest);
    let old_start = old_start?;
    let mut rest = &rest[n..];
    let mut old_lines = None;
    if let Some(after) = rest.strip_prefix(',') {
        let (n, value) = digits(after);
        if let Some(value) = value {
            old_lines = Some(value);
            rest = &after[n..];
        }
    }
    let rest = rest.strip_prefix(" +")?;
    let (n, new_start) = digits(rest);
    let new_start = new_start?;
    let mut rest = &rest[n..];
    let mut new_lines = None;
    if let Some(after) = rest.strip_prefix(',') {
        let (n, value) = digits(after);
        if let Some(value) = value {
            new_lines = Some(value);
            rest = &after[n..];
        }
    }
    rest.starts_with(" @@")
        .then_some((old_start, old_lines, new_start, new_lines))
}

/// Upstream `parseUnifiedDiff`: one [`DiffViewerFile`] per file of `patch`.
///
/// Reads `git diff` output (with `diff --git`, mode, rename, copy and binary lines),
/// classic `diff -u` output (`---`/`+++` pairs), several files in one patch, and a bare
/// `@@` hunk with no file header (one file with no paths).
pub fn parse_unified_diff(patch: &str) -> Vec<DiffViewerFile> {
    let lines: Vec<&str> = patch
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut files: Vec<DiffViewerFile> = Vec::new();
    let mut file: Option<MutableFile> = None;
    // The open hunk: its index in the file's hunks.
    let mut hunk: Option<usize> = None;
    let (mut old_remaining, mut new_remaining) = (0i128, 0i128);
    let (mut old_line, mut new_line) = (0u64, 0u64);

    fn flush(
        file: &mut Option<MutableFile>,
        hunk: &mut Option<usize>,
        files: &mut Vec<DiffViewerFile>,
    ) {
        if let Some(f) = file.take()
            && has_content(&f)
        {
            files.push(finalize(f));
        }
        *hunk = None;
    }

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        // Inside a hunk: consume body lines until both counts are spent.
        if let (Some(h), Some(f)) = (hunk, file.as_mut())
            && (old_remaining > 0 || new_remaining > 0)
        {
            let glyph = line.chars().next();
            let push = |f: &mut MutableFile, kind, old, new| {
                f.hunks[h].lines.push(DiffViewerLine {
                    kind,
                    content: SharedString::from(line.get(1..).unwrap_or("").to_string()),
                    old_line_number: old,
                    new_line_number: new,
                })
            };
            match glyph {
                // "\ No newline at end of file" annotates the previous line.
                Some('\\') => {
                    i += 1;
                    continue;
                }
                Some('+') => {
                    push(f, DiffViewerLineKind::Added, None, Some(new_line));
                    new_line = new_line.saturating_add(1);
                    new_remaining -= 1;
                    i += 1;
                    continue;
                }
                Some('-') => {
                    push(f, DiffViewerLineKind::Removed, Some(old_line), None);
                    old_line = old_line.saturating_add(1);
                    old_remaining -= 1;
                    i += 1;
                    continue;
                }
                Some(' ') | None => {
                    // Some tools strip the trailing space of an empty context line.
                    if line.is_empty() && i == lines.len() - 1 {
                        i += 1;
                        continue;
                    }
                    push(
                        f,
                        DiffViewerLineKind::Context,
                        Some(old_line),
                        Some(new_line),
                    );
                    old_line = old_line.saturating_add(1);
                    new_line = new_line.saturating_add(1);
                    old_remaining -= 1;
                    new_remaining -= 1;
                    i += 1;
                    continue;
                }
                // Anything else ends a short hunk; read it as a header.
                _ => hunk = None,
            }
        }
        if hunk.is_some() && line.starts_with('\\') {
            i += 1;
            continue;
        }

        if let Some((old, new)) = parse_git_header(line) {
            flush(&mut file, &mut hunk, &mut files);
            file = Some(MutableFile {
                old_path: Some(old),
                new_path: Some(new),
                ..MutableFile::default()
            });
            i += 1;
            continue;
        }

        if line.starts_with("--- ")
            && lines
                .get(i + 1)
                .is_some_and(|next| next.starts_with("+++ "))
        {
            // A pair opens a new file unless it belongs to a `diff --git` header whose
            // hunks have not started yet. A binary entry never takes a pair.
            if file
                .as_ref()
                .is_none_or(|f| !f.hunks.is_empty() || f.is_binary)
            {
                flush(&mut file, &mut hunk, &mut files);
                file = Some(MutableFile::default());
            }
            let current = file.as_mut().expect("a file");
            current.old_path = clean_path(&line[4..], "a/");
            current.new_path = clean_path(&lines[i + 1][4..], "b/");
            if current.old_path.is_none() && current.status.is_none() {
                current.status = Some(DiffViewerFileStatus::Added);
            } else if current.new_path.is_none() && current.status.is_none() {
                current.status = Some(DiffViewerFileStatus::Deleted);
            }
            i += 2;
            continue;
        }

        if let Some((old_start, old_lines, new_start, new_lines)) = hunk_header(line) {
            let current = file.get_or_insert_with(MutableFile::default);
            let old_lines = old_lines.unwrap_or(1);
            let new_lines = new_lines.unwrap_or(1);
            current.hunks.push(DiffViewerHunk {
                header: line.to_string().into(),
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
                is_replaced_block: false,
            });
            hunk = Some(current.hunks.len() - 1);
            old_remaining = i128::from(old_lines);
            new_remaining = i128::from(new_lines);
            // A zero-length side's start names the line before the hunk.
            old_line = if old_lines == 0 {
                old_start.saturating_add(1)
            } else {
                old_start
            };
            new_line = if new_lines == 0 {
                new_start.saturating_add(1)
            } else {
                new_start
            };
            i += 1;
            continue;
        }

        // GNU `diff -r` reports a binary file with this one line and no header.
        if let Some((old, new)) = parse_binary_line(line)
            && file
                .as_ref()
                .is_none_or(|f| !f.hunks.is_empty() || f.is_binary)
        {
            flush(&mut file, &mut hunk, &mut files);
            file = Some(MutableFile {
                old_path: clean_path(&old, "a/"),
                new_path: clean_path(&new, "b/"),
                is_binary: true,
                ..MutableFile::default()
            });
            i += 1;
            continue;
        }

        if let Some(current) = file.as_mut() {
            if line.starts_with("new file mode") {
                current.status = Some(DiffViewerFileStatus::Added);
            } else if line.starts_with("deleted file mode") {
                current.status = Some(DiffViewerFileStatus::Deleted);
            } else if let Some(path) = line.strip_prefix("rename from ") {
                current.old_path = Some(unquote_path(path));
                current.status = Some(DiffViewerFileStatus::Renamed);
            } else if let Some(path) = line.strip_prefix("rename to ") {
                current.new_path = Some(unquote_path(path));
                current.status = Some(DiffViewerFileStatus::Renamed);
            } else if let Some(path) = line.strip_prefix("copy from ") {
                // A copy reads as an edit from the source path to the new one.
                current.old_path = Some(unquote_path(path));
            } else if let Some(path) = line.strip_prefix("copy to ") {
                current.new_path = Some(unquote_path(path));
            } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
                current.is_binary = true;
            }
        }
        i += 1;
    }
    flush(&mut file, &mut hunk, &mut files);
    files
}
