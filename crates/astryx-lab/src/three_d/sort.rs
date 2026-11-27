//! `Array.prototype.sort` with a comparator, as V8 runs it, so that the marks' depth order matches
//! upstream's even where the comparator is not a strict order (a NaN depth, which the spec's
//! `SortCompare` turns into "equal to everything").
//!
//! The algorithm is V8's TimSort (`third_party/v8/builtins/array-sort.tq`, Copyright 2018 the
//! V8 project authors, BSD-3-Clause, see `THIRD_PARTY_NOTICES.md`; itself CPython's
//! `listsort`): natural runs, binary
//! insertion up to the minimum run length, the pending-run stack rules, and merges that first
//! trim each run by galloping and then gallop whenever one run keeps winning.

use std::cmp::max;

/// V8's `kMinGallopWins`: the wins in a row that switch a merge to galloping.
const MIN_GALLOP_WINS: usize = 7;

/// Sorts `items` as V8's `items.sort(compare)` would: stable, with a NaN order counted as +0.
pub(crate) fn js_sort<T: Clone>(items: &mut [T], compare: impl Fn(&T, &T) -> f64) {
    let n = items.len();
    if n < 2 {
        return;
    }
    let order = |a: usize, b: usize| {
        let v = compare(&items[a], &items[b]);
        if v.is_nan() { 0. } else { v }
    };
    let mut sort = TimSort {
        work: (0..n).collect(),
        runs: Vec::new(),
        min_gallop: MIN_GALLOP_WINS,
        order: &order,
    };
    sort.run();
    let sorted: Vec<T> = sort.work.iter().map(|&i| items[i].clone()).collect();
    items.clone_from_slice(&sorted);
}

/// V8's `SortState`: the work array (indices into the items), the pending runs and the
/// galloping threshold, which adapts across merges.
struct TimSort<'a> {
    work: Vec<usize>,
    runs: Vec<(usize, usize)>,
    min_gallop: usize,
    order: &'a dyn Fn(usize, usize) -> f64,
}

/// Where a merge ends (V8's `Succeed`, `CopyA` and `CopyB` labels).
enum MergeEnd {
    Succeed,
    CopyRest,
}

fn ix(i: isize) -> usize {
    i as usize
}

impl TimSort<'_> {
    fn less(&self, a: usize, b: usize) -> bool {
        (self.order)(a, b) < 0.
    }

    /// V8's `ArrayTimSortImpl`.
    fn run(&mut self) {
        let n = self.work.len();
        let min_run = {
            let (mut m, mut r) = (n, 0);
            while m >= 64 {
                r |= m & 1;
                m >>= 1;
            }
            m + r
        };
        let (mut low, mut remaining) = (0, n);
        while remaining != 0 {
            let mut run = self.count_and_make_run(low, low + remaining);
            if run < min_run {
                let forced = min_run.min(remaining);
                self.binary_insertion_sort(low, low + run, low + forced);
                run = forced;
            }
            self.runs.push((low, run));
            self.merge_collapse();
            low += run;
            remaining -= run;
        }
        while self.runs.len() > 1 {
            let mut k = self.runs.len() - 2;
            if k > 0 && self.runs[k - 1].1 < self.runs[k + 1].1 {
                k -= 1;
            }
            self.merge_at(k);
        }
    }

    /// V8's `CountAndMakeRun`: the run starting at `low_arg`, reversed in place when descending.
    fn count_and_make_run(&mut self, low_arg: usize, high: usize) -> usize {
        let low = low_arg + 1;
        if low == high {
            return 1;
        }
        let mut run = 2;
        let descending = self.less(self.work[low], self.work[low - 1]);
        let mut previous = self.work[low];
        for idx in low + 1..high {
            let current = self.work[idx];
            let order = (self.order)(current, previous);
            if descending {
                if order >= 0. {
                    break;
                }
            } else if order < 0. {
                break;
            }
            previous = current;
            run += 1;
        }
        if descending {
            self.work[low_arg..low_arg + run].reverse();
        }
        run
    }

    /// V8's `BinaryInsertionSort`: extends the sorted `low..start_arg` to `low..high`.
    fn binary_insertion_sort(&mut self, low: usize, start_arg: usize, high: usize) {
        let mut start = if low == start_arg {
            start_arg + 1
        } else {
            start_arg
        };
        while start < high {
            let (mut left, mut right) = (low, start);
            let pivot = self.work[start];
            while left < right {
                let mid = left + ((right - left) >> 1);
                if self.less(pivot, self.work[mid]) {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            self.work.copy_within(left..start, left + 1);
            self.work[left] = pivot;
            start += 1;
        }
    }

    /// V8's `MergeCollapse`: merges until the pending runs' lengths satisfy TimSort's invariants.
    fn merge_collapse(&mut self) {
        let established =
            |runs: &[(usize, usize)], n: usize| n < 2 || runs[n - 2].1 > runs[n - 1].1 + runs[n].1;
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if !established(&self.runs, n + 1) || !established(&self.runs, n) {
                if self.runs[n - 1].1 < self.runs[n + 1].1 {
                    n -= 1;
                }
                self.merge_at(n);
            } else if self.runs[n].1 <= self.runs[n + 1].1 {
                self.merge_at(n);
            } else {
                break;
            }
        }
    }

    /// V8's `MergeAt`: merges pending runs `i` and `i + 1`, first skipping what is already in place.
    fn merge_at(&mut self, i: usize) {
        let (mut base_a, mut len_a) = self.runs[i];
        let (base_b, mut len_b) = self.runs[i + 1];
        self.runs[i].1 = len_a + len_b;
        self.runs.remove(i + 1);
        let k = gallop_right(self.order, self.work[base_b], &self.work, base_a, len_a, 0);
        base_a += k;
        len_a -= k;
        if len_a == 0 {
            return;
        }
        len_b = gallop_left(
            self.order,
            self.work[base_a + len_a - 1],
            &self.work,
            base_b,
            len_b,
            len_b - 1,
        );
        if len_b == 0 {
            return;
        }
        if len_a <= len_b {
            self.merge_low(base_a, len_a, base_b, len_b);
        } else {
            self.merge_high(base_a, len_a, base_b, len_b);
        }
    }

    /// V8's `MergeLow`: merges forward, run A copied aside.
    fn merge_low(&mut self, base_a: usize, mut len_a: usize, base_b: usize, mut len_b: usize) {
        let temp: Vec<usize> = self.work[base_a..base_a + len_a].to_vec();
        let (mut dest, mut ct, mut cb) = (base_a, 0, base_b);
        self.work[dest] = self.work[cb];
        dest += 1;
        cb += 1;
        let end = 'merge: {
            len_b -= 1;
            if len_b == 0 {
                break 'merge MergeEnd::Succeed;
            }
            if len_a == 1 {
                break 'merge MergeEnd::CopyRest;
            }
            let mut min_gallop = self.min_gallop;
            loop {
                let (mut wins_a, mut wins_b) = (0, 0);
                loop {
                    if self.less(self.work[cb], temp[ct]) {
                        self.work[dest] = self.work[cb];
                        dest += 1;
                        cb += 1;
                        wins_b += 1;
                        len_b -= 1;
                        wins_a = 0;
                        if len_b == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    } else {
                        self.work[dest] = temp[ct];
                        dest += 1;
                        ct += 1;
                        wins_a += 1;
                        len_a -= 1;
                        wins_b = 0;
                        if len_a == 1 {
                            break 'merge MergeEnd::CopyRest;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    }
                }
                min_gallop += 1;
                let mut first = true;
                while wins_a >= MIN_GALLOP_WINS || wins_b >= MIN_GALLOP_WINS || first {
                    first = false;
                    min_gallop = max(1, min_gallop - 1);
                    self.min_gallop = min_gallop;
                    wins_a = gallop_right(self.order, self.work[cb], &temp, ct, len_a, 0);
                    if wins_a > 0 {
                        self.work[dest..dest + wins_a].copy_from_slice(&temp[ct..ct + wins_a]);
                        dest += wins_a;
                        ct += wins_a;
                        len_a -= wins_a;
                        if len_a == 1 {
                            break 'merge MergeEnd::CopyRest;
                        }
                        if len_a == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                    }
                    self.work[dest] = self.work[cb];
                    dest += 1;
                    cb += 1;
                    len_b -= 1;
                    if len_b == 0 {
                        break 'merge MergeEnd::Succeed;
                    }
                    wins_b = gallop_left(self.order, temp[ct], &self.work, cb, len_b, 0);
                    if wins_b > 0 {
                        self.work.copy_within(cb..cb + wins_b, dest);
                        dest += wins_b;
                        cb += wins_b;
                        len_b -= wins_b;
                        if len_b == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                    }
                    self.work[dest] = temp[ct];
                    dest += 1;
                    ct += 1;
                    len_a -= 1;
                    if len_a == 1 {
                        break 'merge MergeEnd::CopyRest;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };
        match end {
            MergeEnd::Succeed => {
                if len_a > 0 {
                    self.work[dest..dest + len_a].copy_from_slice(&temp[ct..ct + len_a]);
                }
            }
            // The last of run A belongs after the rest of run B.
            MergeEnd::CopyRest => {
                self.work.copy_within(cb..cb + len_b, dest);
                self.work[dest + len_b] = temp[ct];
            }
        }
    }

    /// V8's `MergeHigh`: merges backward, run B copied aside.
    fn merge_high(&mut self, base_a: usize, mut len_a: usize, base_b: usize, mut len_b: usize) {
        let temp: Vec<usize> = self.work[base_b..base_b + len_b].to_vec();
        let mut dest = (base_b + len_b - 1) as isize;
        let mut ct = len_b as isize - 1;
        let mut ca = (base_a + len_a - 1) as isize;
        self.work[ix(dest)] = self.work[ix(ca)];
        dest -= 1;
        ca -= 1;
        let end = 'merge: {
            len_a -= 1;
            if len_a == 0 {
                break 'merge MergeEnd::Succeed;
            }
            if len_b == 1 {
                break 'merge MergeEnd::CopyRest;
            }
            let mut min_gallop = self.min_gallop;
            loop {
                let (mut wins_a, mut wins_b) = (0, 0);
                loop {
                    if self.less(temp[ix(ct)], self.work[ix(ca)]) {
                        self.work[ix(dest)] = self.work[ix(ca)];
                        dest -= 1;
                        ca -= 1;
                        wins_a += 1;
                        len_a -= 1;
                        wins_b = 0;
                        if len_a == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    } else {
                        self.work[ix(dest)] = temp[ix(ct)];
                        dest -= 1;
                        ct -= 1;
                        wins_b += 1;
                        len_b -= 1;
                        wins_a = 0;
                        if len_b == 1 {
                            break 'merge MergeEnd::CopyRest;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    }
                }
                min_gallop += 1;
                let mut first = true;
                while wins_a >= MIN_GALLOP_WINS || wins_b >= MIN_GALLOP_WINS || first {
                    first = false;
                    min_gallop = max(1, min_gallop - 1);
                    self.min_gallop = min_gallop;
                    let k = gallop_right(
                        self.order,
                        temp[ix(ct)],
                        &self.work,
                        base_a,
                        len_a,
                        len_a - 1,
                    );
                    wins_a = len_a - k;
                    if wins_a > 0 {
                        dest -= wins_a as isize;
                        ca -= wins_a as isize;
                        self.work
                            .copy_within(ix(ca + 1)..ix(ca + 1) + wins_a, ix(dest + 1));
                        len_a -= wins_a;
                        if len_a == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                    }
                    self.work[ix(dest)] = temp[ix(ct)];
                    dest -= 1;
                    ct -= 1;
                    len_b -= 1;
                    if len_b == 1 {
                        break 'merge MergeEnd::CopyRest;
                    }
                    let k = gallop_left(self.order, self.work[ix(ca)], &temp, 0, len_b, len_b - 1);
                    wins_b = len_b - k;
                    if wins_b > 0 {
                        dest -= wins_b as isize;
                        ct -= wins_b as isize;
                        self.work[ix(dest + 1)..ix(dest + 1) + wins_b]
                            .copy_from_slice(&temp[ix(ct + 1)..ix(ct + 1) + wins_b]);
                        len_b -= wins_b;
                        if len_b == 1 {
                            break 'merge MergeEnd::CopyRest;
                        }
                        if len_b == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                    }
                    self.work[ix(dest)] = self.work[ix(ca)];
                    dest -= 1;
                    ca -= 1;
                    len_a -= 1;
                    if len_a == 0 {
                        break 'merge MergeEnd::Succeed;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };
        match end {
            MergeEnd::Succeed => {
                if len_b > 0 {
                    let at = ix(dest - (len_b as isize - 1));
                    self.work[at..at + len_b].copy_from_slice(&temp[..len_b]);
                }
            }
            // The first of run B belongs before the rest of run A.
            MergeEnd::CopyRest => {
                dest -= len_a as isize;
                ca -= len_a as isize;
                self.work
                    .copy_within(ix(ca + 1)..ix(ca + 1) + len_a, ix(dest + 1));
                self.work[ix(dest)] = temp[ix(ct)];
            }
        }
    }
}

/// V8's `GallopLeft`: where `key` goes in the sorted `run[base..base + length]`, left of any equal
/// elements, searching outward from `hint`.
fn gallop_left(
    order: &dyn Fn(usize, usize) -> f64,
    key: usize,
    run: &[usize],
    base: usize,
    length: usize,
    hint: usize,
) -> usize {
    let less = |a: usize, b: usize| order(a, b) < 0.;
    let (hint, length) = (hint as isize, length as isize);
    let at = |i: isize| run[ix(base as isize + i)];
    let (mut last, mut offset): (isize, isize) = (0, 1);
    if less(at(hint), key) {
        let max_offset = length - hint;
        while offset < max_offset {
            if !less(at(hint + offset), key) {
                break;
            }
            last = offset;
            offset = (offset << 1) + 1;
        }
        offset = offset.min(max_offset);
        last += hint;
        offset += hint;
    } else {
        let max_offset = hint + 1;
        while offset < max_offset {
            if less(at(hint - offset), key) {
                break;
            }
            last = offset;
            offset = (offset << 1) + 1;
        }
        offset = offset.min(max_offset);
        let tmp = last;
        last = hint - offset;
        offset = hint - tmp;
    }
    last += 1;
    while last < offset {
        let m = last + ((offset - last) >> 1);
        if less(at(m), key) {
            last = m + 1;
        } else {
            offset = m;
        }
    }
    offset as usize
}

/// V8's `GallopRight`: where `key` goes in the sorted `run[base..base + length]`, right of any equal
/// elements, searching outward from `hint`.
fn gallop_right(
    order: &dyn Fn(usize, usize) -> f64,
    key: usize,
    run: &[usize],
    base: usize,
    length: usize,
    hint: usize,
) -> usize {
    let less = |a: usize, b: usize| order(a, b) < 0.;
    let (hint, length) = (hint as isize, length as isize);
    let at = |i: isize| run[ix(base as isize + i)];
    let (mut last, mut offset): (isize, isize) = (0, 1);
    if less(key, at(hint)) {
        let max_offset = hint + 1;
        while offset < max_offset {
            if !less(key, at(hint - offset)) {
                break;
            }
            last = offset;
            offset = (offset << 1) + 1;
        }
        offset = offset.min(max_offset);
        let tmp = last;
        last = hint - offset;
        offset = hint - tmp;
    } else {
        let max_offset = length - hint;
        while offset < max_offset {
            if less(key, at(hint + offset)) {
                break;
            }
            last = offset;
            offset = (offset << 1) + 1;
        }
        offset = offset.min(max_offset);
        last += hint;
        offset += hint;
    }
    last += 1;
    while last < offset {
        let m = last + ((offset - last) >> 1);
        if less(key, at(m)) {
            offset = m;
        } else {
            last = m + 1;
        }
    }
    offset as usize
}
