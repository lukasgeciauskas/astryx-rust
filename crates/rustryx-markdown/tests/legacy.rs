//! Upstream `Markdown/parser.test.ts` (and the legacy-shape part of
//! `parser.public.test.ts`): the released `parseMarkdown` / `parseInline` projection
//! (`BlockNode` / `InlineNode`: `bold`, `italic`, `codeblock`, `hr`, … and `range`).
//! Upstream's `toEqual` on node trees compares their serde JSON; `toMatchObject` uses
//! [`assert_match`], vitest's subset rule. Offsets are UTF-8 bytes.
//!
//! Not ported here (another suite ports the incremental parser): the cases that call
//! `parseMarkdownIncremental` (`streams to the same result when chunks split the line
//! ending`, `reports absolute offsets when the document is parsed incrementally`,
//! `re-parses when the caller flips the option on an existing state`, `spans both halves
//! of a list the incremental parser merged`, and the incremental line of the public
//! `exposes legacy and canonical parser results …`).

use std::collections::BTreeSet;

use rustryx_markdown::ast::TableAlign;
use rustryx_markdown::{BlockNode, InlineNode, ParseOptions, parse_inline, parse_markdown};
use serde::Serialize;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn defaults() -> ParseOptions {
    ParseOptions::default()
}

fn math() -> ParseOptions {
    ParseOptions {
        math: true,
        ..ParseOptions::default()
    }
}

fn gfm() -> ParseOptions {
    ParseOptions {
        autolink_gfm: true,
        ..ParseOptions::default()
    }
}

fn ranges() -> ParseOptions {
    ParseOptions {
        source_ranges: true,
        ..ParseOptions::default()
    }
}

fn ids(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|id| id.to_string()).collect()
}

/// Upstream's `sourceIds` set of the citation suite.
fn citations() -> ParseOptions {
    ParseOptions {
        source_ids: Some(ids(&["abc1", "def2", "src3"])),
        ..ParseOptions::default()
    }
}

fn md(input: &str) -> Vec<BlockNode> {
    parse_markdown(input, &defaults())
}

fn inline(input: &str) -> Vec<InlineNode> {
    parse_inline(input, &defaults())
}

fn to_json<T: Serialize + ?Sized>(value: &T) -> Value {
    serde_json::to_value(value).expect("parser nodes serialize")
}

fn text(content: &str) -> InlineNode {
    InlineNode::Text {
        content: content.to_string(),
    }
}

fn inline_type(node: &InlineNode) -> String {
    to_json(node)["type"]
        .as_str()
        .expect("a node has a type")
        .to_string()
}

fn block_types(blocks: &[BlockNode]) -> Vec<&'static str> {
    blocks.iter().map(BlockNode::type_name).collect()
}

/// vitest's `toMatchObject`: every key `expected` names matches, recursively; arrays
/// match element-wise and must be the same length.
fn matches(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => expected
            .iter()
            .all(|(key, value)| actual.get(key).is_some_and(|actual| matches(actual, value))),
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| matches(actual, expected))
        }
        _ => actual == expected,
    }
}

#[track_caller]
fn assert_match<T: Serialize + ?Sized>(actual: &T, expected: Value) {
    let actual = to_json(actual);
    assert!(
        matches(&actual, &expected),
        "\n  actual:   {actual}\n  expected (subset): {expected}"
    );
}

#[track_caller]
fn assert_json<T: Serialize + ?Sized>(actual: &T, expected: Value) {
    assert_eq!(to_json(actual), expected);
}

fn has_break<T: Serialize + ?Sized>(value: &T) -> bool {
    serde_json::to_string(value)
        .expect("parser nodes serialize")
        .contains("\"type\":\"break\"")
}

fn children(block: &BlockNode) -> &[InlineNode] {
    match block {
        BlockNode::Heading { children, .. } | BlockNode::Paragraph { children, .. } => children,
        other => panic!("{} has no inline children", other.type_name()),
    }
}

fn list_items(block: &BlockNode) -> &[rustryx_markdown::ListItemNode] {
    match block {
        BlockNode::List { items, .. } => items,
        other => panic!("expected a list, got {}", other.type_name()),
    }
}

// ---------------------------------------------------------------------------
// describe('parseInline')
// ---------------------------------------------------------------------------

#[test]
fn parses_plain_text() {
    assert_eq!(inline("hello world"), vec![text("hello world")]);
}

#[test]
fn parses_bold_with_double_asterisks() {
    let result = inline("**bold text**");
    let InlineNode::Bold { children } = &result[0] else {
        panic!("expected bold, got {result:?}");
    };
    assert_eq!(children[0], text("bold text"));
}

#[test]
fn parses_italic_with_asterisk() {
    let result = inline("*italic text*");
    let InlineNode::Italic { children } = &result[0] else {
        panic!("expected italic, got {result:?}");
    };
    assert_eq!(children[0], text("italic text"));
}

#[test]
fn parses_inline_code() {
    assert_json(
        &inline("`const x`"),
        json!([{"type": "code", "content": "const x"}]),
    );
}

#[test]
fn keeps_backslash_escapes_literal_inside_standalone_inline_code() {
    assert_json(
        &inline("`T \\| null`"),
        json!([{"type": "code", "content": "T \\| null"}]),
    );
}

#[test]
fn leaves_math_delimiters_as_literal_text_by_default() {
    assert_eq!(
        inline("Euler: $e^{i * pi} + 1 = 0$."),
        vec![text("Euler: $e^{i * pi} + 1 = 0$.")]
    );
}

#[test]
fn parses_inline_math_before_markdown_formatting_when_explicitly_enabled() {
    assert_json(
        &parse_inline("Euler: $e^{i * pi} + 1 = 0$.", &math()),
        json!([
            {"type": "text", "content": "Euler: "},
            {"type": "math", "value": "e^{i * pi} + 1 = 0"},
            {"type": "text", "content": "."},
        ]),
    );
}

#[test]
fn preserves_escaped_delimiters_inside_math_and_leaves_escaped_openers_literal() {
    assert_json(
        &parse_inline("Price: \\$5; formula: $x \\$ y$.", &math()),
        json!([
            {"type": "text", "content": "Price: "},
            {"type": "text", "content": "$5; formula: "},
            {"type": "math", "value": "x \\$ y"},
            {"type": "text", "content": "."},
        ]),
    );
}

#[test]
fn leaves_an_unmatched_inline_math_delimiter_literal() {
    assert_eq!(
        parse_inline("The value is $x + 1.", &math()),
        vec![text("The value is $x + 1.")]
    );
}

#[test]
fn does_not_mistake_paired_currency_amounts_for_inline_math() {
    assert_eq!(
        parse_inline("Tickets cost $20 and $30 today.", &math()),
        vec![text("Tickets cost $20 and $30 today.")]
    );
    assert_json(
        &parse_inline("$x$5 and $y$", &math()),
        json!([
            {"type": "text", "content": "$x$5 and "},
            {"type": "math", "value": "y"},
        ]),
    );
}

#[test]
fn allows_an_inline_expression_to_begin_with_a_number() {
    assert_json(
        &parse_inline("Result: $2 + 2$.", &math()),
        json!([
            {"type": "text", "content": "Result: "},
            {"type": "math", "value": "2 + 2"},
            {"type": "text", "content": "."},
        ]),
    );
}

#[test]
fn does_not_treat_non_block_double_dollar_runs_as_inline_math() {
    assert_eq!(
        parse_inline("Keep $$x + y$$ literal.", &math()),
        vec![text("Keep $$x + y$$ literal.")]
    );
}

#[test]
fn keeps_code_and_link_destinations_opaque_while_parsing_math_in_link_labels() {
    assert_json(
        &parse_inline("`$code$` [$label$](/price/$5)", &math()),
        json!([
            {"type": "code", "content": "$code$"},
            {"type": "text", "content": " "},
            {
                "type": "link",
                "href": "/price/$5",
                "children": [{"type": "math", "value": "label"}],
            },
        ]),
    );
}

#[test]
fn parses_links() {
    let result = inline("[click](https://example.com)");
    let InlineNode::Link { href, children } = &result[0] else {
        panic!("expected a link, got {result:?}");
    };
    assert_eq!(href, "https://example.com");
    assert_eq!(children[0], text("click"));
}

#[test]
fn parses_images() {
    assert_json(
        &inline("![alt](img.png)"),
        json!([{"type": "image", "src": "img.png", "alt": "alt"}]),
    );
}

#[test]
fn rejects_javascript_links_as_plain_text_xss_prevention() {
    assert_eq!(
        inline("[click](javascript:alert(1))"),
        vec![text("[click](javascript:alert(1))")]
    );
}

#[test]
fn rejects_javascript_with_mixed_case_and_whitespace_xss_prevention() {
    assert_eq!(
        inline("[click](JaVaScRiPt:alert(1))"),
        vec![text("[click](JaVaScRiPt:alert(1))")]
    );
}

#[test]
fn rejects_vbscript_links_xss_prevention() {
    assert_eq!(
        inline("[click](vbscript:MsgBox(1))"),
        vec![text("[click](vbscript:MsgBox(1))")]
    );
}

#[test]
fn rejects_data_text_html_image_src_xss_prevention() {
    assert_eq!(
        inline("![xss](data:text/html,<script>alert(1)</script>)"),
        vec![text("![xss](data:text/html,<script>alert(1)</script>)")]
    );
}

#[test]
fn allows_normal_http_https_links() {
    let result = inline("[safe](https://example.com)");
    let InlineNode::Link { href, .. } = &result[0] else {
        panic!("expected a link, got {result:?}");
    };
    assert_eq!(href, "https://example.com");
}

#[test]
fn keeps_link_destinations_the_navigation_rule_accepts_except_data_urls() {
    let destinations = [
        ("https://example.com", true),
        ("/page", true),
        ("#section", true),
        ("mailto:a@example.com", true),
        ("tel:+1234567890", true),
        ("custom:document", true),
        ("data:image/png;base64,iVBORw0KGgo=", false),
        ("javascript:alert(1)", false),
        ("vbscript:MsgBox(1)", false),
        ("data:text/html,<b>x</b>", false),
        ("java\u{0}script:alert(1)", false),
    ];
    for (destination, accepted) in destinations {
        let nodes = inline(&format!("[t]({destination})"));
        assert_eq!(
            matches!(nodes[0], InlineNode::Link { .. }),
            accepted,
            "{destination:?}: {nodes:?}"
        );
    }
}

#[test]
fn parses_strikethrough() {
    let result = inline("~~deleted~~");
    let InlineNode::Strikethrough { children } = &result[0] else {
        panic!("expected strikethrough, got {result:?}");
    };
    assert_eq!(children[0], text("deleted"));
}

#[test]
fn parses_mixed_inline_formatting() {
    let result = inline("Hello **bold** and *italic* with `code` and [link](url)");
    assert!(result.len() >= 5, "{result:?}");
}

#[test]
fn handles_escape_sequences() {
    let result = inline("\\*not italic\\*");
    assert!(
        !result
            .iter()
            .any(|node| matches!(node, InlineNode::Italic { .. })),
        "{result:?}"
    );
}

#[test]
fn parses_bold_italic_with_asterisks() {
    let result = inline("***bold italic***");
    assert_eq!(result.len(), 1, "{result:?}");
    let InlineNode::Bold { children } = &result[0] else {
        panic!("expected bold, got {result:?}");
    };
    assert_eq!(children.len(), 1);
    let InlineNode::Italic { children } = &children[0] else {
        panic!("expected italic inside bold, got {result:?}");
    };
    assert_eq!(children[0], text("bold italic"));
}

#[test]
fn parses_bold_italic_with_underscores() {
    let result = inline("___bold italic___");
    assert_eq!(result.len(), 1, "{result:?}");
    let InlineNode::Bold { children } = &result[0] else {
        panic!("expected bold, got {result:?}");
    };
    assert_eq!(inline_type(&children[0]), "italic");
}

#[test]
fn does_not_italicize_underscores_inside_words() {
    assert_eq!(
        inline("some_variable_name"),
        vec![text("some_variable_name")]
    );
}

#[test]
fn does_not_bold_underscores_inside_words() {
    assert_eq!(inline("foo__bar__baz"), vec![text("foo__bar__baz")]);
}

#[test]
fn italicizes_underscores_at_word_boundaries() {
    let result = inline("hello _world_ end");
    let italic = result
        .iter()
        .find_map(|node| match node {
            InlineNode::Italic { children } => Some(children),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no italic in {result:?}"));
    assert_eq!(italic[0], text("world"));
}

#[test]
fn asterisk_italic_works_mid_word() {
    let result = inline("some*thing*here");
    assert!(
        result
            .iter()
            .any(|node| matches!(node, InlineNode::Italic { .. })),
        "{result:?}"
    );
}

#[test]
fn handles_parentheses_in_link_urls() {
    let result = inline("[text](https://example.com/wiki/Foo_(bar))");
    let InlineNode::Link { href, .. } = &result[0] else {
        panic!("expected a link, got {result:?}");
    };
    assert_eq!(href, "https://example.com/wiki/Foo_(bar)");
}

#[test]
fn keeps_a_link_title_out_of_the_destination() {
    let cases = [
        (
            r#"[t](https://example.com/notes "Notes")"#,
            "https://example.com/notes",
        ),
        (
            "[t](https://example.com/notes 'Notes')",
            "https://example.com/notes",
        ),
        (
            "[t](https://example.com/notes (Notes))",
            "https://example.com/notes",
        ),
        (
            r#"[t](https://example.com "Say \"hi\"")"#,
            "https://example.com",
        ),
        (
            r#"[t](<https://example.com/a b> "Spaced")"#,
            "https://example.com/a b",
        ),
        (
            r#"[t](https://example.com/wiki/Foo_(bar) "Wiki")"#,
            "https://example.com/wiki/Foo_(bar)",
        ),
    ];
    for (source, href) in cases {
        let nodes = inline(source);
        assert!(
            matches(&to_json(&nodes[0]), &json!({"type": "link", "href": href})),
            "{source}: {nodes:?}"
        );
    }
}

#[test]
fn keeps_an_image_title_out_of_the_source_inline_and_standalone() {
    assert_json(
        &inline(r#"![alt](img.png "Caption")"#),
        json!([{"type": "image", "src": "img.png", "alt": "alt"}]),
    );
    assert_match(
        &md(r#"![alt](img.png "Caption")"#)[0],
        json!({"type": "image", "src": "img.png"}),
    );
}

#[test]
fn keeps_link_content_that_has_no_title_shape_as_the_destination() {
    assert_match(
        &inline("[t](https://example.com/a b)")[0],
        json!({"type": "link", "href": "https://example.com/a b"}),
    );
}

#[test]
fn decodes_named_and_numeric_character_references_in_text() {
    assert_eq!(
        inline("Fish &amp; chips &copy; &#169; &#x1F600; a&nbsp;b"),
        vec![text("Fish & chips \u{a9} \u{a9} \u{1F600} a\u{a0}b")]
    );
}

#[test]
fn keeps_unknown_names_missing_semicolons_and_escaped_ampersands_literal() {
    let nodes = inline("&notareference; &amp no semicolon \\&amp;");
    assert!(
        nodes
            .iter()
            .all(|node| matches!(node, InlineNode::Text { .. })),
        "{nodes:?}"
    );
    let joined: String = nodes
        .iter()
        .map(|node| match node {
            InlineNode::Text { content } => content.as_str(),
            _ => "",
        })
        .collect();
    assert_eq!(joined, "&notareference; &amp no semicolon &amp;");
}

#[test]
fn replaces_nul_surrogate_and_out_of_range_numeric_references() {
    assert_eq!(
        inline("&#0; &#xD800; &#1114112;"),
        vec![text("\u{FFFD} \u{FFFD} \u{FFFD}")]
    );
}

#[test]
fn never_turns_a_decoded_character_into_markup() {
    assert_eq!(
        inline("&ast;not emphasis&ast; &lsqb;not a link&rsqb;(x)"),
        vec![text("*not emphasis* [not a link](x)")]
    );
}

#[test]
fn keeps_character_references_literal_in_inline_and_fenced_code() {
    assert_json(
        &inline("`&amp;`"),
        json!([{"type": "code", "content": "&amp;"}]),
    );
    assert_match(
        &md("```\n&copy; &#169;\n```")[0],
        json!({"type": "codeblock", "content": "&copy; &#169;"}),
    );
}

#[test]
fn decodes_character_references_and_escapes_in_image_alt_text() {
    assert_json(
        &inline("![Fish &amp; chips \\*fresh\\*](fish.png)"),
        json!([{"type": "image", "src": "fish.png", "alt": "Fish & chips *fresh*"}]),
    );
    assert_match(
        &md("![&copy; 2026 &#169;](logo.png)")[0],
        json!({"type": "image", "alt": "\u{a9} 2026 \u{a9}"}),
    );
    assert_match(
        &md("![Q&amp;A][logo]\n\n[logo]: /logo.png")[0],
        json!({
            "type": "paragraph",
            "children": [{"type": "image", "src": "/logo.png", "alt": "Q&A"}],
        }),
    );
}

#[test]
fn keeps_an_escaped_ampersand_and_unknown_names_literal_in_alt_text() {
    assert_json(
        &inline("![\\&amp; &nope;](x.png)"),
        json!([{"type": "image", "src": "x.png", "alt": "&amp; &nope;"}]),
    );
}

#[test]
fn makes_two_spaces_or_a_backslash_before_a_crlf_line_ending_a_hard_break() {
    assert_eq!(
        inline("two  \r\nspaces"),
        vec![text("two"), InlineNode::Break, text("spaces")]
    );
    assert_eq!(
        inline("back\\\r\nslash"),
        vec![text("back"), InlineNode::Break, text("slash")]
    );
    // A Windows path ending a line keeps its other backslashes.
    assert_eq!(
        inline("C:\\Users\\Ada\\\r\nnext"),
        vec![text("C:\\Users\\Ada"), InlineNode::Break, text("next")]
    );
}

#[test]
fn escapes_only_ascii_punctuation_and_keeps_other_backslashes_literal() {
    let text_of = |source: &str| -> String {
        inline(source)
            .iter()
            .map(|node| match node {
                InlineNode::Text { content } => content.clone(),
                other => format!("<{}>", inline_type(other)),
            })
            .collect()
    };
    assert_eq!(text_of("C:\\Users\\Ada"), "C:\\Users\\Ada");
    assert_eq!(
        text_of("\\*not emphasis\\* and \\\\ one backslash"),
        "*not emphasis* and \\ one backslash"
    );
    assert_eq!(text_of("caf\\\u{e9} \\a \\1"), "caf\\\u{e9} \\a \\1");
}

#[test]
fn makes_a_backslash_before_a_line_break_a_hard_break() {
    assert_eq!(
        inline("line\\\nnext"),
        vec![text("line"), InlineNode::Break, text("next")]
    );
}

#[test]
fn keeps_non_punctuation_backslashes_in_image_alt_text() {
    assert_json(
        &inline("![C:\\Users \\& more](x.png)"),
        json!([{"type": "image", "src": "x.png", "alt": "C:\\Users & more"}]),
    );
}

#[test]
fn handles_parentheses_in_image_urls() {
    let result = inline("![alt](https://example.com/img_(1).png)");
    let InlineNode::Image { src, .. } = &result[0] else {
        panic!("expected an image, got {result:?}");
    };
    assert_eq!(src, "https://example.com/img_(1).png");
}

#[test]
fn detects_line_break_from_two_trailing_spaces_before_newline() {
    let result = inline("hello  \nworld");
    assert!(result.contains(&InlineNode::Break));
    assert_eq!(
        result,
        vec![text("hello"), InlineNode::Break, text("world")]
    );
}

#[test]
fn does_not_produce_break_with_only_one_trailing_space() {
    let result = inline("hello \nworld");
    assert!(!result.contains(&InlineNode::Break), "{result:?}");
}

// ---------------------------------------------------------------------------
// describe('parseMarkdown') > describe('hard breaks in CRLF documents')
// ---------------------------------------------------------------------------

fn lf(source: &str) -> String {
    source.replace("\r\n", "\n")
}

fn is_empty_text(node: &Value) -> bool {
    node.get("type").and_then(Value::as_str) == Some("text")
        && node.get("content").and_then(Value::as_str) == Some("")
}

/// Upstream `withoutCarriageReturns`: drop every `\r` from strings, and any text node
/// left empty, to compare structure with the LF document.
fn without_carriage_returns(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(without_carriage_returns)
                .filter(|node| !is_empty_text(node))
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, entry)| (key.clone(), without_carriage_returns(entry)))
                .collect(),
        ),
        Value::String(string) => Value::String(string.replace('\r', "")),
        other => other.clone(),
    }
}

#[test]
fn breaks_lines_the_same_way_as_the_lf_document() {
    for source in [
        "Line one  \r\nLine two\r\n",
        "C:\\Users\\Ada\\\r\nnext\r\n",
        "[two  \r\nlines](https://example.com)\r\n",
        "> quoted  \r\n> lines\r\n",
    ] {
        let crlf = md(source);
        assert!(has_break(&crlf), "{source:?}: {crlf:?}");
        assert_eq!(
            without_carriage_returns(&to_json(&crlf)),
            to_json(&md(&lf(source))),
            "{source:?}"
        );
    }
}

#[test]
fn leaves_code_spans_code_blocks_and_table_cells_without_breaks() {
    for source in [
        "`a  \r\nb` and `c\\\r\nd`\r\n",
        "```\r\ncode  \r\nmore\\\r\n```\r\n",
        "| a |\r\n| --- |\r\n| x  |\r\n",
    ] {
        let blocks = md(source);
        assert!(!has_break(&blocks), "{source:?}: {blocks:?}");
    }
}

// ---------------------------------------------------------------------------
// describe('parseMarkdown')
// ---------------------------------------------------------------------------

#[test]
fn parses_headings() {
    let result = md("# Hello");
    let BlockNode::Heading { level, .. } = &result[0] else {
        panic!("expected a heading, got {result:?}");
    };
    assert_eq!(*level, 1);
}

#[test]
fn parses_h1_h6() {
    let result = md("# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6");
    assert_eq!(result.len(), 6);
    for (i, block) in result.iter().enumerate() {
        let BlockNode::Heading { level, .. } = block else {
            panic!("expected a heading, got {block:?}");
        };
        assert_eq!(usize::from(*level), i + 1);
    }
}

#[test]
fn parses_paragraphs() {
    assert_eq!(md("Hello world")[0].type_name(), "paragraph");
}

#[test]
fn separates_paragraphs_by_blank_lines() {
    let result = md("First paragraph\n\nSecond paragraph");
    assert_eq!(
        result
            .iter()
            .filter(|block| block.type_name() == "paragraph")
            .count(),
        2
    );
}

#[test]
fn parses_fenced_code_blocks_with_backticks() {
    let result = md("```javascript\nconst x = 1;\nconsole.log(x);\n```");
    let BlockNode::Codeblock {
        language, content, ..
    } = &result[0]
    else {
        panic!("expected a codeblock, got {result:?}");
    };
    assert_eq!(language, "javascript");
    assert!(content.contains("const x = 1;"));
}

#[test]
fn parses_code_blocks_with_tilde_fences() {
    let result = md("~~~python\nprint(\"hello\")\n~~~");
    let BlockNode::Codeblock { language, .. } = &result[0] else {
        panic!("expected a codeblock, got {result:?}");
    };
    assert_eq!(language, "python");
}

#[test]
fn parses_display_math_only_when_explicitly_enabled() {
    let source = "$$\n\\int_0^1 x^2 \\, dx\n$$";
    let unexpected = json!({"type": "math", "value": "\\int_0^1 x^2 \\, dx"});
    assert!(
        !md(source).iter().any(|block| to_json(block) == unexpected),
        "{:?}",
        md(source)
    );
    assert_json(
        &parse_markdown(source, &math()),
        json!([{"type": "math", "value": "\\int_0^1 x^2 \\, dx"}]),
    );
}

#[test]
fn parses_a_same_line_display_math_block() {
    assert_json(
        &parse_markdown("$$E = mc^2$$", &math()),
        json!([{"type": "math", "value": "E = mc^2"}]),
    );
}

#[test]
fn keeps_empty_display_delimiters_literal() {
    let source = "$$\n$$";
    assert_eq!(parse_markdown(source, &math()), md(source));
}

#[test]
fn leaves_unmatched_display_math_delimiters_literal() {
    let source = "$$\nx + y";
    assert_eq!(parse_markdown(source, &math()), md(source));
}

#[test]
fn leaves_escaped_display_delimiters_literal() {
    let blocks = parse_markdown("\\$\\$\nx + y\n\\$\\$", &math());
    assert!(
        !blocks
            .iter()
            .any(|block| matches!(block, BlockNode::Math { .. })),
        "{blocks:?}"
    );
}

#[test]
fn reports_a_display_math_source_range_including_its_delimiters() {
    let source = "Before.\n\n$$\nx + y\n$$\n\nAfter.";
    let blocks = parse_markdown(
        source,
        &ParseOptions {
            math: true,
            source_ranges: true,
            ..ParseOptions::default()
        },
    );
    let range = blocks[1].range().expect("the math block has a range");
    assert_json(&range, json!({"start": 9, "end": 20}));
    assert_eq!(&source[range.start..range.end], "$$\nx + y\n$$");
}

#[test]
fn keeps_fenced_code_opaque_when_math_parsing_is_enabled() {
    assert_json(
        &parse_markdown("```tex\n$x$\n$$y$$\n```", &math()),
        json!([{"type": "codeblock", "language": "tex", "content": "$x$\n$$y$$"}]),
    );
}

#[test]
fn does_not_collect_link_definitions_from_display_math() {
    let blocks = parse_markdown("$$\n[x]: /not-a-link\n$$\n\n[x]", &math());
    assert_json(
        &blocks[0],
        json!({"type": "math", "value": "[x]: /not-a-link"}),
    );
    assert!(
        !serde_json::to_string(&blocks[1])
            .unwrap()
            .contains("\"type\":\"link\""),
        "{blocks:?}"
    );
}

#[test]
fn parses_blockquotes() {
    assert_eq!(md("> This is a quote")[0].type_name(), "blockquote");
}

#[test]
fn keeps_lazy_paragraph_continuations_inside_blockquotes_and_list_items() {
    assert_json(
        &md("> quoted\ncontinued lazily"),
        json!([{
            "type": "blockquote",
            "children": [{
                "type": "paragraph",
                "children": [{"type": "text", "content": "quoted\ncontinued lazily"}],
            }],
        }]),
    );

    for marker in ["-", "1.", "- [ ]"] {
        let blocks = md(&format!("{marker} listed\ncontinued lazily"));
        assert_match(
            &blocks[0],
            json!({
                "type": "list",
                "items": [{
                    "children": [{
                        "type": "paragraph",
                        "children": [{"type": "text", "content": "listed\ncontinued lazily"}],
                    }],
                }],
            }),
        );
    }
    assert_match(
        &md("> # Foo\n> bar\nbaz"),
        json!([{
            "type": "blockquote",
            "children": [
                {"type": "heading", "level": 1},
                {"type": "paragraph", "children": [{"type": "text", "content": "bar\nbaz"}]},
            ],
        }]),
    );
    assert_match(
        &md("> bar\nbaz\n> foo"),
        json!([{
            "type": "blockquote",
            "children": [
                {"type": "paragraph", "children": [{"type": "text", "content": "bar\nbaz\nfoo"}]},
            ],
        }]),
    );
}

#[test]
fn keeps_lazy_continuations_in_the_deepest_open_paragraph() {
    assert_match(
        &md("> 1. > Blockquote\ncontinued here."),
        json!([{
            "type": "blockquote",
            "children": [{
                "type": "list",
                "items": [{
                    "children": [{
                        "type": "blockquote",
                        "children": [{
                            "type": "paragraph",
                            "children": [{"type": "text", "content": "Blockquote\ncontinued here."}],
                        }],
                    }],
                }],
            }],
        }]),
    );

    assert_match(
        &md("- Outer item\n  > Nested quote\ncontinued here."),
        json!([{
            "type": "list",
            "items": [{
                "children": [
                    {"type": "paragraph"},
                    {
                        "type": "blockquote",
                        "children": [{
                            "type": "paragraph",
                            "children": [{"type": "text", "content": "Nested quote\ncontinued here."}],
                        }],
                    },
                ],
            }],
        }]),
    );
}

#[test]
fn resumes_lazy_continuation_for_a_later_blockquote_paragraph() {
    assert_match(
        &md("> para1\n>\n> para2\nlazy"),
        json!([{
            "type": "blockquote",
            "children": [
                {"type": "paragraph", "children": [{"type": "text", "content": "para1"}]},
                {"type": "paragraph", "children": [{"type": "text", "content": "para2\nlazy"}]},
            ],
        }]),
    );

    assert_match(
        &md("> > para1\n> >\n> > para2\nlazy"),
        json!([{
            "type": "blockquote",
            "children": [{
                "type": "blockquote",
                "children": [
                    {"type": "paragraph", "children": [{"type": "text", "content": "para1"}]},
                    {"type": "paragraph", "children": [{"type": "text", "content": "para2\nlazy"}]},
                ],
            }],
        }]),
    );
}

#[test]
fn does_not_continue_a_container_paragraph_across_a_blank_or_block_start() {
    assert_eq!(
        block_types(&md("> quoted\n\noutside")),
        ["blockquote", "paragraph"]
    );
    assert_eq!(
        block_types(&md("- listed\n\noutside")),
        ["list", "paragraph"]
    );
    assert_eq!(
        block_types(&md("> quoted\n# Outside")),
        ["blockquote", "heading"]
    );
    assert_match(
        &md("- listed\n```\noutside\n```"),
        json!([{"type": "list"}, {"type": "codeblock", "content": "outside"}]),
    );
}

#[test]
fn parses_horizontal_rules() {
    assert_eq!(md("---")[0].type_name(), "hr");
}

#[test]
fn parses_unordered_lists() {
    let result = md("- Item 1\n- Item 2\n- Item 3");
    let BlockNode::List { ordered, items, .. } = &result[0] else {
        panic!("expected a list, got {result:?}");
    };
    assert!(!ordered);
    assert_eq!(items.len(), 3);
}

#[test]
fn parses_ordered_lists() {
    let result = md("1. First\n2. Second");
    let BlockNode::List { ordered, items, .. } = &result[0] else {
        panic!("expected a list, got {result:?}");
    };
    assert!(ordered);
    assert_eq!(items.len(), 2);
}

#[test]
fn parses_an_ordered_list_with_a_custom_start_number() {
    let result = md("5. five\n6. six\n7. seven");
    let BlockNode::List {
        ordered,
        start,
        items,
        ..
    } = &result[0]
    else {
        panic!("expected a list, got {result:?}");
    };
    assert!(ordered);
    assert_eq!(*start, Some(5));
    assert_eq!(items.len(), 3);
}

#[test]
fn parses_an_ordered_list_using_the_paren_delimiter_commonmark_5_2() {
    let result = md("1) First\n2) Second\n3) Third");
    assert_eq!(result.len(), 1);
    let BlockNode::List {
        ordered,
        start,
        items,
        ..
    } = &result[0]
    else {
        panic!("expected a list, got {result:?}");
    };
    assert!(ordered);
    assert_eq!(*start, Some(1));
    assert_eq!(items.len(), 3);
}

#[test]
fn parses_a_paren_ordered_list_with_a_custom_start_number() {
    let result = md("3) three\n4) four");
    let BlockNode::List {
        ordered,
        start,
        items,
        ..
    } = &result[0]
    else {
        panic!("expected a list, got {result:?}");
    };
    assert!(ordered);
    assert_eq!(*start, Some(3));
    assert_eq!(items.len(), 2);
}

#[test]
fn starts_a_new_list_when_the_ordered_delimiter_changes() {
    // CommonMark: changing the marker delimiter (. -> )) begins a new list.
    let result = md("1. First\n2) Second");
    let lists: Vec<&BlockNode> = result
        .iter()
        .filter(|block| block.type_name() == "list")
        .collect();
    assert_eq!(lists.len(), 2, "{result:?}");
    assert_eq!(list_items(lists[0]).len(), 1);
    assert_eq!(list_items(lists[1]).len(), 1);
    let BlockNode::List { start, .. } = lists[1] else {
        unreachable!()
    };
    assert_eq!(*start, Some(2));
}

#[test]
fn only_lets_a_marker_value_of_1_interrupt_a_paragraph() {
    // CommonMark 5.2: an ordered list interrupts a paragraph only when it starts at 1
    // (for either delimiter). A `2)`/`2.` line is lazy paragraph text.
    let two = md("Intro\n2) second step");
    assert_eq!(two.len(), 1);
    assert_eq!(two[0].type_name(), "paragraph");

    let one = md("Intro\n1) first step");
    assert_eq!(block_types(&one), ["paragraph", "list"]);
}

#[test]
fn treats_a_zero_padded_value_1_marker_as_interrupting_a_paragraph() {
    // The interruption rule is by numeric value, so `01.` / `001)` (value 1) still start
    // a list, while `0)` (value 0) does not.
    assert_eq!(block_types(&md("Intro\n01. first")), ["paragraph", "list"]);
    assert_eq!(block_types(&md("Intro\n001) first")), ["paragraph", "list"]);
    assert_eq!(md("Intro\n0) zero").len(), 1);
}

#[test]
fn joins_blank_line_separated_same_style_items_into_one_loose_list() {
    // CommonMark §5.3 loose-list: blank lines between items of the same style/indent
    // still form a single list.
    let result = md("1. apple\n\n1. banana\n\n1. cherry");
    assert_eq!(result.len(), 1);
    let BlockNode::List {
        ordered,
        loose,
        items,
        start,
        ..
    } = &result[0]
    else {
        panic!("expected a list, got {result:?}");
    };
    assert!(ordered);
    assert_eq!(*loose, Some(true));
    assert_eq!(items.len(), 3);
    assert_eq!(*start, Some(1));
}

#[test]
fn joins_blank_line_separated_unordered_items_into_one_loose_list() {
    let result = md("- a\n\n- b\n\n- c");
    assert_eq!(result.len(), 1);
    let BlockNode::List {
        ordered,
        loose,
        items,
        ..
    } = &result[0]
    else {
        panic!("expected a list, got {result:?}");
    };
    assert!(!ordered);
    assert_eq!(*loose, Some(true));
    assert_eq!(items.len(), 3);
}

#[test]
fn does_not_join_lists_of_different_styles_across_a_blank_line() {
    let result = md("- a\n\n1. b");
    assert_eq!(result.len(), 2);
    let (
        BlockNode::List { ordered: first, .. },
        BlockNode::List {
            ordered: second, ..
        },
    ) = (&result[0], &result[1])
    else {
        panic!("expected two lists, got {result:?}");
    };
    assert!(!first);
    assert!(second);
}

#[test]
fn treats_a_tight_list_with_no_blank_lines_as_not_loose() {
    let result = md("1. a\n2. b\n3. c");
    let BlockNode::List { loose, .. } = &result[0] else {
        panic!("expected a list, got {result:?}");
    };
    assert_eq!(*loose, None);
}

#[test]
fn parses_task_lists() {
    let result = md("- [ ] Unchecked\n- [x] Checked");
    let items = list_items(&result[0]);
    assert_eq!(items[0].checked, Some(false));
    assert_eq!(items[1].checked, Some(true));
}

#[test]
fn parses_gfm_tables() {
    let result = md("| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |");
    let BlockNode::Table { headers, rows, .. } = &result[0] else {
        panic!("expected a table, got {result:?}");
    };
    assert_eq!(headers.len(), 2);
    assert_eq!(rows.len(), 2);
}

#[test]
fn parses_table_alignment() {
    let result = md("| Left | Center | Right |\n| :--- | :---: | ---: |\n| a | b | c |");
    let BlockNode::Table { alignments, .. } = &result[0] else {
        panic!("expected a table, got {result:?}");
    };
    assert_eq!(
        alignments,
        &[
            Some(TableAlign::Left),
            Some(TableAlign::Center),
            Some(TableAlign::Right)
        ]
    );
}

#[test]
fn parses_standalone_images() {
    let result = md("![alt text](image.png)");
    let BlockNode::Image { src, alt, .. } = &result[0] else {
        panic!("expected an image, got {result:?}");
    };
    assert_eq!(src, "image.png");
    assert_eq!(alt, "alt text");
}

#[test]
fn rejects_a_standalone_image_with_an_unsafe_scheme_as_literal_text_xss_prevention() {
    let result = md("![alt](vbscript:msgbox)");
    assert_eq!(result[0].type_name(), "paragraph");
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("\"type\":\"image\""),
        "{result:?}"
    );
}

#[test]
fn parses_complex_ai_response() {
    let input = [
        "# Analysis",
        "",
        "Here is the result of the analysis.",
        "",
        "```typescript",
        "const result = analyze(data);",
        "```",
        "",
        "- Point one",
        "- Point two",
        "- Point three",
        "",
        "> Important note about the findings.",
        "",
        "| Metric | Value |",
        "| --- | --- |",
        "| Score | 95 |",
    ]
    .join("\n");
    let types = block_types(&md(&input));
    for expected in [
        "heading",
        "paragraph",
        "codeblock",
        "list",
        "blockquote",
        "table",
    ] {
        assert!(
            types.contains(&expected),
            "{expected} missing from {types:?}"
        );
    }
}

#[test]
fn parses_nested_unordered_lists() {
    let result = md("- Item 1\n  - Nested 1a\n  - Nested 1b\n- Item 2");
    let items = list_items(&result[0]);
    assert_eq!(items.len(), 2);
    // Item 1 should have children that include a nested list.
    let nested = items[0]
        .children
        .iter()
        .find(|child| child.type_name() == "list")
        .unwrap_or_else(|| panic!("no nested list in {result:?}"));
    assert_eq!(list_items(nested).len(), 2);
}

#[test]
fn parses_deeply_nested_lists() {
    let result = md("- L1\n  - L2\n    - L3");
    let item = &list_items(&result[0])[0];
    let l2 = item
        .children
        .iter()
        .find(|child| child.type_name() == "list")
        .unwrap_or_else(|| panic!("no L2 in {result:?}"));
    assert!(
        list_items(l2)[0]
            .children
            .iter()
            .any(|child| child.type_name() == "list"),
        "no L3 in {result:?}"
    );
}

#[test]
fn parses_table_without_leading_pipe() {
    let result = md("Name | Age\n--- | ---\nAlice | 30\nBob | 25");
    let BlockNode::Table { headers, rows, .. } = &result[0] else {
        panic!("expected a table, got {result:?}");
    };
    assert_eq!(headers.len(), 2);
    assert_eq!(rows.len(), 2);
}

#[test]
fn decodes_escaped_pipes_in_table_code_spans_without_changing_prose_nodes() {
    let input = "| Concept | TypeScript |\n| --- | --- |\n| Prose | A \\| B |\n| Null safety | `T \\| null` |\n| Union | `A \\| B \\| C` |";
    let result = md(input);
    let BlockNode::Table { headers, rows, .. } = &result[0] else {
        panic!("expected a table, got {result:?}");
    };
    assert_eq!(headers.len(), 2);
    assert_eq!(rows.len(), 3);
    assert_json(
        &rows[0][1].children,
        json!([
            {"type": "text", "content": "A "},
            {"type": "text", "content": "| B"},
        ]),
    );
    assert_json(
        &rows[1][1].children,
        json!([{"type": "code", "content": "T | null"}]),
    );
    assert_json(
        &rows[2][1].children,
        json!([{"type": "code", "content": "A | B | C"}]),
    );
}

#[test]
fn parses_spaced_hr_dashes() {
    assert_eq!(md("- - -")[0].type_name(), "hr");
}

#[test]
fn parses_spaced_hr_asterisks() {
    assert_eq!(md("* * *")[0].type_name(), "hr");
}

#[test]
fn parses_spaced_hr_underscores() {
    assert_eq!(md("_ _ _")[0].type_name(), "hr");
}

// ---------------------------------------------------------------------------
// describe('citation parsing')
// ---------------------------------------------------------------------------

// describe('bracket notation [id]')

#[test]
fn parses_a_bracket_citation_when_source_id_matches() {
    assert_json(
        &parse_inline("Tokyo is large[abc1].", &citations()),
        json!([
            {"type": "text", "content": "Tokyo is large"},
            {"type": "citation", "sourceId": "abc1"},
            {"type": "text", "content": "."},
        ]),
    );
}

#[test]
fn parses_multiple_bracket_citations() {
    assert_json(
        &parse_inline("Fact[abc1][def2].", &citations()),
        json!([
            {"type": "text", "content": "Fact"},
            {"type": "citation", "sourceId": "abc1"},
            {"type": "citation", "sourceId": "def2"},
            {"type": "text", "content": "."},
        ]),
    );
}

fn has_citation(nodes: &[InlineNode]) -> bool {
    nodes
        .iter()
        .any(|node| matches!(node, InlineNode::Citation { .. }))
}

#[test]
fn does_not_treat_id_as_citation_when_id_is_not_in_source_ids() {
    let result = parse_inline("Use [PID] here.", &citations());
    assert!(!has_citation(&result), "{result:?}");
}

#[test]
fn does_not_treat_text_url_as_citation() {
    assert_json(
        &parse_inline("[click here](https://example.com)", &citations()),
        json!([{
            "type": "link",
            "href": "https://example.com",
            "children": [{"type": "text", "content": "click here"}],
        }]),
    );
}

#[test]
fn handles_citation_next_to_link() {
    let result = parse_inline("See [link](https://example.com)[abc1].", &citations());
    assert_json(&result[0], json!({"type": "text", "content": "See "}));
    assert_json(
        &result[1],
        json!({
            "type": "link",
            "href": "https://example.com",
            "children": [{"type": "text", "content": "link"}],
        }),
    );
    assert_json(&result[2], json!({"type": "citation", "sourceId": "abc1"}));
}

#[test]
fn ignores_brackets_when_no_source_ids_provided() {
    let result = inline("Text [abc1] here.");
    assert!(!has_citation(&result), "{result:?}");
}

// describe('fullwidth bracket notation 【id】')

#[test]
fn parses_a_fullwidth_bracket_citation() {
    assert_json(
        &parse_inline("Data【abc1】.", &citations()),
        json!([
            {"type": "text", "content": "Data"},
            {"type": "citation", "sourceId": "abc1"},
            {"type": "text", "content": "."},
        ]),
    );
}

#[test]
fn parses_consecutive_fullwidth_citations() {
    assert_json(
        &parse_inline("Info【abc1】【def2】end.", &citations()),
        json!([
            {"type": "text", "content": "Info"},
            {"type": "citation", "sourceId": "abc1"},
            {"type": "citation", "sourceId": "def2"},
            {"type": "text", "content": "end."},
        ]),
    );
}

#[test]
fn preserves_fullwidth_brackets_when_id_is_not_in_source_ids() {
    let result = parse_inline("Keep【hello world】.", &citations());
    assert!(!has_citation(&result), "{result:?}");
}

// describe('mixed formats')

#[test]
fn handles_both_bracket_and_fullwidth_in_same_text() {
    assert_json(
        &parse_inline("A[abc1] B【def2】.", &citations()),
        json!([
            {"type": "text", "content": "A"},
            {"type": "citation", "sourceId": "abc1"},
            {"type": "text", "content": " B"},
            {"type": "citation", "sourceId": "def2"},
            {"type": "text", "content": "."},
        ]),
    );
}

// describe('block-level threading')

#[test]
fn detects_citations_inside_paragraphs() {
    let blocks = parse_markdown("Tokyo is big[abc1].", &citations());
    assert_eq!(blocks.len(), 1);
    if let BlockNode::Paragraph { children, .. } = &blocks[0] {
        assert!(has_citation(children), "{blocks:?}");
    }
}

#[test]
fn detects_citations_inside_list_items() {
    let blocks = parse_markdown("- Item[abc1]\n- Item[def2]", &citations());
    assert_eq!(blocks.len(), 1);
    if let BlockNode::List { items, .. } = &blocks[0]
        && let BlockNode::Paragraph { children, .. } = &items[0].children[0]
    {
        assert!(has_citation(children), "{blocks:?}");
    }
}

#[test]
fn detects_citations_inside_table_cells() {
    let blocks = parse_markdown("| A | B |\n|---|---|\n| data[abc1] | ok |", &citations());
    assert_eq!(blocks.len(), 1);
    if let BlockNode::Table { rows, .. } = &blocks[0] {
        assert!(has_citation(&rows[0][0].children), "{blocks:?}");
    }
}

// describe('autolink (gfm)')

fn link_json(href: &str, label: &str) -> Value {
    json!({"type": "link", "href": href, "children": [{"type": "text", "content": label}]})
}

#[test]
fn is_opt_in_bare_urls_stay_literal_by_default() {
    assert_eq!(
        inline("see https://example.com for info"),
        vec![text("see https://example.com for info")]
    );
}

#[test]
fn links_a_bare_https_url_when_enabled() {
    assert_json(
        &parse_inline("see https://example.com for info", &gfm()),
        json!([
            {"type": "text", "content": "see "},
            link_json("https://example.com", "https://example.com"),
            {"type": "text", "content": " for info"},
        ]),
    );
}

#[test]
fn links_a_bare_http_url() {
    let result = parse_inline("go to http://example.com", &gfm());
    assert_json(
        &result[1],
        link_json("http://example.com", "http://example.com"),
    );
}

#[test]
fn links_a_bare_www_url_with_http_prefix_on_href() {
    let result = parse_inline("go www.example.com", &gfm());
    assert_json(
        &result[1],
        link_json("http://www.example.com", "www.example.com"),
    );
}

#[test]
fn links_a_bare_email() {
    let result = parse_inline("ping user@example.com please", &gfm());
    assert_json(
        &result[1],
        link_json("mailto:user@example.com", "user@example.com"),
    );
}

#[test]
fn allows_plus_and_percent_in_the_email_local_part() {
    let result = parse_inline("mail user+tag%suffix@example.com here", &gfm());
    let InlineNode::Link { href, .. } = &result[1] else {
        panic!("expected a link, got {result:?}");
    };
    assert_eq!(href, "mailto:user+tag%suffix@example.com");
}

#[test]
fn finds_the_same_bare_emails_as_a_scan_from_every_character() {
    // The reference is the unanchored pattern the scan from each `@` replaced, with the
    // same boundary checks.
    let pattern = regex::Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+")
        .expect("a valid pattern");
    let reference_emails = |text: &str| -> Vec<String> {
        let mut found = Vec::new();
        for m in pattern.find_iter(text) {
            let prev = text[..m.start()].chars().next_back();
            if prev.is_some_and(|prev| prev.is_ascii_alphanumeric() || "._%+-@".contains(prev)) {
                continue;
            }
            if text[m.start().saturating_sub(7)..m.start()] == *"mailto:" {
                continue;
            }
            found.push(m.as_str().to_string());
        }
        found
    };
    let linked_emails = |text: &str| -> Vec<String> {
        parse_inline(text, &gfm())
            .into_iter()
            .filter_map(|node| match node {
                InlineNode::Link { href, .. } => href.strip_prefix("mailto:").map(String::from),
                _ => None,
            })
            .collect()
    };

    let mut cases: Vec<String> = [
        "a@b.c",
        "x@a.b+c@d.e",
        "name@x.y@host.com",
        "a@b@c.d",
        "mailto:user@host.com and user@host.com",
        "ab@cd ef@gh.ij.",
        "..a@b.c..d@e.f",
        "@a.b x@ y@.z q@r.s",
    ]
    .map(String::from)
    .to_vec();
    // A seeded walk over the characters that matter to the scan. Upstream computes the
    // step in JavaScript doubles (the product exceeds 2^53 and rounds), so it does here.
    let mut seed: f64 = 7.0;
    let alphabet: Vec<char> = "ab.@+-_ %:".chars().collect();
    for _ in 0..300 {
        let mut text = String::new();
        for _ in 0..24 {
            seed = (seed * 1_103_515_245.0 + 12_345.0) % 2_147_483_648.0;
            text.push(alphabet[(seed % alphabet.len() as f64) as usize]);
        }
        cases.push(text);
    }
    for text in &cases {
        assert_eq!(linked_emails(text), reference_emails(text), "{text:?}");
    }
}

#[test]
fn parses_scheme_url_angle_bracket_autolinks() {
    let result = parse_inline("see <https://example.com> ok", &gfm());
    assert_json(
        &result[1],
        link_json("https://example.com", "https://example.com"),
    );
}

#[test]
fn rejects_javascript_angle_bracket_autolinks_xss_prevention() {
    let result = parse_inline("see <javascript:alert(1)> ok", &gfm());
    // Should NOT produce a link node with a javascript: href.
    assert!(
        !result
            .iter()
            .any(|node| matches!(node, InlineNode::Link { .. })),
        "{result:?}"
    );
}

#[test]
fn parses_email_angle_bracket_autolinks() {
    let result = parse_inline("mail <user@example.com> please", &gfm());
    assert_json(
        &result[1],
        link_json("mailto:user@example.com", "user@example.com"),
    );
}

#[test]
fn peels_trailing_sentence_punctuation_off_bare_urls() {
    assert_json(
        &parse_inline("see https://example.com.", &gfm()),
        json!([
            {"type": "text", "content": "see "},
            link_json("https://example.com", "https://example.com"),
            {"type": "text", "content": "."},
        ]),
    );
}

#[test]
fn peels_unbalanced_trailing_close_parens() {
    assert_json(
        &parse_inline("(see https://example.com/foo) here", &gfm()),
        json!([
            {"type": "text", "content": "(see "},
            link_json("https://example.com/foo", "https://example.com/foo"),
            {"type": "text", "content": ") here"},
        ]),
    );
}

#[test]
fn keeps_balanced_parens_inside_the_url() {
    let result = parse_inline("go https://example.com/Foo_(bar) ok", &gfm());
    assert_json(
        &result[1],
        link_json(
            "https://example.com/Foo_(bar)",
            "https://example.com/Foo_(bar)",
        ),
    );
}

#[test]
fn peels_multiple_trailing_punctuation_chars_in_one_pass() {
    assert_json(
        &parse_inline("see https://example.com?!.", &gfm()),
        json!([
            {"type": "text", "content": "see "},
            link_json("https://example.com", "https://example.com"),
            {"type": "text", "content": "?!."},
        ]),
    );
}

#[test]
fn peels_interleaved_punct_and_unbalanced_parens() {
    assert_json(
        &parse_inline("(https://example.com/path).", &gfm()),
        json!([
            {"type": "text", "content": "("},
            link_json("https://example.com/path", "https://example.com/path"),
            {"type": "text", "content": ")."},
        ]),
    );
}

#[test]
fn peels_all_trailing_punctuation_leaving_only_scheme() {
    // "https://..." peels the dots, leaving "https://" as the href: a degenerate case,
    // but not rejected (TLDs and path content are not validated).
    let result = parse_inline("see https://...", &gfm());
    let href = result
        .iter()
        .find_map(|node| match node {
            InlineNode::Link { href, .. } => Some(href),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no link in {result:?}"));
    assert_eq!(href, "https://");
}

#[test]
fn does_not_autolink_urls_inside_inline_code() {
    assert_json(
        &parse_inline("try `https://example.com` here", &gfm()),
        json!([
            {"type": "text", "content": "try "},
            {"type": "code", "content": "https://example.com"},
            {"type": "text", "content": " here"},
        ]),
    );
}

#[test]
fn does_not_nest_links_skips_urls_inside_an_existing_link() {
    let result = parse_inline("[my site https://other.com](https://example.com)", &gfm());
    assert_eq!(result.len(), 1, "{result:?}");
    let InlineNode::Link { href, children } = &result[0] else {
        panic!("expected a link, got {result:?}");
    };
    assert_eq!(href, "https://example.com");
    // The label still has the URL as plain text, not a nested link.
    assert_eq!(children, &[text("my site https://other.com")]);
}

#[test]
fn does_not_autolink_the_destination_of_a_markdown_link() {
    let result = parse_inline("see [docs](https://example.com)", &gfm());
    assert_eq!(result.len(), 2, "{result:?}");
    let InlineNode::Link { href, .. } = &result[1] else {
        panic!("expected a link, got {result:?}");
    };
    assert_eq!(href, "https://example.com");
}

#[test]
fn autolinks_inside_emphasis_containers() {
    let result = parse_inline("see **https://example.com** ok", &gfm());
    let InlineNode::Bold { children } = &result[1] else {
        panic!("expected bold, got {result:?}");
    };
    assert_json(
        children,
        json!([link_json("https://example.com", "https://example.com")]),
    );
}

#[test]
fn does_not_match_when_preceded_by_an_alphanumeric_char() {
    assert_eq!(
        parse_inline("xhttps://example.com", &gfm()),
        vec![text("xhttps://example.com")]
    );
}

#[test]
fn handles_multiple_autolinks_in_one_block() {
    let result = parse_inline("see https://a.com or https://b.com or me@x.com", &gfm());
    let hrefs: Vec<&str> = result
        .iter()
        .filter_map(|node| match node {
            InlineNode::Link { href, .. } => Some(href.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(hrefs, ["https://a.com", "https://b.com", "mailto:me@x.com"]);
}

#[test]
fn does_not_autolink_inside_fenced_code_blocks() {
    let blocks = parse_markdown("```\nhttps://example.com\n```", &gfm());
    assert_eq!(blocks.len(), 1);
    assert_json(
        &blocks[0],
        json!({"type": "codeblock", "language": "plaintext", "content": "https://example.com"}),
    );
}

#[test]
fn passes_through_parse_markdown_for_headings_and_paragraphs() {
    let blocks = parse_markdown("# https://heading.com\n\nsee https://para.com here", &gfm());
    assert_eq!(blocks.len(), 2);
    if let BlockNode::Heading { children, .. } = &blocks[0] {
        assert_json(
            &children[0],
            link_json("https://heading.com", "https://heading.com"),
        );
    }
    if let BlockNode::Paragraph { children, .. } = &blocks[1] {
        assert!(
            children
                .iter()
                .any(|node| matches!(node, InlineNode::Link { .. })),
            "{blocks:?}"
        );
    }
}

#[test]
fn still_accepts_source_ids_via_legacy_signature() {
    // Default behavior: no autolink, source ids only (upstream passes the set positionally).
    let options = ParseOptions {
        source_ids: Some(ids(&["abc1"])),
        ..ParseOptions::default()
    };
    let result = parse_inline("see [abc1] for https://example.com", &options);
    assert!(has_citation(&result), "{result:?}");
    // The bare URL remains literal text because autolink is unset.
    assert!(
        result.contains(&text(" for https://example.com")),
        "{result:?}"
    );
}

#[test]
fn combines_source_ids_and_autolink_in_the_options_bag() {
    let options = ParseOptions {
        source_ids: Some(ids(&["abc1"])),
        autolink_gfm: true,
        ..ParseOptions::default()
    };
    let result = parse_inline("see [abc1] for https://example.com", &options);
    assert!(has_citation(&result), "{result:?}");
    assert!(
        result
            .iter()
            .any(|node| matches!(node, InlineNode::Link { .. })),
        "{result:?}"
    );
}

// ---------------------------------------------------------------------------
// describe('link reference definitions')
// ---------------------------------------------------------------------------

/// Upstream `collectLinks`' entries: `(type, href or src, text)`.
type Collected = (&'static str, String, String);

fn text_of(nodes: &[InlineNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            InlineNode::Text { content } => content.clone(),
            InlineNode::Bold { children }
            | InlineNode::Italic { children }
            | InlineNode::Strikethrough { children }
            | InlineNode::Link { children, .. } => text_of(children),
            _ => String::new(),
        })
        .collect()
}

/// Upstream `collectLinks`: every link and image node across an inline tree.
fn collect_links(nodes: &[InlineNode]) -> Vec<Collected> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            InlineNode::Link { href, children } => {
                out.push(("link", href.clone(), text_of(children)));
            }
            InlineNode::Image { src, alt } => out.push(("image", src.clone(), alt.clone())),
            InlineNode::Bold { children }
            | InlineNode::Italic { children }
            | InlineNode::Strikethrough { children } => out.extend(collect_links(children)),
            _ => {}
        }
    }
    out
}

fn paragraph_links_of(blocks: &[BlockNode]) -> Vec<Collected> {
    blocks
        .iter()
        .flat_map(|block| match block {
            BlockNode::Paragraph { children, .. } => collect_links(children),
            _ => Vec::new(),
        })
        .collect()
}

/// Upstream `paragraphLinks`.
fn paragraph_links(input: &str) -> (Vec<BlockNode>, Vec<Collected>) {
    let blocks = md(input);
    let links = paragraph_links_of(&blocks);
    (blocks, links)
}

fn link(href: &str, text: &str) -> Collected {
    ("link", href.to_string(), text.to_string())
}

#[test]
fn resolves_a_full_reference_text_label_and_drops_the_definition() {
    let (blocks, links) =
        paragraph_links("See [the docs][docs] here.\n\n[docs]: https://example.com/docs\n");
    // Only the paragraph survives; the definition line produces no block.
    assert_eq!(block_types(&blocks), ["paragraph"]);
    assert_eq!(links, [link("https://example.com/docs", "the docs")]);
}

#[test]
fn resolves_a_collapsed_reference_text() {
    let (_, links) = paragraph_links("See [the docs][].\n\n[the docs]: https://example.com/docs");
    assert_eq!(links, [link("https://example.com/docs", "the docs")]);
}

#[test]
fn resolves_a_shortcut_reference_text() {
    let (_, links) = paragraph_links("See [the docs].\n\n[the docs]: https://example.com/docs");
    assert_eq!(links, [link("https://example.com/docs", "the docs")]);
}

#[test]
fn resolves_a_reference_that_appears_before_its_definition() {
    let (_, links) = paragraph_links("[foo]\n\n[foo]: /bar");
    assert_eq!(links, [link("/bar", "foo")]);
}

#[test]
fn matches_labels_case_insensitively_with_collapsed_whitespace() {
    let (_, links) = paragraph_links("See [The   Docs][DOCS].\n\n[docs]: https://example.com/d");
    assert_eq!(links, [link("https://example.com/d", "The   Docs")]);
}

#[test]
fn supports_an_angle_bracket_destination_and_a_title() {
    let (_, links) = paragraph_links("See [x].\n\n[x]: <https://example.com/x> \"the title\"");
    assert_eq!(links, [link("https://example.com/x", "x")]);
}

#[test]
fn absorbs_a_title_on_the_line_after_a_title_less_definition() {
    let (blocks, links) =
        paragraph_links("See [x].\n\n[x]: https://example.com/x\n  \"the title\"\n");
    // The continuation title line must not leak as its own paragraph.
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    assert_eq!(links, [link("https://example.com/x", "x")]);
}

#[test]
fn resolves_an_empty_angle_bracket_destination_to_an_empty_href() {
    let (blocks, links) = paragraph_links("[foo]\n\n[foo]: <>");
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    assert_eq!(links, [link("", "foo")]);
}

#[test]
fn recognizes_a_definition_immediately_after_a_heading_no_blank_line() {
    let (blocks, links) = paragraph_links("# Title\n[x]: /url\n\n[x]");
    assert_eq!(block_types(&blocks), ["heading", "paragraph"]);
    assert_eq!(links, [link("/url", "x")]);
}

#[test]
fn recognizes_a_definition_immediately_after_a_closed_code_fence() {
    let (blocks, links) = paragraph_links("```\ncode\n```\n[x]: /url\n\n[x]");
    assert_eq!(block_types(&blocks), ["codeblock", "paragraph"]);
    assert_eq!(links, [link("/url", "x")]);
}

#[test]
fn uses_the_first_definition_when_a_label_is_defined_twice() {
    let (_, links) = paragraph_links("[a]\n\n[a]: /first\n[a]: /second");
    assert_eq!(links, [link("/first", "a")]);
}

#[test]
fn resolves_a_blockquote_nested_definition_even_with_a_top_level_definition_present() {
    let blocks = md("[outer]: /o\n\n> [inner]\n>\n> [inner]: /i");
    let quote = blocks
        .iter()
        .find_map(|block| match block {
            BlockNode::Blockquote { children, .. } => Some(children),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no blockquote in {blocks:?}"));
    assert_eq!(paragraph_links_of(quote), [link("/i", "inner")]);
}

#[test]
fn resolves_a_reference_image_alt_label() {
    let (_, links) = paragraph_links("![logo][l]\n\n[l]: /logo.png");
    assert_eq!(
        links,
        [("image", "/logo.png".to_string(), "logo".to_string())]
    );
}

#[test]
fn rejects_a_reference_image_whose_definition_has_an_unsafe_scheme_xss_prevention() {
    let (_, links) = paragraph_links("![logo][l]\n\n[l]: <javascript:alert(1)>");
    assert_eq!(links, []);
}

#[test]
fn rejects_a_shortcut_reference_image_with_an_unsafe_definition_xss_prevention() {
    let (_, links) = paragraph_links("![logo]\n\n[logo]: <javascript:alert(1)>");
    assert_eq!(links, []);
}

#[test]
fn rejects_definitions_that_hide_the_scheme_behind_control_chars_xss_prevention() {
    // The safe-URL check strips control characters before testing; the angle-bracket
    // destination form is the only definition shape that can contain them. Both consumers
    // of a definition (reference images and reference links) must apply it.
    let (_, image_links) = paragraph_links("![logo][l]\n\n[l]: <java\tscript:alert(1)>");
    assert_eq!(image_links, []);
    let (_, ref_links) = paragraph_links("[click][l]\n\n[l]: <java\tscript:alert(1)>");
    assert_eq!(ref_links, []);
}

#[test]
fn does_not_treat_a_whitespace_only_second_label_as_collapsed() {
    // `[foo][ ]` is a full reference to the empty (normalized) label and matches nothing;
    // `[foo]` still resolves as a shortcut and `[ ]` stays literal.
    let blocks = md("[foo][ ]\n\n[foo]: /f");
    assert_eq!(blocks[0].type_name(), "paragraph");
    let nodes = children(&blocks[0]);
    assert_eq!(collect_links(nodes), [link("/f", "foo")]);
    let literal: String = nodes
        .iter()
        .map(|node| match node {
            InlineNode::Text { content } => content.as_str(),
            _ => "",
        })
        .collect();
    assert!(literal.contains("[ ]"), "{literal:?}");
}

#[test]
fn leaves_an_unresolved_reference_as_literal_text() {
    let (blocks, links) = paragraph_links("See [the docs][missing].");
    assert_eq!(links, []);
    assert_eq!(blocks[0].type_name(), "paragraph");
    assert!(
        children(&blocks[0]).contains(&text("See [the docs][missing].")),
        "{blocks:?}"
    );
}

#[test]
fn does_not_resolve_a_definition_or_reference_inside_a_code_fence() {
    let blocks = md("```\n[x]\n[x]: /should-not-resolve\n```");
    assert_eq!(blocks.len(), 1);
    let BlockNode::Codeblock { content, .. } = &blocks[0] else {
        panic!("expected a codeblock, got {blocks:?}");
    };
    assert_eq!(content, "[x]\n[x]: /should-not-resolve");
}

#[test]
fn does_not_treat_a_definition_shaped_line_as_a_definition_mid_paragraph() {
    // CommonMark: a definition cannot interrupt a paragraph.
    let (blocks, links) = paragraph_links("Foo\n[bar]: /baz");
    assert_eq!(links, []);
    assert_eq!(block_types(&blocks), ["paragraph"]);
}

#[test]
fn leaves_footnote_markers_and_definitions_untouched() {
    let blocks = md("A claim.[^1]\n\n[^1]: https://example.com/note");
    // No link nodes; the `[^1]` marker stays literal and `[^1]:` is not treated as a
    // link reference definition (footnotes are out of scope).
    assert_eq!(paragraph_links_of(&blocks), []);
    assert_eq!(blocks.len(), 2, "{blocks:?}");
}

#[test]
fn leaves_ordinary_bracketed_text_with_no_definition_alone() {
    let (blocks, links) = paragraph_links("an array like [1, 2, 3] here");
    assert_eq!(links, []);
    assert_eq!(blocks[0].type_name(), "paragraph");
}

// ---------------------------------------------------------------------------
// describe('sourceRanges')
// ---------------------------------------------------------------------------

fn slice<'a>(source: &'a str, block: &BlockNode) -> Option<&'a str> {
    block.range().map(|range| &source[range.start..range.end])
}

#[test]
fn is_off_by_default() {
    assert_eq!(md("# Title")[0].range(), None);
}

#[test]
fn addresses_a_block_with_named_start_and_end_offsets() {
    let blocks = parse_markdown("One.\n\nTwo.", &ranges());
    assert_json(&blocks[1].range(), json!({"start": 6, "end": 10}));
}

#[test]
fn gives_every_top_level_block_the_source_it_came_from() {
    let source = [
        "# Title",
        "",
        "A paragraph that",
        "wraps onto two lines.",
        "",
        "- one",
        "- two",
        "",
        "```js",
        "const x = 1;",
        "```",
        "",
        "| a | b |",
        "| --- | --- |",
        "| 1 | 2 |",
        "",
        "> quoted",
        "",
        "---",
    ]
    .join("\n");
    let blocks = parse_markdown(&source, &ranges());
    let slices: Vec<Option<&str>> = blocks.iter().map(|block| slice(&source, block)).collect();
    assert_eq!(
        slices,
        [
            Some("# Title"),
            Some("A paragraph that\nwraps onto two lines."),
            Some("- one\n- two"),
            Some("```js\nconst x = 1;\n```"),
            Some("| a | b |\n| --- | --- |\n| 1 | 2 |"),
            Some("> quoted"),
            Some("---"),
        ]
    );
}

#[test]
fn reports_offsets_into_the_input_not_into_the_link_definition_stripped_text() {
    // The definition lines are removed before the block loop runs, so a naive offset
    // would drift by their length for everything after them.
    let source = [
        "[ref]: https://example.com",
        "",
        "See [the docs][ref].",
        "",
        "Another paragraph.",
    ]
    .join("\n");
    let blocks = parse_markdown(&source, &ranges());
    let slices: Vec<Option<&str>> = blocks.iter().map(|block| slice(&source, block)).collect();
    assert_eq!(
        slices,
        [Some("See [the docs][ref]."), Some("Another paragraph.")]
    );
}

#[test]
fn keeps_the_blank_lines_an_unterminated_fence_owns() {
    // Mid-stream the closing fence has not arrived, and the blank lines are part of the
    // code, not spacing between blocks.
    let source = "```\ncode\n\n";
    let blocks = parse_markdown(source, &ranges());
    // The whole thing: the fence consumed those lines as code.
    assert_eq!(slice(source, &blocks[0]), Some(source));
}

#[test]
fn slices_to_something_that_re_parses_to_the_same_block() {
    // The property a consumer needs, and the one that says the offsets are right: what
    // the range points at is the block.
    let check = |source: &str| {
        for block in parse_markdown(source, &ranges()) {
            let mut node = to_json(&block);
            node.as_object_mut()
                .expect("a block serializes to an object")
                .remove("range");
            let sliced = slice(source, &block).expect("a top-level block has a range");
            assert_eq!(
                to_json(&md(sliced)),
                json!([node]),
                "{source:?} / {sliced:?}"
            );
        }
    };
    check("# Title\n\nA paragraph.\n\n- one\n- two\n\n> quoted");
    // CRLF: the parser keeps the `\r` in its own content, so the range does too rather
    // than slicing to text that parses differently.
    check("# Title\r\n\r\nA paragraph.\r\n");
}

// ---------------------------------------------------------------------------
// parser.public.test.ts: describe('@astryxdesign/core/Markdown/parser')
// ---------------------------------------------------------------------------

/// The legacy-shape assertions of `exposes legacy and canonical parser results from a
/// server-only entry point` (its incremental line belongs to the incremental suite).
#[test]
fn exposes_legacy_parser_results() {
    let source = "# Heading\n\nParagraph";
    assert_match(&md(source)[0], json!({"type": "heading", "level": 1}));
    assert_match(&inline("**bold**")[0], json!({"type": "bold"}));
}
