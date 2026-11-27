//! Upstream's incremental and streaming parser tests, ported: `Markdown/incremental.test.ts`
//! (every describe), `Markdown/streamingListShapes.test.tsx`,
//! `Markdown/incrementalIndentation.test.tsx`, and from `Markdown/parser.test.ts` the CRLF
//! streaming case and the `sourceRanges` cases that use `parseMarkdownIncremental`.
//!
//! Upstream's incremental calls without options mean `isFinal: false`; here they pass
//! [`streaming`]'s options. Offsets are UTF-8 bytes (every input here is ASCII, so they
//! equal upstream's UTF-16 units). Upstream's object-identity checks (`toBe` on nodes or
//! arrays) are ported as value equality.

use rustryx_markdown::{
    BlockNode, IncrementalState, InlineNode, ParseOptions, create_incremental_state,
    get_incremental_parse_work, parse_markdown, parse_markdown_incremental,
    trim_streaming_artifacts,
};

/// Upstream's options-less incremental call: `isFinal` false, everything else default.
fn streaming() -> ParseOptions {
    ParseOptions {
        is_final: false,
        ..ParseOptions::default()
    }
}

/// Default (complete-input) options, upstream's options-less `parseMarkdown`.
fn full() -> ParseOptions {
    ParseOptions::default()
}

/// Options with `math` and `sourceRanges`, for streaming (`is_final` false).
fn with(math: bool, source_ranges: bool) -> ParseOptions {
    ParseOptions {
        math,
        source_ranges,
        is_final: false,
        ..ParseOptions::default()
    }
}

/// The same options as a complete parse (`is_final` true), for `parse_markdown`.
fn complete(options: &ParseOptions) -> ParseOptions {
    ParseOptions {
        is_final: true,
        ..options.clone()
    }
}

fn md(text: &str) -> Vec<BlockNode> {
    parse_markdown(text, &full())
}

fn incr(text: &str, state: &mut IncrementalState) -> Vec<BlockNode> {
    parse_markdown_incremental(text, state, &streaming())
}

/// Upstream `simulateStreaming`: snapshots every `chunk_size` characters, then the whole
/// text, through one state.
fn simulate_streaming(full_text: &str, chunk_size: usize) -> (Vec<BlockNode>, IncrementalState) {
    let mut state = create_incremental_state();
    let mut i = chunk_size;
    while i <= full_text.len() {
        incr(&full_text[..i], &mut state);
        i += chunk_size;
    }
    let last = incr(full_text, &mut state);
    (last, state)
}

fn list_items(block: &BlockNode) -> &[rustryx_markdown::ListItemNode] {
    match block {
        BlockNode::List { items, .. } => items,
        other => panic!("expected a list, got {other:?}"),
    }
}

/// `source.slice(range.start, range.end)`, or `None` without a range.
fn slice<'a>(source: &'a str, block: &BlockNode) -> Option<&'a str> {
    block.range().map(|range| &source[range.start..range.end])
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

/// `describe('parseMarkdownIncremental')`
mod parse_markdown_incremental {
    use super::*;

    #[test]
    fn produces_same_final_result_as_full_parse() {
        let text = "# Hello\n\nThis is a paragraph.\n\n- Item 1\n- Item 2";
        let (last, _) = simulate_streaming(text, 10);
        assert_eq!(last, md(text));
    }

    #[test]
    fn joins_loose_ordered_list_across_streamed_chunks() {
        let text = "1. apple\n\n1. banana\n\n1. cherry\n";
        let (last, _) = simulate_streaming(text, 5);
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].type_name(), "list");
        let BlockNode::List { items, loose, .. } = &last[0] else {
            unreachable!()
        };
        assert_eq!(items.len(), 3);
        assert_eq!(*loose, Some(true));
    }

    #[test]
    fn keeps_projected_settled_list_identity_when_a_following_block_settles_too() {
        let mut state = create_incremental_state();
        incr("1. a\n\nTail", &mut state);
        let result = incr("1. a\n\n1. b\n\nAfter\n\nTail", &mut state);

        assert_eq!(state.settled_blocks(), md(state.settled_text()).as_slice());
        // Upstream: `toBe` (identity); ported as value equality.
        assert_eq!(result[0], state.settled_blocks()[0]);
    }

    #[test]
    fn joins_loose_paren_ordered_list_across_streamed_chunks() {
        let text = "1) apple\n\n1) banana\n\n1) cherry\n";
        let (last, _) = simulate_streaming(text, 5);
        assert_eq!(last.len(), 1);
        let BlockNode::List {
            items, delimiter, ..
        } = &last[0]
        else {
            panic!("expected a list, got {:?}", last[0]);
        };
        assert_eq!(items.len(), 3);
        assert_eq!(*delimiter, Some(')'));
    }

    #[test]
    fn does_not_merge_ordered_lists_with_different_delimiters_across_chunks() {
        let text = "1. First\n\n2) Second\n";
        let (last, _) = simulate_streaming(text, 4);
        assert_eq!(last, md(text));
        assert_eq!(last.iter().filter(|b| b.type_name() == "list").count(), 2);
    }

    #[test]
    fn handles_empty_input() {
        let mut state = create_incremental_state();
        assert_eq!(incr("", &mut state), vec![]);
    }

    #[test]
    fn settles_blocks_after_double_newline() {
        let mut state = create_incremental_state();
        incr("# Heading\n\nParagraph text\n\nMore text", &mut state);
        assert!(!state.settled_blocks().is_empty());
    }

    #[test]
    fn code_fences_prevent_false_settlement() {
        let mut state = create_incremental_state();
        incr(
            "```javascript\nconst x = 1;\n\n// still in code\nconsole.log(x);",
            &mut state,
        );
        assert!(state.settled_blocks().is_empty());
    }

    #[test]
    fn handles_heading_then_code_block() {
        let text = "# Title\n\n```ts\nconst x = 1;\n```";
        let (last, _) = simulate_streaming(text, 10);
        assert_eq!(last, md(text));
    }

    #[test]
    fn matches_the_full_parse_when_math_delimiters_arrive_across_chunks() {
        let text = "Before $x_1 + *y*$ after.\n\n$$\n\\sum_i x_i\n$$\n\nDone.";
        let options = with(true, false);
        let mut state = create_incremental_state();
        let mut last = vec![];
        for end in 1..=text.len() {
            last = parse_markdown_incremental(&text[..end], &mut state, &options);
        }
        assert_eq!(last, parse_markdown(text, &complete(&options)));
    }

    /// `describe.each('nested display math in a %s')` / `withholds the incomplete container`.
    fn nested_math_withholds(text: &str) {
        for source_ranges in [false, true] {
            let closing = text.rfind("$$").unwrap();
            let mut state = create_incremental_state();
            assert_eq!(
                parse_markdown_incremental(
                    &text[..closing],
                    &mut state,
                    &with(true, source_ranges)
                ),
                vec![],
                "sourceRanges={source_ranges}"
            );
        }
    }

    /// `describe.each('nested display math in a %s')` / `matches the full parse at every
    /// stream split`.
    fn nested_math_matches_full(text: &str) {
        for source_ranges in [false, true] {
            let options = with(true, source_ranges);
            let whole = parse_markdown(text, &complete(&options));

            // One long-lived state sees every character boundary in order.
            let mut state = create_incremental_state();
            let mut result = parse_markdown_incremental("", &mut state, &options);
            for end in 1..=text.len() {
                result = parse_markdown_incremental(&text[..end], &mut state, &options);
            }
            assert_eq!(
                result, whole,
                "character stream, sourceRanges={source_ranges}"
            );

            // The production path trims partial syntax first.
            let mut state = create_incremental_state();
            let mut result = parse_markdown_incremental("", &mut state, &options);
            for end in 1..=text.len() {
                let prefix = trim_streaming_artifacts(&text[..end], true);
                result = parse_markdown_incremental(&prefix, &mut state, &options);
            }
            assert_eq!(
                result, whole,
                "trimmed stream, sourceRanges={source_ranges}"
            );

            // Every two-chunk split converges, directly and trimmed.
            for split in 0..=text.len() {
                let mut state = create_incremental_state();
                parse_markdown_incremental(&text[..split], &mut state, &options);
                assert_eq!(
                    parse_markdown_incremental(text, &mut state, &options),
                    whole,
                    "split {split}, sourceRanges={source_ranges}"
                );

                let mut state = create_incremental_state();
                parse_markdown_incremental(
                    &trim_streaming_artifacts(&text[..split], true),
                    &mut state,
                    &options,
                );
                assert_eq!(
                    parse_markdown_incremental(
                        &trim_streaming_artifacts(text, true),
                        &mut state,
                        &options
                    ),
                    whole,
                    "trimmed split {split}, sourceRanges={source_ranges}"
                );
            }
        }
    }

    const NESTED_LIST: &str = "- $$\n  x + y\n  $$";
    const NESTED_TASK_LIST: &str = "- [ ] $$\n  x + y\n  $$";
    const NESTED_BLOCKQUOTE: &str = "> $$\n> x + y\n> $$";
    const NESTED_BLOCKQUOTE_IN_A_LIST: &str = "- > $$\n  > x + y\n  > $$";
    const NESTED_CRLF_LIST: &str = "- $$\r\n  x + y\r\n  $$\r\n";

    #[test]
    fn nested_display_math_in_a_list_withholds_the_incomplete_container() {
        nested_math_withholds(NESTED_LIST);
    }
    #[test]
    fn nested_display_math_in_a_task_list_withholds_the_incomplete_container() {
        nested_math_withholds(NESTED_TASK_LIST);
    }
    #[test]
    fn nested_display_math_in_a_blockquote_withholds_the_incomplete_container() {
        nested_math_withholds(NESTED_BLOCKQUOTE);
    }
    #[test]
    fn nested_display_math_in_a_blockquote_in_a_list_withholds_the_incomplete_container() {
        nested_math_withholds(NESTED_BLOCKQUOTE_IN_A_LIST);
    }
    #[test]
    fn nested_display_math_in_a_crlf_list_withholds_the_incomplete_container() {
        nested_math_withholds(NESTED_CRLF_LIST);
    }
    #[test]
    fn nested_display_math_in_a_list_matches_the_full_parse_at_every_stream_split() {
        nested_math_matches_full(NESTED_LIST);
    }
    #[test]
    fn nested_display_math_in_a_task_list_matches_the_full_parse_at_every_stream_split() {
        nested_math_matches_full(NESTED_TASK_LIST);
    }
    #[test]
    fn nested_display_math_in_a_blockquote_matches_the_full_parse_at_every_stream_split() {
        nested_math_matches_full(NESTED_BLOCKQUOTE);
    }
    #[test]
    fn nested_display_math_in_a_blockquote_in_a_list_matches_the_full_parse_at_every_stream_split()
    {
        nested_math_matches_full(NESTED_BLOCKQUOTE_IN_A_LIST);
    }
    #[test]
    fn nested_display_math_in_a_crlf_list_matches_the_full_parse_at_every_stream_split() {
        nested_math_matches_full(NESTED_CRLF_LIST);
    }

    /// `describe.each('unmatched display math after a %s ends')` / `returns to literal
    /// parsing`.
    fn unmatched_math_returns_to_literal(text: &str) {
        for source_ranges in [false, true] {
            let options = with(true, source_ranges);
            let whole = parse_markdown(text, &complete(&options));
            for trims in [false, true] {
                let mut state = create_incremental_state();
                let mut result = parse_markdown_incremental("", &mut state, &options);
                for end in 1..=text.len() {
                    let prefix = &text[..end];
                    let snapshot = if trims {
                        trim_streaming_artifacts(prefix, true)
                    } else {
                        prefix.to_string()
                    };
                    result = parse_markdown_incremental(&snapshot, &mut state, &options);
                }
                assert_eq!(
                    result, whole,
                    "sourceRanges={source_ranges}, trimsStreamingArtifacts={trims}"
                );
            }
        }
    }

    #[test]
    fn unmatched_display_math_after_a_list_ends_returns_to_literal_parsing() {
        unmatched_math_returns_to_literal("- $$\n  x\n- next\n\nAfter");
    }
    #[test]
    fn unmatched_display_math_after_a_blockquote_ends_returns_to_literal_parsing() {
        unmatched_math_returns_to_literal("> $$\n> x\n\nAfter");
    }
    #[test]
    fn unmatched_display_math_after_a_deeper_blockquote_ends_returns_to_literal_parsing() {
        unmatched_math_returns_to_literal("> $$\n> > x\n> > $$");
    }
    #[test]
    fn unmatched_display_math_after_a_deeper_blockquote_in_a_list_ends_returns_to_literal_parsing()
    {
        unmatched_math_returns_to_literal("- > $$\n  > > x\n  > > $$");
    }
    #[test]
    fn unmatched_display_math_after_a_deeper_inner_quote_in_a_quoted_list_ends_returns_to_literal_parsing()
     {
        unmatched_math_returns_to_literal("> - $$\n>   > x\n>   > $$");
    }

    #[test]
    fn withholds_an_incomplete_display_math_block_while_streaming() {
        let mut state = create_incremental_state();
        let blocks =
            parse_markdown_incremental("Intro\n\n$$\nx + y", &mut state, &with(true, false));
        assert_eq!(blocks, md("Intro"));
    }

    #[test]
    fn stores_math_nodes_only_in_a_math_enabled_incremental_state() {
        let text = "Intro.\n\nInline $x$.";
        let options = with(true, false);
        let mut state = create_incremental_state();
        let blocks = parse_markdown_incremental(text, &mut state, &options);
        assert_eq!(blocks, parse_markdown(text, &complete(&options)));
        assert_eq!(
            state.settled_blocks(),
            parse_markdown("Intro.", &complete(&options)).as_slice()
        );
    }

    #[test]
    fn handles_table_streaming() {
        let text = "| Col1 | Col2 |\n| --- | --- |\n| a | b |\n| c | d |";
        let (last, _) = simulate_streaming(text, 5);
        assert_eq!(last, md(text));
    }

    #[test]
    fn handles_list_streaming() {
        let text = "- Item 1\n- Item 2\n- Item 3";
        let (last, _) = simulate_streaming(text, 10);
        assert_eq!(last, md(text));
        assert_eq!(list_items(&last[0]).len(), 3);
    }

    /// `it.each('keeps a lazy continuation in its %s at every stream split')`.
    fn lazy_continuation(text: &str) {
        let ranges = ParseOptions {
            source_ranges: true,
            ..ParseOptions::default()
        };
        let streamed = ParseOptions {
            is_final: false,
            ..ranges.clone()
        };
        let whole = parse_markdown(text, &ranges);
        assert_eq!(whole.len(), 2);
        assert!(json(&whole[1]).contains("continued lazily"));

        let mut state = create_incremental_state();
        let mut result = vec![];
        for end in 1..=text.len() {
            result = parse_markdown_incremental(&text[..end], &mut state, &streamed);
        }
        assert_eq!(result, whole, "character stream");

        for split in 0..=text.len() {
            let mut state = create_incremental_state();
            parse_markdown_incremental(&text[..split], &mut state, &streamed);
            assert_eq!(
                parse_markdown_incremental(text, &mut state, &ranges),
                whole,
                "split {split}"
            );
        }
    }

    #[test]
    fn keeps_a_lazy_continuation_in_its_blockquote_at_every_stream_split() {
        lazy_continuation("Intro.\n\n> quoted\ncontinued lazily");
    }
    #[test]
    fn keeps_a_lazy_continuation_in_its_unordered_list_at_every_stream_split() {
        lazy_continuation("Intro.\n\n- listed\ncontinued lazily");
    }
    #[test]
    fn keeps_a_lazy_continuation_in_its_ordered_list_at_every_stream_split() {
        lazy_continuation("Intro.\n\n1. listed\ncontinued lazily");
    }
    #[test]
    fn keeps_a_lazy_continuation_in_its_task_list_at_every_stream_split() {
        lazy_continuation("Intro.\n\n- [ ] listed\ncontinued lazily");
    }
    #[test]
    fn keeps_a_lazy_continuation_in_its_crlf_blockquote_at_every_stream_split() {
        lazy_continuation("Intro.\r\n\r\n> quoted\r\ncontinued lazily");
    }
    #[test]
    fn keeps_a_lazy_continuation_in_its_later_blockquote_paragraph_at_every_stream_split() {
        lazy_continuation("Intro.\n\n> para1\n>\n> para2\ncontinued lazily");
    }
    #[test]
    fn keeps_a_lazy_continuation_in_its_nested_containers_at_every_stream_split() {
        lazy_continuation("Intro.\n\n> 1. > quoted\ncontinued lazily");
    }

    #[test]
    fn handles_1_char_chunks_and_matches_full_parse() {
        let text = "# Hello\n\nWorld";
        let (last, _) = simulate_streaming(text, 1);
        assert_eq!(last, md(text));
    }

    #[test]
    fn handles_multiple_code_fences() {
        let text = "```js\nconst a = 1;\n```\n\n```py\nprint(\"hi\")\n```";
        let (last, _) = simulate_streaming(text, 10);
        assert_eq!(last, md(text));
    }

    #[test]
    fn resets_state_on_empty_input() {
        let mut state = create_incremental_state();
        incr("# Hello\n\nWorld", &mut state);
        assert_ne!(state.prev_input(), "");
        incr("", &mut state);
        assert_eq!(state.prev_input(), "");
        assert!(state.settled_blocks().is_empty());
        assert_eq!(state.settled_up_to(), 0);
        assert_eq!(state.settled_text(), "");
    }

    #[test]
    fn settles_what_precedes_an_unclosed_fence_and_nothing_inside_it() {
        let mut state = create_incremental_state();
        incr(
            "# Title\n\n```python\ndef foo():\n    pass\n\n# still inside fence",
            &mut state,
        );
        assert_eq!(state.settled_text(), "# Title");
        assert_eq!(state.settled_blocks().len(), 1);
    }

    #[test]
    fn does_not_hold_back_lines_inside_an_unclosed_fence() {
        let mut state = create_incremental_state();
        let blocks = incr("Intro\n\n```ts\ntype Id = string | number;", &mut state);
        let code = blocks
            .iter()
            .find_map(|block| match block {
                BlockNode::Codeblock { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .expect("a codeblock");
        assert_eq!(code, "type Id = string | number;");
    }

    #[test]
    fn handles_task_list_streaming() {
        let text = "- [ ] Todo 1\n- [x] Done 1\n- [ ] Todo 2";
        let (last, _) = simulate_streaming(text, 10);
        assert_eq!(last, md(text));
        let items = list_items(&last[0]);
        assert_eq!(items[0].checked, Some(false));
        assert_eq!(items[1].checked, Some(true));
    }

    #[test]
    fn complex_mixed_content_matches_full_parse() {
        let text = [
            "# Overview",
            "",
            "This is an introduction paragraph.",
            "",
            "```typescript",
            "function hello() {",
            "  return \"world\";",
            "}",
            "```",
            "",
            "| Feature | Status |",
            "| --- | --- |",
            "| Auth | Done |",
            "",
            "> A blockquote with important info.",
            "",
            "1. First step",
            "2. Second step",
            "",
            "---",
            "",
            "Visit [our site](https://example.com) for more.",
        ]
        .join("\n");
        let (last, _) = simulate_streaming(&text, 15);
        assert_eq!(last, md(&text));
    }

    #[test]
    fn reuses_cached_settled_blocks_when_settled_portion_is_unchanged() {
        let mut state = create_incremental_state();
        incr("# Title\n\nParagraph\n\nStill typing", &mut state);
        let cached_text = state.settled_text().to_string();
        let cached_blocks = state.settled_blocks().to_vec();

        incr("# Title\n\nParagraph\n\nStill typing more", &mut state);
        // Upstream checks the array's identity; ported as an unchanged settled prefix.
        assert_eq!(state.settled_text(), cached_text);
        assert_eq!(state.settled_blocks(), cached_blocks.as_slice());
    }

    #[test]
    fn returns_a_fresh_array_and_leaves_earlier_snapshots_untouched() {
        let mut state = create_incremental_state();
        let first = incr("# Stable\n\nFirst paragraph\n\nTail", &mut state);
        let stable_heading = first[0].clone();
        let snapshot = first.clone();

        let second = incr("# Stable\n\nFirst paragraph\n\nTail grows", &mut state);

        // `expect(second).not.toBe(first)` is identity, which owned Rust values give.
        assert_eq!(first, snapshot);
        // Upstream: `toBe` (identity); ported as value equality.
        assert_eq!(second[0], stable_heading);
        assert_eq!(second, md("# Stable\n\nFirst paragraph\n\nTail grows"));
    }

    #[test]
    fn re_parses_when_settled_text_is_replaced_at_the_same_length() {
        let mut state = create_incremental_state();
        let before = "# Alpha\n\nOld body copy";
        let after = "# Bravo\n\nNew body copy";
        assert_eq!(after.len(), before.len());

        incr(before, &mut state);
        let blocks = incr(after, &mut state);
        assert_eq!(blocks, md(after));
    }

    #[test]
    fn re_parses_when_settled_text_is_replaced_by_longer_content() {
        let mut state = create_incremental_state();
        incr("# Alpha\n\nOld body copy", &mut state);

        let after = "# Bravo heading\n\nNew body copy, longer than before\n\nTail";
        let blocks = incr(after, &mut state);
        assert_eq!(blocks, md(after));
    }

    #[test]
    fn keeps_the_prefix_settled_while_a_long_fence_streams_and_closes() {
        let mut state = create_incremental_state();
        let prefix = "# Stable\n\n";
        let fence = "```ts\nconst a = 1;\n\nconst b = 2;";
        let mut stable_heading: Option<BlockNode> = None;

        for end in 1..=fence.len() {
            let blocks = incr(&format!("{prefix}{}", &fence[..end]), &mut state);
            let heading = stable_heading.get_or_insert_with(|| blocks[0].clone());
            // Upstream: `toBe` (identity); ported as value equality.
            assert_eq!(&blocks[0], heading, "end {end}");
            assert_eq!(state.settled_text(), "# Stable", "end {end}");
        }

        let whole = format!("{prefix}{fence}\n```\n\nAfter");
        let blocks = incr(&whole, &mut state);
        assert_eq!(Some(&blocks[0]), stable_heading.as_ref());
        assert_eq!(blocks, md(&whole));
    }

    #[test]
    fn tracks_settled_text_correctly() {
        let mut state = create_incremental_state();
        incr("# Hello\n\nWorld", &mut state);
        assert_eq!(state.settled_text(), "# Hello");
    }
}

/// `describe('trimStreamingArtifacts')`
mod trim_streaming_artifacts_tests {
    use super::*;

    fn trim(input: &str) -> String {
        trim_streaming_artifacts(input, false)
    }

    #[test]
    fn trims_trailing_unclosed_bold_markers() {
        assert_eq!(trim("Hello **"), "Hello ");
        assert_eq!(trim("Hello *"), "Hello ");
        assert_eq!(trim("Hello ***"), "Hello ");
    }

    #[test]
    fn auto_closes_unclosed_mid_line_bold() {
        assert_eq!(trim("Hello **bold"), "Hello **bold**");
        assert_eq!(trim("Hello **bo"), "Hello **bo**");
        assert_eq!(trim("Hello **"), "Hello ");
    }

    #[test]
    fn auto_closes_unclosed_mid_line_italic() {
        assert_eq!(trim("Hello *ital"), "Hello *ital*");
        assert_eq!(trim("Hello *"), "Hello ");
    }

    #[test]
    fn preserves_complete_bold_italic_in_same_line() {
        assert_eq!(
            trim("Hello **bold** and *italic*"),
            "Hello **bold** and *italic*"
        );
    }

    #[test]
    fn auto_closes_unclosed_bold_after_closed_bold() {
        assert_eq!(
            trim("Hello **done** and **open"),
            "Hello **done** and **open**"
        );
    }

    #[test]
    fn preserves_closed_bold_markers() {
        assert_eq!(trim("Hello **world**"), "Hello **world**");
    }

    #[test]
    fn trims_trailing_unclosed_backticks() {
        assert_eq!(trim("Hello `"), "Hello ");
        assert_eq!(trim("Hello ```"), "Hello ");
    }

    #[test]
    fn preserves_closed_inline_code() {
        assert_eq!(trim("Hello `code`"), "Hello `code`");
    }

    #[test]
    fn withholds_incomplete_inline_math_only_when_math_parsing_is_enabled() {
        assert_eq!(trim_streaming_artifacts("Value $", true), "Value ");
        assert_eq!(trim_streaming_artifacts("Value $x + 1", true), "Value ");
        assert_eq!(trim("Value $x + 1"), "Value $x + 1");
        assert_eq!(
            trim_streaming_artifacts("Value $x + 1$", true),
            "Value $x + 1$"
        );
    }

    #[test]
    fn trims_trailing_unclosed_strikethrough() {
        assert_eq!(trim("Hello ~~"), "Hello ");
        assert_eq!(trim("Hello ~"), "Hello ");
    }

    #[test]
    fn trims_unclosed_mid_line_strikethrough() {
        assert_eq!(trim("Hello ~~struck"), "Hello ");
        assert_eq!(trim("Hello ~~s"), "Hello ");
    }

    #[test]
    fn preserves_closed_strikethrough() {
        assert_eq!(trim("Hello ~~struck~~ more"), "Hello ~~struck~~ more");
    }

    #[test]
    fn trims_unclosed_link_text() {
        assert_eq!(trim("Hello [link text"), "Hello ");
        assert_eq!(trim("Hello [link"), "Hello ");
    }

    #[test]
    fn preserves_closed_links() {
        assert_eq!(trim("Hello [text](url)"), "Hello [text](url)");
    }

    #[test]
    fn trims_unclosed_image_alt() {
        assert_eq!(trim("Hello ![alt"), "Hello ");
    }

    #[test]
    fn operates_only_on_the_last_line() {
        assert_eq!(trim("Line 1\nHello **"), "Line 1\nHello ");
        assert_eq!(
            trim("Line 1\n**bold** done\nHello *"),
            "Line 1\n**bold** done\nHello "
        );
    }

    #[test]
    fn handles_empty_input() {
        assert_eq!(trim(""), "");
    }

    #[test]
    fn handles_input_with_no_artifacts() {
        assert_eq!(trim("Just plain text"), "Just plain text");
    }
}

/// `describe('streaming structural suppression')`
mod streaming_structural_suppression {
    use super::*;

    #[test]
    fn suppresses_bare_list_markers_in_unsettled_zone() {
        let mut state = create_incremental_state();
        let blocks = incr("Some text\n\n- ", &mut state);
        assert!(!blocks.iter().any(|b| b.type_name() == "list"));
    }

    #[test]
    fn renders_list_items_once_text_arrives() {
        let mut state = create_incremental_state();
        let blocks = incr("Some text\n\n- First item", &mut state);
        let list = blocks
            .iter()
            .find(|b| b.type_name() == "list")
            .expect("a list");
        assert_eq!(list_items(list).len(), 1);
    }

    #[test]
    fn suppresses_incomplete_table_header_without_separator() {
        let mut state = create_incremental_state();
        let blocks = incr("Intro\n\n| Col1 | Col2 |", &mut state);
        assert!(!blocks.iter().any(|b| b.type_name() == "table"));
    }

    #[test]
    fn suppresses_table_separator_without_data_rows() {
        // Upstream asserts nothing here: a header and separator with no rows is a valid
        // table, which the parser may render. The call must simply not fail.
        let mut state = create_incremental_state();
        incr("Intro\n\n| Col1 | Col2 |\n| --- | --- |", &mut state);
    }

    #[test]
    fn renders_complete_table_once_rows_arrive() {
        let mut state = create_incremental_state();
        let blocks = incr(
            "Intro\n\n| Col1 | Col2 |\n| --- | --- |\n| a | b |",
            &mut state,
        );
        let Some(BlockNode::Table { rows, .. }) = blocks.iter().find(|b| b.type_name() == "table")
        else {
            panic!("expected a table in {blocks:?}");
        };
        assert_eq!(rows.len(), 1);
    }

    /// `describe('escaped pipes in the unsettled tail')`
    mod escaped_pipes_in_the_unsettled_tail {
        use super::*;

        fn from_inline(nodes: &[InlineNode]) -> String {
            nodes
                .iter()
                .map(|node| match node {
                    InlineNode::Text { content } | InlineNode::Code { content } => content.clone(),
                    InlineNode::Math { value } => value.clone(),
                    InlineNode::Bold { children }
                    | InlineNode::Italic { children }
                    | InlineNode::Strikethrough { children }
                    | InlineNode::Link { children, .. } => from_inline(children),
                    InlineNode::Break => "\n".to_string(),
                    InlineNode::Image { alt, .. } => alt.clone(),
                    InlineNode::Citation { .. } | InlineNode::Extension(_) => String::new(),
                })
                .collect()
        }

        /// Upstream `visibleText`: the text a reader would see for one block.
        fn visible_text(block: &BlockNode) -> String {
            match block {
                BlockNode::Heading { children, .. } | BlockNode::Paragraph { children, .. } => {
                    from_inline(children)
                }
                BlockNode::Codeblock { content, .. } => content.clone(),
                BlockNode::Math { value, .. } => value.clone(),
                BlockNode::Blockquote { children, .. } => children
                    .iter()
                    .map(visible_text)
                    .collect::<Vec<_>>()
                    .join("\n"),
                BlockNode::List { items, .. } => items
                    .iter()
                    .map(|item| {
                        item.children
                            .iter()
                            .map(visible_text)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                BlockNode::Table { headers, rows, .. } => std::iter::once(headers)
                    .chain(rows.iter())
                    .map(|row| {
                        row.iter()
                            .map(|cell| from_inline(&cell.children))
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                BlockNode::Image { alt, .. } => alt.clone(),
                BlockNode::Hr { .. } | BlockNode::Extension(_) => String::new(),
            }
        }

        /// Upstream `expectedTail`: every `\\|` one literal pipe, whitespace trimmed.
        fn expected_tail(source: &str) -> String {
            source.replace("\\|", "|").trim().to_string()
        }

        #[test]
        fn renders_an_unfinished_first_line_whose_only_pipes_are_escaped() {
            let text = "Costs 5 \\| 10 per unit";
            let mut state = create_incremental_state();
            let blocks = incr(text, &mut state);
            assert_eq!(blocks, md(text));
            assert_eq!(blocks.len(), 1);
        }

        #[test]
        fn renders_an_unfinished_tail_line_after_a_settled_block() {
            let text = "Intro\n\nCosts 5 \\| 10 per unit";
            let mut state = create_incremental_state();
            let blocks = incr(text, &mut state);
            assert_eq!(blocks, md(text));
            assert_eq!(blocks.len(), 2);
        }

        #[test]
        fn keeps_the_whole_tail_visible_at_every_prefix_with_no_settled_text() {
            let text = "Costs 5 \\| 10 per unit";
            let mut state = create_incremental_state();
            for length in 1..=text.len() {
                let blocks = incr(&text[..length], &mut state);
                let tail = expected_tail(&text[..length]);
                assert_eq!(blocks.len(), 1, "length {length}");
                assert_eq!(visible_text(&blocks[0]), tail, "length {length}");
            }
        }

        #[test]
        fn keeps_the_whole_tail_visible_at_every_prefix_after_a_settled_block() {
            let settled = "Intro\n\n";
            let text = format!("{settled}Costs 5 \\| 10 per unit");
            let mut state = create_incremental_state();
            for length in settled.len() + 1..=text.len() {
                let blocks = incr(&text[..length], &mut state);
                let tail = expected_tail(&text[settled.len()..length]);
                assert_eq!(blocks.len(), 2, "length {length}");
                assert_eq!(visible_text(&blocks[0]), "Intro", "length {length}");
                assert_eq!(visible_text(&blocks[1]), tail, "length {length}");
            }
            assert_eq!(incr(&text, &mut state), md(&text));
        }

        #[test]
        fn keeps_parsing_bounded_to_the_tail_as_the_line_streams_in() {
            let prefix = "Filler paragraph.\n\n".repeat(40);
            let text = format!("{prefix}Costs 5 \\| 10 per unit");
            let mut state = create_incremental_state();
            // Prime the cache: the first call reads the whole snapshot by definition.
            incr(&format!("{prefix}C"), &mut state);
            let mut worst_split_characters = 0;
            for length in prefix.len() + 2..=text.len() {
                incr(&text[..length], &mut state);
                worst_split_characters =
                    worst_split_characters.max(get_incremental_parse_work(&state).split_characters);
            }
            // Only the unsettled tail is re-split, never the settled filler.
            assert!(
                worst_split_characters < text.len() - prefix.len() + 5,
                "worst {worst_split_characters}"
            );
        }

        #[test]
        fn decodes_escaped_pipes_in_code_spans_during_production_streaming() {
            let text = "Intro\n\n| Concept | TypeScript |\n| --- | --- |\n| Null safety | **`T \\| null`** |";
            for source_ranges in [false, true] {
                let options = with(false, source_ranges);
                let whole = parse_markdown(text, &complete(&options));

                for trims in [false, true] {
                    let mut state = create_incremental_state();
                    let mut streamed = parse_markdown_incremental("", &mut state, &options);
                    for end in 1..=text.len() {
                        let prefix = &text[..end];
                        let snapshot = if trims {
                            trim_streaming_artifacts(prefix, false)
                        } else {
                            prefix.to_string()
                        };
                        streamed = parse_markdown_incremental(&snapshot, &mut state, &options);
                    }
                    let label = format!("sourceRanges={source_ranges}, trims={trims}");
                    assert_eq!(state.settled_text(), "Intro", "{label}");
                    assert_eq!(streamed, whole, "{label}");
                    assert_eq!(streamed.len(), 2, "{label}");
                    assert_eq!(streamed[1].type_name(), "table", "{label}");
                    assert_eq!(
                        visible_text(&streamed[1]),
                        "Concept TypeScript\nNull safety T | null",
                        "{label}"
                    );
                }
            }
        }

        #[test]
        fn still_holds_back_a_lone_header_whose_pipes_are_unescaped() {
            let mut state = create_incremental_state();
            let blocks = incr("Col1 | Col2", &mut state);
            assert!(blocks.is_empty(), "{blocks:?}");
        }

        #[test]
        fn streams_table_rows_containing_escaped_pipes_once_the_table_exists() {
            let text = "| Col1 | Col2 |\n| --- | --- |\n| a \\| b | c |";
            let mut state = create_incremental_state();
            let blocks = incr(text, &mut state);
            let Some(BlockNode::Table { rows, .. }) =
                blocks.iter().find(|b| b.type_name() == "table")
            else {
                panic!("expected a table in {blocks:?}");
            };
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].len(), 2);
        }
    }

    #[test]
    fn suppresses_ordered_list_marker_without_content() {
        let mut state = create_incremental_state();
        let blocks = incr("Intro\n\n1. ", &mut state);
        assert!(!blocks.iter().any(|b| b.type_name() == "list"));
    }

    #[test]
    fn no_suppression_for_complete_list_items() {
        let mut state = create_incremental_state();
        let blocks = incr("Intro\n\n- Complete item\n- Another item", &mut state);
        let list = blocks
            .iter()
            .find(|b| b.type_name() == "list")
            .expect("a list");
        assert_eq!(list_items(list).len(), 2);
    }

    #[test]
    fn character_by_character_streaming_never_produces_bare_list_markers() {
        let full_text = "Hello world\n\n- Item one\n- Item two\n- Item three";
        let mut state = create_incremental_state();
        for i in 1..=full_text.len() {
            let blocks = incr(&full_text[..i], &mut state);
            for block in &blocks {
                let BlockNode::List { items, .. } = block else {
                    continue;
                };
                for item in items {
                    if let Some(BlockNode::Paragraph { children, .. }) = item.children.first() {
                        let text_content: String = children
                            .iter()
                            .map(|c| match c {
                                InlineNode::Text { content } => content.as_str(),
                                _ => "",
                            })
                            .collect();
                        assert!(!text_content.trim().is_empty(), "prefix {i}: {blocks:?}");
                    }
                }
            }
        }
    }
}

/// `describe('streaming end-to-end: no raw syntax visible')`
mod streaming_end_to_end_no_raw_syntax_visible {
    use super::*;

    /// Upstream `extractVisibleText`.
    fn extract_visible_text(blocks: &[BlockNode]) -> String {
        let mut text = String::new();
        for block in blocks {
            match block {
                BlockNode::Heading { children, .. } | BlockNode::Paragraph { children, .. } => {
                    text += &extract_inline_text(children)
                }
                BlockNode::Codeblock { content, .. } => text += content,
                BlockNode::Math { value, .. } => text += value,
                BlockNode::Blockquote { children, .. } => text += &extract_visible_text(children),
                BlockNode::List { items, .. } => {
                    for item in items {
                        text += &extract_visible_text(&item.children);
                    }
                }
                BlockNode::Table { headers, rows, .. } => {
                    for h in headers {
                        text += &extract_inline_text(&h.children);
                    }
                    for row in rows {
                        for cell in row {
                            text += &extract_inline_text(&cell.children);
                        }
                    }
                }
                BlockNode::Hr { .. } | BlockNode::Extension(_) => {}
                BlockNode::Image { alt, .. } => text += alt,
            }
        }
        text
    }

    /// Upstream `extractInlineText`.
    fn extract_inline_text(nodes: &[InlineNode]) -> String {
        let mut text = String::new();
        for node in nodes {
            match node {
                InlineNode::Text { content } | InlineNode::Code { content } => text += content,
                InlineNode::Math { value } => text += value,
                InlineNode::Bold { children }
                | InlineNode::Italic { children }
                | InlineNode::Strikethrough { children }
                | InlineNode::Link { children, .. } => text += &extract_inline_text(children),
                InlineNode::Image { alt, .. } => text += alt,
                InlineNode::Citation { source_id } => text += source_id,
                InlineNode::Break => text.push('\n'),
                InlineNode::Extension(_) => {}
            }
        }
        text
    }

    /// Upstream `streamCharByChar`: the visible text of every trimmed snapshot.
    fn stream_char_by_char(full_text: &str) -> Vec<String> {
        let mut state = create_incremental_state();
        (1..=full_text.len())
            .map(|i| {
                let trimmed = trim_streaming_artifacts(&full_text[..i], false);
                extract_visible_text(&incr(&trimmed, &mut state))
            })
            .collect()
    }

    #[test]
    fn bold_text_appears_progressively_during_streaming() {
        let visible_texts = stream_char_by_char("Hello **bold text** and more");
        for visible in visible_texts.iter().filter(|t| !t.is_empty()) {
            if visible.len() >= 5 {
                assert!(visible.starts_with("Hello"), "{visible:?}");
            }
        }
        assert_eq!(
            visible_texts.last().map(String::as_str),
            Some("Hello bold text and more")
        );
    }

    #[test]
    fn never_shows_raw_double_asterisk_during_bold_streaming() {
        for visible in stream_char_by_char("Hello **bold text** and more") {
            assert!(!visible.contains("**"), "{visible:?}");
        }
    }

    /// `/(?:^|\s)\*(?:\s|$)/`: a lone `*` that is not part of a word.
    fn has_lone_asterisk(text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        chars.iter().enumerate().any(|(i, &c)| {
            c == '*'
                && (i == 0 || chars[i - 1].is_whitespace())
                && chars.get(i + 1).is_none_or(|next| next.is_whitespace())
        })
    }

    #[test]
    fn never_shows_raw_asterisk_during_italic_streaming() {
        for visible in stream_char_by_char("Hello *italic text* and more") {
            assert!(!has_lone_asterisk(&visible), "{visible:?}");
        }
    }

    #[test]
    fn never_shows_raw_tildes_during_strikethrough_streaming() {
        for visible in stream_char_by_char("Hello ~~struck~~ and more") {
            assert!(!visible.contains("~~"), "{visible:?}");
        }
    }

    #[test]
    fn never_shows_raw_bracket_during_link_streaming() {
        for visible in stream_char_by_char("Visit [our site](https://example.com) for info") {
            assert!(!visible.contains('['), "{visible:?}");
            assert!(!visible.contains("]("), "{visible:?}");
        }
    }

    #[test]
    fn never_shows_raw_image_bracket_during_image_streaming() {
        for visible in stream_char_by_char("See ![alt text](https://img.png) here") {
            assert!(!visible.contains("!["), "{visible:?}");
        }
    }

    #[test]
    fn table_rows_render_incrementally_after_header_established() {
        let text = "| A | B |\n| --- | --- |\n| r1a | r1b |\n| r2a | r2b |\n| r3a | r3b |";
        let mut state = create_incremental_state();
        let mut max_rows = 0;
        let lines: Vec<&str> = text.split('\n').collect();
        for line_count in 1..=lines.len() {
            let input = lines[..line_count].join("\n");
            let trimmed = trim_streaming_artifacts(&input, false);
            let blocks = incr(&trimmed, &mut state);
            if let Some(BlockNode::Table { rows, .. }) =
                blocks.iter().find(|b| b.type_name() == "table")
            {
                max_rows = max_rows.max(rows.len());
            }
        }
        assert_eq!(max_rows, 3);
    }

    #[test]
    fn complex_mixed_content_char_by_char_never_shows_raw_syntax() {
        let text = [
            "## Heading",
            "",
            "A paragraph with **bold**, *italic*, and `code`.",
            "",
            "- List item **one**",
            "- List item [two](http://two.com)",
            "- List item ~~three~~",
            "",
            "| Col | Val |",
            "| --- | --- |",
            "| **a** | *b* |",
        ]
        .join("\n");
        for visible in stream_char_by_char(&text) {
            assert!(!visible.contains("]("), "{visible:?}");
            assert!(!visible.contains("!["), "{visible:?}");
        }
    }

    /// `describe('with autolink option')`
    mod with_autolink_option {
        use super::*;

        fn autolink(is_final: bool) -> ParseOptions {
            ParseOptions {
                autolink_gfm: true,
                is_final,
                ..ParseOptions::default()
            }
        }

        fn has_link(blocks: &[BlockNode]) -> bool {
            blocks.iter().any(|b| match b {
                BlockNode::Paragraph { children, .. } => children
                    .iter()
                    .any(|n| matches!(n, InlineNode::Link { .. })),
                _ => false,
            })
        }

        #[test]
        fn produces_same_final_result_as_full_parse_with_autolink_gfm() {
            let text = "Visit https://example.com or mail user@example.com today.";
            let mut state = create_incremental_state();
            let incremental = parse_markdown_incremental(text, &mut state, &autolink(false));
            assert_eq!(incremental, parse_markdown(text, &autolink(true)));
            let BlockNode::Paragraph { children, .. } = &incremental[0] else {
                panic!("expected paragraph block");
            };
            let link_hrefs: Vec<&str> = children
                .iter()
                .filter_map(|n| match n {
                    InlineNode::Link { href, .. } => Some(href.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                link_hrefs,
                ["https://example.com", "mailto:user@example.com"]
            );
        }

        #[test]
        fn streams_character_by_character_to_the_same_final_result() {
            let text = "see https://example.com here and mail me@example.com soon.";
            let mut state = create_incremental_state();
            let mut last = vec![];
            for i in 1..=text.len() {
                last = parse_markdown_incremental(&text[..i], &mut state, &autolink(false));
            }
            assert_eq!(last, parse_markdown(text, &autolink(true)));
        }

        #[test]
        fn invalidates_the_cache_when_the_autolink_option_toggles() {
            let text = "first https://a.com.\n\nsecond https://b.com.";
            let mut state = create_incremental_state();
            let off = incr(text, &mut state);
            assert_eq!(off, md(text));
            assert!(!has_link(&off));
            let on = parse_markdown_incremental(text, &mut state, &autolink(false));
            assert_eq!(on, parse_markdown(text, &autolink(true)));
            assert!(has_link(&on));
            let off_again = incr(text, &mut state);
            assert_eq!(off_again, md(text));
        }
    }
}

/// `describe('parseMarkdownIncremental link reference definitions')`
mod parse_markdown_incremental_link_reference_definitions {
    use super::*;

    fn first_link_href(blocks: &[BlockNode]) -> Option<&str> {
        blocks.iter().find_map(|block| match block {
            BlockNode::Paragraph { children, .. } => children.iter().find_map(|n| match n {
                InlineNode::Link { href, .. } => Some(href.as_str()),
                _ => None,
            }),
            _ => None,
        })
    }

    #[test]
    fn resolves_a_footer_reference_streamed_across_chunks_matches_full_parse() {
        let text = "See [the docs][d] here.\n\n[d]: https://example.com/d\n";
        let (last, _) = simulate_streaming(text, 5);
        assert_eq!(last, md(text));
        assert_eq!(first_link_href(&last), Some("https://example.com/d"));
    }

    #[test]
    fn re_resolves_an_already_settled_reference_once_its_definition_arrives() {
        let mut state = create_incremental_state();
        incr("See [d] here.\n\n", &mut state);
        let before = incr("See [d] here.\n\n[d", &mut state);
        assert_eq!(first_link_href(&before), None);
        let after = incr("See [d] here.\n\n[d]: /docs\n", &mut state);
        assert_eq!(first_link_href(&after), Some("/docs"));
    }

    #[test]
    fn keeps_first_definition_wins_when_definitions_move_into_the_settled_prefix() {
        let mut state = create_incremental_state();
        let first = "See [d].\n\n[d]: /first\n\nMiddle paragraph\n\n[d]: /second";
        assert_eq!(first_link_href(&incr(first, &mut state)), Some("/first"));

        let whole = format!("{first}\n\nAfter");
        let blocks = incr(&whole, &mut state);
        assert_eq!(blocks, md(&whole));
        assert_eq!(first_link_href(&blocks), Some("/first"));
    }
}

/// `streamingListShapes.test.tsx`: `describe('lists a blank line apart, streamed')`
mod lists_a_blank_line_apart_streamed {
    use super::*;

    /// Upstream `streamInChunks`: the streamed parse in chunks of `sizes`, cycled.
    fn stream_in_chunks(markdown: &str, sizes: &[usize], source_ranges: bool) -> Vec<BlockNode> {
        let options = with(false, source_ranges);
        let mut state = create_incremental_state();
        let mut result = parse_markdown_incremental("", &mut state, &options);
        let mut end = 0;
        let mut step = 0;
        while end < markdown.len() {
            end = markdown.len().min(end + sizes[step % sizes.len()]);
            result = parse_markdown_incremental(&markdown[..end], &mut state, &options);
            step += 1;
        }
        result
    }

    fn list_count(blocks: &[BlockNode]) -> usize {
        blocks.iter().filter(|b| b.type_name() == "list").count()
    }

    /// `it.each('streams %s as the full parse reads it')`.
    fn streams_as_full_parse(markdown: &str, lists: usize) {
        assert_eq!(list_count(&md(markdown)), lists);
        for source_ranges in [false, true] {
            let options = ParseOptions {
                source_ranges,
                ..ParseOptions::default()
            };
            assert_eq!(
                stream_in_chunks(markdown, &[1], source_ranges),
                parse_markdown(markdown, &options),
                "sourceRanges={source_ranges}"
            );
        }
    }

    #[test]
    fn streams_a_bullet_change_as_the_full_parse_reads_it() {
        streams_as_full_parse("- a\n\n* b", 2);
    }
    #[test]
    fn streams_another_bullet_change_as_the_full_parse_reads_it() {
        streams_as_full_parse("* a\n\n- b", 2);
    }
    #[test]
    fn streams_two_bullet_changes_as_the_full_parse_reads_it() {
        streams_as_full_parse("- a\n\n+ b\n\n- c", 3);
    }
    #[test]
    fn streams_numbered_items_at_another_indentation_as_the_full_parse_reads_it() {
        streams_as_full_parse(" 1. a\n\n2. b", 2);
    }
    #[test]
    fn streams_bullets_at_another_indentation_as_the_full_parse_reads_it() {
        streams_as_full_parse(" - a\n\n- b", 2);
    }
    #[test]
    fn streams_one_list_then_a_bullet_change_as_the_full_parse_reads_it() {
        streams_as_full_parse("- a\n\n- b\n\n* c", 2);
    }
    #[test]
    fn streams_one_loose_bulleted_list_as_the_full_parse_reads_it() {
        streams_as_full_parse("- a\n\n- b\n\n- c", 1);
    }
    #[test]
    fn streams_one_loose_numbered_list_as_the_full_parse_reads_it() {
        streams_as_full_parse("1. a\n\n3. b\n\n4. c", 1);
    }

    /// Upstream's LCG, `state = (state * 1103515245 + 12345) % 2147483648`. JavaScript
    /// evaluates it in doubles, and the product exceeds 2^53, so the multiplication rounds;
    /// replaying it in `f64` (IEEE multiply, add and `%`, as JavaScript does) gives
    /// upstream's exact sequence, which exact integer arithmetic would not.
    struct Lcg(f64);

    impl Lcg {
        fn random(&mut self) -> f64 {
            self.0 = (self.0 * 1_103_515_245.0 + 12_345.0) % 2_147_483_648.0;
            self.0 / 2_147_483_648.0
        }

        /// `Math.floor(random() * n)`.
        fn below(&mut self, n: usize) -> usize {
            (self.random() * n as f64).floor() as usize
        }

        fn pick<T: Copy>(&mut self, choices: &[T]) -> T {
            choices[self.below(choices.len())]
        }
    }

    fn answer(random: &mut Lcg) -> String {
        let mut lines: Vec<String> = Vec::new();
        let items = 2 + random.below(6);
        for item in 0..items {
            let indent = " ".repeat(random.pick(&[0, 0, 0, 1, 2]));
            let marker = random.pick(&["- ", "* ", "+ ", "1. ", "2) ", "3. "]);
            lines.push(format!("{indent}{marker}item {item}"));
            if random.random() < 0.2 {
                lines.push(format!("{indent}{}more", " ".repeat(marker.len())));
            }
            if random.random() < 0.7 {
                lines.push(String::new());
            }
            if random.random() < 0.15 {
                lines.push("A paragraph.".to_string());
                lines.push(String::new());
            }
        }
        lines.join("\n").trim_end().to_string()
    }

    #[test]
    fn streams_600_answers_mixing_list_styles_to_the_full_parse() {
        let mut random = Lcg(5656.0);
        let mut diverged: Vec<String> = Vec::new();
        for round in 0..600 {
            let markdown = answer(&mut random);
            let sizes = if round % 10 == 0 {
                vec![1]
            } else {
                let first = 1 + random.below(12);
                let second = 1 + random.below(5);
                vec![first, second]
            };
            if json(&stream_in_chunks(&markdown, &sizes, false)) != json(&md(&markdown)) {
                diverged.push(markdown);
            }
        }
        assert_eq!(diverged, Vec::<String>::new());
    }
}

/// `incrementalIndentation.test.tsx`: `describe("a streamed document keeps its first line's
/// indentation")`
mod a_streamed_document_keeps_its_first_lines_indentation {
    use super::*;

    /// `it.each('gives %s the full parse, at once and streamed')`.
    fn gives_the_full_parse(markdown: &str) {
        let whole = md(markdown);
        assert_eq!(
            incr(markdown, &mut create_incremental_state()),
            whole,
            "at once"
        );
        let mut state = create_incremental_state();
        let mut streamed = incr("", &mut state);
        for end in 1..=markdown.len() {
            streamed = incr(&markdown[..end], &mut state);
        }
        assert_eq!(streamed, whole, "streamed");
    }

    #[test]
    fn gives_sibling_items_indented_two_spaces_the_full_parse_at_once_and_streamed() {
        gives_the_full_parse("  - a\n  - b");
    }
    #[test]
    fn gives_ordered_items_indented_one_space_the_full_parse_at_once_and_streamed() {
        gives_the_full_parse(" 1. a\n 2. b");
    }
    #[test]
    fn gives_a_paragraph_indented_one_space_the_full_parse_at_once_and_streamed() {
        gives_the_full_parse(" 1");
    }
    #[test]
    fn gives_two_paragraphs_each_indented_the_full_parse_at_once_and_streamed() {
        gives_the_full_parse(" a\n\n b");
    }
    #[test]
    fn gives_a_table_indented_two_spaces_the_full_parse_at_once_and_streamed() {
        gives_the_full_parse("  | a | b |\n  | - | - |\n  | 1 | 2 |");
    }
    #[test]
    fn gives_a_heading_marker_indented_three_spaces_the_full_parse_at_once_and_streamed() {
        gives_the_full_parse("   # h");
    }

    #[test]
    fn still_drops_blank_lines_before_the_first_block() {
        assert_eq!(
            incr("\n\n  - a\n  - b", &mut create_incremental_state()),
            md("\n\n  - a\n  - b")
        );
    }
}

/// `parser.test.ts`: `describe('parseMarkdown')` / `describe('hard breaks in CRLF documents')`
mod hard_breaks_in_crlf_documents {
    use super::*;

    fn has_break(blocks: &[BlockNode]) -> bool {
        json(&blocks).contains("\"type\":\"break\"")
    }

    #[test]
    fn streams_to_the_same_result_when_chunks_split_the_line_ending() {
        let source = "One  \r\ntwo\\\r\nthree\r\n\r\nC:\\Users\r\n";
        let mut state = create_incremental_state();
        let mut blocks = vec![];
        for end in 1..=source.len() {
            blocks = incr(&source[..end], &mut state);
        }
        assert_eq!(blocks, md(source));
        assert!(has_break(&blocks));
    }
}

/// `parser.test.ts`: `describe('sourceRanges')`, the cases that parse incrementally.
mod source_ranges {
    use super::*;

    fn ranged() -> ParseOptions {
        with(false, true)
    }

    #[test]
    fn reports_absolute_offsets_when_the_document_is_parsed_incrementally() {
        let source = ["# Title", "", "One.", "", "Two.", "", "Three."].join("\n");
        let mut state = create_incremental_state();
        let mut blocks = vec![];
        for end in 1..=source.len() {
            blocks = parse_markdown_incremental(&source[..end], &mut state, &ranged());
        }
        let slices: Vec<Option<&str>> = blocks.iter().map(|b| slice(&source, b)).collect();
        assert_eq!(
            slices,
            [Some("# Title"), Some("One."), Some("Two."), Some("Three.")]
        );
    }

    #[test]
    fn re_parses_when_the_caller_flips_the_option_on_an_existing_state() {
        let source = "One.\n\nTwo.\n\nThree.";
        let mut state = create_incremental_state();
        let without = incr(source, &mut state);
        assert!(without.iter().all(|b| b.range().is_none()));
        let with_ = parse_markdown_incremental(source, &mut state, &ranged());
        let slices: Vec<Option<&str>> = with_.iter().map(|b| slice(source, b)).collect();
        assert_eq!(slices, [Some("One."), Some("Two."), Some("Three.")]);
        let back = incr(source, &mut state);
        assert!(back.iter().all(|b| b.range().is_none()));
    }

    #[test]
    fn spans_both_halves_of_a_list_the_incremental_parser_merged() {
        let source = "1. one\n\n1. two";
        let mut state = create_incremental_state();
        parse_markdown_incremental("1. one\n\n", &mut state, &ranged());
        let blocks = parse_markdown_incremental(source, &mut state, &ranged());
        assert_eq!(blocks.len(), 1);
        assert_eq!(slice(source, &blocks[0]), Some(source));
    }
}
