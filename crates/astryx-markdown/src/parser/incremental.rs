//! Incremental parsing, upstream `Markdown/parser.ts` ("Incremental parsing"): a streamed
//! document's settled prefix (what lies before the last blank line no open fence or
//! display expression holds) is parsed once, and only the tail after it on each call.
//!
//! A snapshot that no longer starts with the settled prefix (a replacement rather than
//! an append) discards the cache, and so does a change of any option that changes the
//! parsed nodes (FR15). Link reference definitions are document-global: when the set the
//! settled blocks were parsed with changes, they are parsed again, so a reference that
//! precedes its footer definition resolves (FR11).

use std::collections::HashMap;
use std::rc::Rc;

use super::block::parse_markdown_impl;
use super::link_definitions::{FenceEvent, TopLevelFences, extract_link_definitions};
use super::streaming::{
    DisplayMathContainer, DisplayMathLineState, display_math_container, display_math_line_state,
    trim_open_display_math, trim_unsettled_structural,
};
use super::{LinkDefinitions, ResolvedOptions};
use crate::ParseOptions;
use crate::ast::{Block, List, Position};
use crate::js;
use crate::legacy::BlockNode;

/// Upstream `IncrementalWork` (`getIncrementalParseWork`): deterministic counters of what
/// the most recent incremental parse visited, for performance regression tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IncrementalWork {
    /// Bytes copied into the tail's line array.
    pub split_characters: usize,
    /// Tail lines visited by fence and blank-boundary detection.
    pub boundary_lines: usize,
    /// Bytes visited while collecting document-global definitions.
    pub definition_characters: usize,
    /// Blocks parsed anew this call (the settled delta and the unsettled tail).
    pub rendered_blocks: usize,
}

/// Upstream `IncrementalCache`: what a state keeps between calls, besides the fields
/// upstream exposes on `IncrementalState`.
#[derive(Default)]
struct IncrementalCache {
    /// The byte offset just past the immutable settled prefix.
    settled_end: usize,
    /// Line breaks before `settled_end`, kept only while source lines are recorded;
    /// `None` when unknown (counted once from the settled text).
    settled_end_line: Option<usize>,
    /// Definitions whose whole block is in the settled prefix.
    settled_link_defs: LinkDefinitions,
    /// Definitions still in the mutable tail on the preceding call.
    tail_link_defs: LinkDefinitions,
    /// The effective document-global definitions the slices are parsed with.
    link_defs: Rc<LinkDefinitions>,
    link_defs_key: String,
    /// The canonical settled blocks.
    settled_ast_blocks: Vec<Block>,
    settled_revision: u64,
    /// The settled revision (and range flag) `settled_blocks` was projected from.
    projected: Option<(u64, bool)>,
    work: IncrementalWork,
}

/// Upstream `IncrementalState` (`createIncrementalState`): one streamed document's parser
/// cache. Create one per document with [`crate::create_incremental_state`] and hand it to
/// every [`crate::parse_markdown_ast_incremental`] or [`crate::parse_markdown_incremental`]
/// call for that document's snapshots.
#[derive(Default)]
pub struct IncrementalState {
    prev_input: String,
    settled_text: String,
    settled_blocks: Vec<BlockNode>,
    settled_up_to: usize,
    /// FR15's identity of what the settled blocks were parsed with (upstream's separate
    /// `autolink`, `math`, `sourceRanges`, `sourceIdsKey` and `pluginSyntaxIdentity`
    /// fields, as one key). `None` before the first parse and after a reset, which
    /// upstream's unset keys mean.
    identity: Option<String>,
    link_defs_key: Option<String>,
    cache: IncrementalCache,
}

impl std::fmt::Debug for IncrementalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncrementalState")
            .field("prev_input", &self.prev_input)
            .field("settled_text", &self.settled_text)
            .field("settled_up_to", &self.settled_up_to)
            .field("settled_blocks", &self.cache.settled_ast_blocks.len())
            .finish_non_exhaustive()
    }
}

impl IncrementalState {
    /// Upstream `createIncrementalState`: an empty cache.
    pub fn new() -> Self {
        IncrementalState::default()
    }

    /// Upstream `prevInput`: the snapshot the last call parsed (empty after a reset).
    pub fn prev_input(&self) -> &str {
        &self.prev_input
    }

    /// Upstream `settledText`: the settled prefix of the document, parsed once.
    pub fn settled_text(&self) -> &str {
        &self.settled_text
    }

    /// Upstream `settledBlocks`: the settled prefix's blocks in the released shape, as the
    /// last [`crate::parse_markdown_incremental`] call projected them (empty after a
    /// reset). The canonical entry point leaves it alone, as upstream's does.
    pub fn settled_blocks(&self) -> &[BlockNode] {
        &self.settled_blocks
    }

    /// The settled prefix's canonical blocks (before plugin transforms).
    pub fn settled_ast_blocks(&self) -> &[Block] {
        &self.cache.settled_ast_blocks
    }

    /// Upstream `settledUpTo`: how many lines the settled prefix spans.
    pub fn settled_up_to(&self) -> usize {
        self.settled_up_to
    }

    /// Upstream `linkDefsKey`: the signature of the link reference definitions the
    /// settled blocks were parsed with.
    pub fn link_defs_key(&self) -> Option<&str> {
        self.link_defs_key.as_deref()
    }

    /// Upstream `getIncrementalParseWork`: what the most recent call visited.
    pub fn work(&self) -> IncrementalWork {
        self.cache.work
    }

    /// Refreshes `settled_blocks` from the canonical settled blocks (upstream's
    /// `projectedSettledBlocks`), when they changed since the last projection.
    pub(crate) fn project_settled_blocks(&mut self, with_ranges: bool) {
        let current = (self.cache.settled_revision, with_ranges);
        if self.cache.projected != Some(current) {
            self.settled_blocks = self
                .cache
                .settled_ast_blocks
                .iter()
                .map(|block| crate::legacy::project_block_node(block, with_ranges))
                .collect();
            self.cache.projected = Some(current);
        }
    }
}

/// Upstream `sourceIdsSignature`: an order-independent signature of a citation-source set.
fn source_ids_signature(options: &ParseOptions) -> String {
    options.source_ids.as_ref().map_or_else(String::new, |ids| {
        ids.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\u{0}")
    })
}

/// Upstream `linkDefsSignature`: an order-independent signature of a definition set.
fn link_defs_signature(defs: &LinkDefinitions) -> String {
    let mut entries: Vec<(&String, &String)> = defs.iter().collect();
    entries.sort();
    entries
        .iter()
        .map(|(label, destination)| format!("{label}\u{0}{destination}"))
        .collect::<Vec<_>>()
        .join("\u{1}")
}

/// The cache identity (FR15): every option that changes the parsed nodes, then each
/// syntax-bearing plugin's name, API version and parse key, in order. Finality is not part
/// of it, as upstream's is not: blocks settled while streaming stand in the final parse.
fn incremental_identity(options: &ParseOptions, opts: &ResolvedOptions) -> String {
    let plugins = opts.plugins.map_or("", |plugins| plugins.syntax_identity());
    format!(
        "autolink={}\u{1}math={}\u{1}sourceRanges={}\u{1}sourceIds={}\u{1}plugins={plugins}",
        options.autolink_gfm,
        options.math,
        options.source_ranges,
        source_ids_signature(options),
    )
}

/// What [`find_settled_boundary`] found.
struct SettledBoundary {
    /// The line index of the last blank line that settles what precedes it.
    boundary: Option<usize>,
    open_fence: bool,
    open_math: bool,
}

/// Upstream `findSettledBoundary`: the line index of the last blank line no fenced code
/// block or display expression holds, and whether a fence or a display expression is
/// still open at the end. The boundary never moves back as the document grows: a blank
/// last line is only the newline written so far, and an open fence or expression keeps
/// the boundary from before it opened, which nothing typed inside it can change.
fn find_settled_boundary(lines: &[&str], math: bool) -> SettledBoundary {
    let mut fences = TopLevelFences::default();
    let mut math_container: Option<DisplayMathContainer> = None;
    let mut suppress_math_until_boundary = false;
    let mut last_boundary = None;
    let mut boundary_before_fence = None;
    let mut boundary_before_math = None;

    for line_index in 0..lines.len() {
        let line = lines[line_index];
        if fences.is_open() {
            fences.read(lines, line_index);
            continue;
        }
        if let Some(container) = &math_container {
            match display_math_line_state(line, container) {
                DisplayMathLineState::Close => {
                    math_container = None;
                    continue;
                }
                DisplayMathLineState::Inside => continue,
                // The container ended before a closer arrived: the parser reads the
                // opener as text, so ordinary detection resumes on this line.
                DisplayMathLineState::Outside => {
                    math_container = None;
                    suppress_math_until_boundary = true;
                }
            }
        }
        // A same-line `$$…$$` never changes the state; a standalone marker opens display
        // math in its container (the top level, a block quote or a list item).
        if math
            && !suppress_math_until_boundary
            && let Some(container) = display_math_container(line)
        {
            math_container = Some(container);
            boundary_before_math = last_boundary;
            continue;
        }
        if fences.read(lines, line_index) == Some(FenceEvent::Open) {
            boundary_before_fence = last_boundary;
            continue;
        }
        if js::is_blank(line) {
            suppress_math_until_boundary = false;
            // A blank line settles what precedes it only when the next line starts at
            // the margin: an indented one may continue a list item across it.
            if line_index > 0
                && line_index < lines.len() - 1
                && lines[line_index + 1]
                    .chars()
                    .next()
                    .is_some_and(|c| !js::is_whitespace(c))
            {
                last_boundary = Some(line_index);
            }
        }
    }

    SettledBoundary {
        boundary: if fences.is_open() {
            boundary_before_fence
        } else if math_container.is_some() {
            boundary_before_math
        } else {
            last_boundary
        },
        open_fence: fences.is_open(),
        open_math: math_container.is_some(),
    }
}

/// Upstream `atOffset`: the same options for a slice starting at byte `offset` (and, while
/// lines are recorded, document line `line`), so its blocks address the whole document.
fn at_offset<'a>(opts: &ResolvedOptions<'a>, offset: usize, line: usize) -> ResolvedOptions<'a> {
    if !opts.ast_positions {
        return opts.clone();
    }
    ResolvedOptions {
        base_offset: offset,
        base_line: if opts.source_lines {
            line
        } else {
            opts.base_line
        },
        ..opts.clone()
    }
}

/// Upstream `countLineBreaks`: line breaks in `text[start..end]`.
fn count_line_breaks(text: &str, start: usize, end: usize) -> usize {
    text.as_bytes()[start..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Upstream `continuesList`: whether the full parse reads `next`, a blank line after
/// `previous`, as more of the same list: both ordered or both bulleted, with one
/// delimiter, and, for lists the block parser built, markers at one indentation with one
/// bullet. So `- a⏎⏎* b` and ` 1. a⏎⏎2. b` stay two lists.
fn continues_list(previous: &List, next: &List) -> bool {
    if previous.ordered != next.ordered || previous.delimiter != next.delimiter {
        return false;
    }
    match (previous.marker_shape.0, next.marker_shape.0) {
        (Some(previous), Some(next)) => previous == next,
        _ => true,
    }
}

/// The one loose list `previous` and `next` make, spanning both, with `previous`'s
/// marker shape (upstream `withMarkerShapeOf`).
fn joined_list(previous: &List, next: List) -> List {
    let position = match (previous.position, next.position) {
        (Some(start), Some(end)) => Some(Position {
            start: start.start,
            end: end.end,
        }),
        _ => None,
    };
    let mut children = previous.children.clone();
    children.extend(next.children);
    List {
        ordered: previous.ordered,
        start: previous.start,
        spread: Some(true),
        delimiter: previous.delimiter,
        children,
        position,
        data: None,
        marker_shape: previous.marker_shape,
    }
}

/// Splits a leading list off `delta` when it continues `previous`'s last list.
fn continuing_list(prev: &[Block], delta: &mut Vec<Block>) -> Option<List> {
    let (Some(Block::List(last)), Some(Block::List(first))) = (prev.last(), delta.first()) else {
        return None;
    };
    if !continues_list(last, first) {
        return None;
    }
    match delta.remove(0) {
        Block::List(first) => Some(joined_list(last, first)),
        _ => None,
    }
}

/// Upstream `mergeSettledBlocks`: the settled blocks then the tail's, an adjacent pair of
/// same-style lists joined into one loose list. Each blank-separated segment settles on
/// its own, so without this a streamed `1.⏎⏎1.⏎⏎1.` would be three lists, where the full
/// parse makes one (CommonMark §5.3).
fn merge_settled_blocks(prev: &[Block], mut delta: Vec<Block>) -> Vec<Block> {
    let joined = continuing_list(prev, &mut delta);
    let mut merged = Vec::with_capacity(prev.len() + delta.len());
    match joined {
        Some(list) => {
            merged.extend_from_slice(&prev[..prev.len() - 1]);
            merged.push(Block::List(list));
        }
        None => merged.extend_from_slice(prev),
    }
    merged.extend(delta);
    merged
}

/// Upstream `appendSettledBlocks`: appends a newly settled slice's blocks, joining a list
/// that continues the last one.
fn append_settled_blocks(prev: &mut Vec<Block>, mut delta: Vec<Block>) {
    if let Some(list) = continuing_list(prev, &mut delta) {
        let last = prev.len() - 1;
        prev[last] = Block::List(list);
    }
    prev.extend(delta);
}

/// Upstream `sameUnsettledDefinitions`: whether the tail's definitions that the settled
/// prefix does not define are unchanged.
fn same_unsettled_definitions(
    previous: &LinkDefinitions,
    next: &LinkDefinitions,
    settled: &LinkDefinitions,
) -> bool {
    let unchanged = |a: &LinkDefinitions, b: &LinkDefinitions| {
        a.iter().all(|(label, destination)| {
            settled.contains_key(label) || b.get(label) == Some(destination)
        })
    };
    unchanged(previous, next) && unchanged(next, previous)
}

/// Upstream `resetIncrementalCache`.
fn reset_incremental_cache(state: &mut IncrementalState) {
    state.prev_input.clear();
    state.settled_text.clear();
    state.settled_blocks.clear();
    state.settled_up_to = 0;
    state.link_defs_key = None;
    state.identity = None;
    let cache = &mut state.cache;
    cache.settled_end = 0;
    cache.settled_end_line = Some(0);
    cache.settled_link_defs.clear();
    cache.tail_link_defs = HashMap::new();
    cache.link_defs = Rc::default();
    cache.link_defs_key.clear();
    cache.settled_ast_blocks.clear();
    cache.settled_revision += 1;
    cache.projected = None;
    cache.work = IncrementalWork::default();
}

/// `text.replace(/^(?:[ \t]*\r?\n)+/, '')`: `text` without the blank lines that open it.
fn strip_leading_blank_lines(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut start = 0;
    loop {
        let mut at = start;
        while at < bytes.len() && matches!(bytes[at], b' ' | b'\t') {
            at += 1;
        }
        if bytes.get(at) == Some(&b'\r') {
            at += 1;
        }
        if bytes.get(at) != Some(&b'\n') {
            return &text[start..];
        }
        start = at + 1;
    }
}

/// `/\r(?:\n[\s]*)?$/.test(text)`: whether a CR ends `text`'s last content line.
fn ends_with_cr_line(text: &str) -> bool {
    let trailing = &text[js::trim_end(text).len()..];
    trailing.ends_with('\r') || trailing.contains("\r\n")
}

/// Upstream `parseMarkdownIncrementalAstBlocks`: the canonical blocks of one cumulative
/// snapshot, before plugin transforms.
pub(crate) fn parse_markdown_incremental_ast_blocks(
    input: &str,
    state: &mut IncrementalState,
    opts: &ResolvedOptions,
    options: &ParseOptions,
) -> Vec<Block> {
    let mut reparse_settled = false;
    // Blocks settled under other parse-affecting options would be reused unchanged.
    let identity = incremental_identity(options, opts);
    if state.identity.as_deref() != Some(identity.as_str()) {
        reparse_settled = true;
        state.identity = Some(identity.clone());
    }
    if input.is_empty() {
        reset_incremental_cache(state);
        return Vec::new();
    }
    // The settled prefix is reusable only while the input still holds it verbatim: a
    // replacement of the same length or longer disagrees with it without being shorter.
    if state.settled_text.len() != state.cache.settled_end
        || !input.starts_with(&state.settled_text)
    {
        reset_incremental_cache(state);
        state.identity = Some(identity);
    }

    // The recurring costs stay in the mutable suffix: splitting, fence and boundary
    // detection, definition collection, and block construction.
    let old_settled_end = state.cache.settled_end;
    let old_settled_line = if opts.source_lines {
        let settled_text = &state.settled_text;
        *state
            .cache
            .settled_end_line
            .get_or_insert_with(|| count_line_breaks(settled_text, 0, settled_text.len()))
    } else {
        0
    };
    let tail_raw = &input[old_settled_end..];
    let tail_lines: Vec<&str> = tail_raw.split('\n').collect();
    let SettledBoundary {
        boundary,
        open_fence,
        open_math,
    } = find_settled_boundary(&tail_lines, opts.math);
    // The lines before the boundary, joined: a prefix of the tail.
    let settled_delta = boundary.map_or("", |boundary| {
        let len = tail_lines[..boundary]
            .iter()
            .map(|line| line.len())
            .sum::<usize>()
            + boundary
            - 1;
        &tail_raw[..len]
    });
    let next_settled_end = old_settled_end + settled_delta.len();
    let unsettled_input = &input[next_settled_end..];

    // A definition is promoted only once its whole block is immutable; the tail's are
    // collected again, since the tail may change. A change to the effective set parses
    // the settled blocks again: a reference may precede its footer definition.
    let mut definitions_changed = false;
    let cache = &mut state.cache;
    if !settled_delta.is_empty() {
        for (label, destination) in extract_link_definitions(settled_delta, opts.math).defs {
            if !cache.settled_link_defs.contains_key(&label) {
                if cache.link_defs.get(&label) != Some(&destination) {
                    definitions_changed = true;
                }
                cache.settled_link_defs.insert(label, destination);
            }
        }
    }
    let tail_link_defs = extract_link_definitions(unsettled_input, opts.math).defs;
    if !same_unsettled_definitions(
        &cache.tail_link_defs,
        &tail_link_defs,
        &cache.settled_link_defs,
    ) {
        definitions_changed = true;
    }
    cache.tail_link_defs = tail_link_defs;
    if definitions_changed {
        // Settled (earlier) definitions win.
        let mut defs = cache.tail_link_defs.clone();
        defs.extend(
            cache
                .settled_link_defs
                .iter()
                .map(|(label, destination)| (label.clone(), destination.clone())),
        );
        cache.link_defs = Rc::new(defs);
        cache.link_defs_key = link_defs_signature(&cache.link_defs);
        reparse_settled = true;
    }
    state.link_defs_key = Some(cache.link_defs_key.clone());
    let parse_opts = if cache.link_defs.is_empty() {
        opts.clone()
    } else {
        ResolvedOptions {
            link_defs: Some(cache.link_defs.clone()),
            ..opts.clone()
        }
    };

    if !settled_delta.is_empty() {
        state.settled_text.push_str(settled_delta);
        state.settled_up_to += settled_delta.matches('\n').count();
        if old_settled_end == 0 {
            state.settled_up_to += 1;
        }
        cache.settled_end = next_settled_end;
        cache.settled_end_line = opts
            .source_lines
            .then(|| old_settled_line + count_line_breaks(settled_delta, 0, settled_delta.len()));
    }

    // Blank lines before the tail and whitespace after it carry nothing, but the first
    // line's indentation does: it decides a list's indent and whether a line is a
    // heading, as in a full parse of the same text.
    let trimmed_unsettled_input = js::trim_end(strip_leading_blank_lines(unsettled_input));
    // Trimming the end takes the CR of a CRLF snapshot's last content line too; that one
    // byte stays, so positions and delimiter content match a full parse.
    let unsettled_raw = if !trimmed_unsettled_input.is_empty() && ends_with_cr_line(unsettled_input)
    {
        format!("{trimmed_unsettled_input}\r")
    } else {
        trimmed_unsettled_input.to_string()
    };
    // Structural trimming holds back lines that look like an incomplete list or table,
    // which inside a fence are code; and an open fence's trailing blank lines are code,
    // as in a full parse of the same text.
    let unsettled_text = if open_fence {
        strip_leading_blank_lines(unsettled_input).to_string()
    } else if open_math {
        trim_open_display_math(&unsettled_raw).into_owned()
    } else {
        trim_unsettled_structural(&unsettled_raw)
    };

    let mut parsed_settled_blocks = 0;
    if reparse_settled {
        cache.settled_ast_blocks = if state.settled_text.is_empty() {
            Vec::new()
        } else {
            parse_markdown_impl(&state.settled_text, &parse_opts)
        };
        parsed_settled_blocks = cache.settled_ast_blocks.len();
        cache.settled_revision += 1;
    } else if !settled_delta.is_empty() {
        let delta_blocks = parse_markdown_impl(
            settled_delta,
            &at_offset(&parse_opts, old_settled_end, old_settled_line),
        );
        parsed_settled_blocks = delta_blocks.len();
        append_settled_blocks(&mut cache.settled_ast_blocks, delta_blocks);
        cache.settled_revision += 1;
    }

    // The tail was trimmed before parsing, so its offset is where that text starts, not
    // the boundary. Should it not be found, it is parsed without positions rather than
    // with wrong ones. The search starts at the settled end, so it never scans the prefix.
    let settled_end = cache.settled_end;
    let unsettled_start = if !unsettled_text.is_empty() && opts.ast_positions {
        input[settled_end..]
            .find(unsettled_text.as_str())
            .map(|at| settled_end + at)
    } else {
        None
    };
    let unsettled_blocks = if unsettled_text.is_empty() {
        Vec::new()
    } else {
        let tail_opts = match unsettled_start {
            Some(start) => {
                let line = if opts.source_lines {
                    cache.settled_end_line.unwrap_or(0)
                        + count_line_breaks(input, settled_end, start)
                } else {
                    0
                };
                at_offset(&parse_opts, start, line)
            }
            None => parse_opts.nested(),
        };
        parse_markdown_impl(&unsettled_text, &tail_opts)
    };

    state.prev_input.clear();
    state.prev_input.push_str(input);
    cache.work = IncrementalWork {
        split_characters: tail_raw.len(),
        boundary_lines: tail_lines.len(),
        definition_characters: settled_delta.len() + unsettled_input.len(),
        rendered_blocks: parsed_settled_blocks + unsettled_blocks.len(),
    };
    merge_settled_blocks(&cache.settled_ast_blocks, unsettled_blocks)
}
