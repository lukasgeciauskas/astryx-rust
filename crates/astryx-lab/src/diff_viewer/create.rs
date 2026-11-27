//! Upstream lab `DiffViewer/createDiffFile.ts`: a line diff (Myers) of two versions of a
//! text, for callers that hold contents rather than a patch.

use std::collections::HashMap;

use gpui::SharedString;

use super::types::{
    DiffViewerFile, DiffViewerFileStatus, DiffViewerHunk, DiffViewerLine, DiffViewerLineKind,
};

/// Upstream `CreateDiffFileOptions`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateDiffFileOptions {
    /// The original text.
    pub old_value: String,
    /// The changed text.
    pub new_value: String,
    /// The file's path on both sides; `old_path` and `new_path` take precedence.
    pub path: Option<String>,
    /// The path before the change: `Some(None)` marks the file added (upstream `null`).
    pub old_path: Option<Option<String>>,
    /// The path after the change: `Some(None)` marks the file deleted.
    pub new_path: Option<Option<String>>,
    /// Unchanged lines kept around each change (3 when unset or NaN); changes closer
    /// than twice this share one hunk.
    pub context_lines: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Context(usize, usize),
    Removed(usize),
    Added(usize),
}

fn split_lines(value: &str) -> Vec<&str> {
    if value.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = value
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

/// Upstream `MAX_EDIT_DISTANCE`.
const MAX_EDIT_DISTANCE: f64 = 10_000.;
/// Upstream `MAX_SEARCH_WORK`.
const MAX_SEARCH_WORK: f64 = 1_000_000_000.;
/// Upstream `TRACED_SEARCH_LIMIT`.
const TRACED_SEARCH_LIMIT: f64 = 1000.;

/// Upstream `diffLines`: shared leading and trailing lines are context; the middle is
/// a shortest edit script, or, past the edit limit, the old middle removed and the new
/// one added (a replaced block).
fn diff_lines(a: &[&str], b: &[&str]) -> (Vec<Op>, bool) {
    let mut head = 0;
    while head < a.len() && head < b.len() && a[head] == b[head] {
        head += 1;
    }
    let (mut tail_a, mut tail_b) = (a.len(), b.len());
    while tail_a > head && tail_b > head && a[tail_a - 1] == b[tail_b - 1] {
        tail_a -= 1;
        tail_b -= 1;
    }
    let mut ops: Vec<Op> = (0..head).map(|i| Op::Context(i, i)).collect();
    let one_sided = tail_a == head || tail_b == head;
    let edit_limit =
        MAX_EDIT_DISTANCE.min((MAX_SEARCH_WORK / (tail_a - head + (tail_b - head)) as f64).floor());
    let middle = if one_sided {
        None
    } else {
        traced_edit_script(
            a,
            b,
            head,
            tail_a,
            tail_b,
            TRACED_SEARCH_LIMIT.min(edit_limit),
        )
        .or_else(|| shortest_edit_script(a, b, head, tail_a, tail_b, edit_limit))
    };
    let replaced = middle.is_none() && !one_sided;
    match middle {
        Some(middle) => ops.extend(middle),
        None => {
            ops.extend((head..tail_a).map(Op::Removed));
            ops.extend((head..tail_b).map(Op::Added));
        }
    }
    let mut i = 0;
    while tail_a + i < a.len() {
        ops.push(Op::Context(tail_a + i, tail_b + i));
        i += 1;
    }
    (ops, replaced)
}

/// Upstream `shortestEditScript`: Myers' O(ND) search in linear space over
/// `a[start..end_a]` and `b[start..end_b]`; `None` past `max_edits`.
fn shortest_edit_script<'t>(
    a: &[&'t str],
    b: &[&'t str],
    start: usize,
    end_a: usize,
    end_b: usize,
    max_edits: f64,
) -> Option<Vec<Op>> {
    let mut ids: HashMap<&'t str, i32> = HashMap::new();
    let mut to_ids = |lines: &[&'t str], end: usize| -> Vec<i32> {
        (start..end)
            .map(|i| {
                let next = ids.len() as i32;
                *ids.entry(lines[i]).or_insert(next)
            })
            .collect()
    };
    let old_ids = to_ids(a, end_a);
    let new_ids = to_ids(b, end_b);
    // Each copy of a line one side has more often than the other is an edit: a lower
    // bound, past the limit for a rewrite.
    let mut surplus = vec![0i64; ids.len()];
    for id in &old_ids {
        surplus[*id as usize] += 1;
    }
    for id in &new_ids {
        surplus[*id as usize] -= 1;
    }
    let min_edits: i64 = surplus.iter().map(|c| c.abs()).sum();
    if min_edits as f64 > max_edits {
        return None;
    }
    let offset = (old_ids.len() + new_ids.len() + 1) as isize;
    let size = (2 * offset + 1) as usize;
    let mut forward = vec![0isize; size];
    let mut backward = vec![0isize; size];
    let mut ops: Vec<Op> = Vec::new();

    struct Search<'s> {
        old: &'s [i32],
        new: &'s [i32],
        forward: &'s mut Vec<isize>,
        backward: &'s mut Vec<isize>,
        offset: isize,
        start: usize,
        ops: &'s mut Vec<Op>,
    }

    impl Search<'_> {
        fn f(&self, k: isize) -> isize {
            self.forward[(self.offset + k) as usize]
        }
        fn b(&self, k: isize) -> isize {
            self.backward[(self.offset + k) as usize]
        }

        fn meeting_point(
            &mut self,
            a_lo: isize,
            a_hi: isize,
            b_lo: isize,
            b_hi: isize,
            limit: f64,
        ) -> Option<(isize, isize)> {
            let n = a_hi - a_lo;
            let m = b_hi - b_lo;
            let delta = n - m;
            let odd = delta & 1 != 0;
            self.forward[(self.offset + 1) as usize] = 0;
            self.backward[(self.offset + 1) as usize] = 0;
            let steps = ((n + m) as f64 / 2.).ceil() as isize;
            for d in 0..=steps {
                if (2 * d - 1) as f64 > limit {
                    return None;
                }
                let mut k = -d;
                while k <= d {
                    let mut x = if k == -d || (k != d && self.f(k - 1) < self.f(k + 1)) {
                        self.f(k + 1)
                    } else {
                        self.f(k - 1) + 1
                    };
                    let mut y = x - k;
                    while x < n
                        && y < m
                        && self.old[(a_lo + x) as usize] == self.new[(b_lo + y) as usize]
                    {
                        x += 1;
                        y += 1;
                    }
                    self.forward[(self.offset + k) as usize] = x;
                    let back = delta - k;
                    if odd && x <= n && y <= m && back > -d && back < d && x + self.b(back) >= n {
                        return Some((a_lo + x, b_lo + y));
                    }
                    k += 2;
                }
                if (2 * d) as f64 > limit {
                    return None;
                }
                let mut k = -d;
                while k <= d {
                    let mut x = if k == -d || (k != d && self.b(k - 1) < self.b(k + 1)) {
                        self.b(k + 1)
                    } else {
                        self.b(k - 1) + 1
                    };
                    let mut y = x - k;
                    while x < n
                        && y < m
                        && self.old[(a_hi - 1 - x) as usize] == self.new[(b_hi - 1 - y) as usize]
                    {
                        x += 1;
                        y += 1;
                    }
                    self.backward[(self.offset + k) as usize] = x;
                    let front = delta - k;
                    if !odd
                        && x <= n
                        && y <= m
                        && front >= -d
                        && front <= d
                        && x + self.f(front) >= n
                    {
                        let meet_x = self.f(front);
                        return Some((a_lo + meet_x, b_lo + meet_x - front));
                    }
                    k += 2;
                }
            }
            None
        }

        fn solve(
            &mut self,
            mut a_lo: isize,
            a_hi: isize,
            mut b_lo: isize,
            b_hi: isize,
            limit: f64,
        ) -> bool {
            while a_lo < a_hi && b_lo < b_hi && self.old[a_lo as usize] == self.new[b_lo as usize] {
                self.ops.push(Op::Context(
                    self.start + a_lo as usize,
                    self.start + b_lo as usize,
                ));
                a_lo += 1;
                b_lo += 1;
            }
            let mut tail = 0;
            while a_hi - tail > a_lo
                && b_hi - tail > b_lo
                && self.old[(a_hi - 1 - tail) as usize] == self.new[(b_hi - 1 - tail) as usize]
            {
                tail += 1;
            }
            let a_end = a_hi - tail;
            let b_end = b_hi - tail;
            if a_lo == a_end || b_lo == b_end {
                if ((a_end - a_lo) + (b_end - b_lo)) as f64 > limit {
                    return false;
                }
                for i in a_lo..a_end {
                    self.ops.push(Op::Removed(self.start + i as usize));
                }
                for j in b_lo..b_end {
                    self.ops.push(Op::Added(self.start + j as usize));
                }
            } else {
                let Some((px, py)) = self.meeting_point(a_lo, a_end, b_lo, b_end, limit) else {
                    return false;
                };
                if !self.solve(a_lo, px, b_lo, py, f64::INFINITY)
                    || !self.solve(px, a_end, py, b_end, f64::INFINITY)
                {
                    return false;
                }
            }
            let mut t = tail;
            while t > 0 {
                self.ops.push(Op::Context(
                    self.start + (a_hi - t) as usize,
                    self.start + (b_hi - t) as usize,
                ));
                t -= 1;
            }
            true
        }
    }

    let (n, m) = (old_ids.len() as isize, new_ids.len() as isize);
    let mut search = Search {
        old: &old_ids,
        new: &new_ids,
        forward: &mut forward,
        backward: &mut backward,
        offset,
        start,
        ops: &mut ops,
    };
    if !search.solve(0, n, 0, m, max_edits) {
        return None;
    }
    Some(removals_first(ops))
}

/// Upstream `removalsFirst`: each run's removals before its additions, so split view
/// pairs each removed line with the added line that replaced it.
fn removals_first(ops: Vec<Op>) -> Vec<Op> {
    let mut sorted = Vec::with_capacity(ops.len());
    let mut added = Vec::new();
    for op in ops {
        match op {
            Op::Added(_) => {
                added.push(op);
                continue;
            }
            Op::Context(..) => sorted.append(&mut added),
            Op::Removed(_) => {}
        }
        sorted.push(op);
    }
    sorted.append(&mut added);
    sorted
}

/// Upstream `tracedEditScript`: Myers' O(ND) search keeping every step's diagonals to
/// trace the path back; `None` past `max_edits`.
fn traced_edit_script(
    a: &[&str],
    b: &[&str],
    start: usize,
    end_a: usize,
    end_b: usize,
    max_edits: f64,
) -> Option<Vec<Op>> {
    let n = (end_a - start) as isize;
    let m = (end_b - start) as isize;
    let max = n + m;
    let offset = max + 1;
    let mut v = vec![0isize; (2 * max + 3) as usize];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let at = |v: &[isize], k: isize| v[(offset + k) as usize];
    'outer: for d in 0..=max {
        if d as f64 > max_edits {
            return None;
        }
        trace.push(v[(offset - d) as usize..(offset + d + 1) as usize].to_vec());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && at(&v, k - 1) < at(&v, k + 1)) {
                at(&v, k + 1)
            } else {
                at(&v, k - 1) + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[start + x as usize] == b[start + y as usize] {
                x += 1;
                y += 1;
            }
            v[(offset + k) as usize] = x;
            if x >= n && y >= m {
                break 'outer;
            }
            k += 2;
        }
    }
    let mut ops = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..trace.len() as isize).rev() {
        let vd = &trace[d as usize];
        // Outside the stored window (only k = 1 at d = 0) v was still zero.
        let at = |k: isize| -> isize {
            let i = k + d;
            if i < 0 {
                0
            } else {
                vd.get(i as usize).copied().unwrap_or(0)
            }
        };
        let k = x - y;
        let prev_k = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = at(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            ops.push(Op::Context(start + x as usize, start + y as usize));
        }
        if d > 0 {
            if x == prev_x {
                ops.push(Op::Added(start + prev_y as usize));
            } else {
                ops.push(Op::Removed(start + prev_x as usize));
            }
        }
        x = prev_x;
        y = prev_y;
    }
    ops.reverse();
    Some(ops)
}

fn format_range(start: u64, count: u64) -> String {
    if count == 1 {
        start.to_string()
    } else {
        format!("{start},{count}")
    }
}

/// Upstream `createDiffFile`: a [`DiffViewerFile`] from two versions of a text.
pub fn create_diff_file(options: &CreateDiffFileOptions) -> DiffViewerFile {
    let old_path = match &options.old_path {
        Some(p) => p.clone(),
        None => options.path.clone(),
    };
    let new_path = match &options.new_path {
        Some(p) => p.clone(),
        None => options.path.clone(),
    };
    let context = match options.context_lines {
        Some(c) if c.is_nan() => 3.,
        Some(c) => c.floor().max(0.),
        None => 3.,
    };
    let mut status = DiffViewerFileStatus::Modified;
    if options.old_path == Some(None) {
        status = DiffViewerFileStatus::Added;
    } else if options.new_path == Some(None) {
        status = DiffViewerFileStatus::Deleted;
    } else if let (Some(o), Some(n)) = (&old_path, &new_path)
        && o != n
    {
        status = DiffViewerFileStatus::Renamed;
    }
    let a = split_lines(&options.old_value);
    let b = split_lines(&options.new_value);
    let (ops, replaced) = diff_lines(&a, &b);
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Context(..)))
        .map(|(i, _)| i)
        .collect();
    let mut hunks = Vec::new();
    let mut g = 0;
    while g < changed.len() {
        let mut last = changed[g];
        let mut h = g + 1;
        while h < changed.len() && (changed[h] - last - 1) as f64 <= context * 2. {
            last = changed[h];
            h += 1;
        }
        let start = (changed[g] as f64 - context).max(0.) as usize;
        let end = ((ops.len() - 1) as f64).min(last as f64 + context) as usize;
        let mut lines = Vec::new();
        let (mut old_count, mut new_count) = (0u64, 0u64);
        for op in &ops[start..=end] {
            match *op {
                Op::Context(ai, bi) => {
                    lines.push(DiffViewerLine {
                        kind: DiffViewerLineKind::Context,
                        content: SharedString::from(a[ai].to_string()),
                        old_line_number: Some(ai as u64 + 1),
                        new_line_number: Some(bi as u64 + 1),
                    });
                    old_count += 1;
                    new_count += 1;
                }
                Op::Removed(ai) => {
                    lines.push(DiffViewerLine {
                        kind: DiffViewerLineKind::Removed,
                        content: SharedString::from(a[ai].to_string()),
                        old_line_number: Some(ai as u64 + 1),
                        new_line_number: None,
                    });
                    old_count += 1;
                }
                Op::Added(bi) => {
                    lines.push(DiffViewerLine {
                        kind: DiffViewerLineKind::Added,
                        content: SharedString::from(b[bi].to_string()),
                        old_line_number: None,
                        new_line_number: Some(bi as u64 + 1),
                    });
                    new_count += 1;
                }
            }
        }
        // Lines of each side before this hunk.
        let old_before = ops[..start]
            .iter()
            .filter(|op| !matches!(op, Op::Added(_)))
            .count() as u64;
        let new_before = ops[..start]
            .iter()
            .filter(|op| !matches!(op, Op::Removed(_)))
            .count() as u64;
        // An empty side's start is the line before the hunk.
        let old_start = if old_count == 0 {
            old_before
        } else {
            old_before + 1
        };
        let new_start = if new_count == 0 {
            new_before
        } else {
            new_before + 1
        };
        hunks.push(DiffViewerHunk {
            header: format!(
                "@@ -{} +{} @@",
                format_range(old_start, old_count),
                format_range(new_start, new_count)
            )
            .into(),
            old_start,
            old_lines: old_count,
            new_start,
            new_lines: new_count,
            lines,
            // A replaced block is the only change, so it fills the only hunk.
            is_replaced_block: replaced,
        });
        g = h;
    }
    DiffViewerFile {
        old_path: if status == DiffViewerFileStatus::Added {
            None
        } else {
            old_path.map(Into::into)
        },
        new_path: if status == DiffViewerFileStatus::Deleted {
            None
        } else {
            new_path.map(Into::into)
        },
        status,
        is_binary: false,
        hunks,
    }
}
