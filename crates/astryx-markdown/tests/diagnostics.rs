//! The port's own surface around upstream's protocol: the diagnostic channel (FR19:
//! source-free events, deduplicated and rate-limited, admission failures that never
//! fail a parse), `depends_on` (AST-036 FR27), parse identity (FR15), the empty pipeline
//! (FR12) and plugin admission errors.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use astryx_markdown::ast::{Block, ExtensionDisplay, ExtensionNode, Phrasing};
use astryx_markdown::plugins::diagnostics::MAX_DIAGNOSTICS_PER_PARSE;
use astryx_markdown::plugins::{
    DiagnosticCallback, DiagnosticCode, DiagnosticSeverity, ExtensionRenderer, MarkdownPlugin,
    PluginDefinition, PluginDiagnostic, PluginPhase, SyntaxCapability, SyntaxContribution,
    TokenizeResult, Transform, create_markdown_plugin, markdown_source_lines_plugin,
};
use astryx_markdown::{
    ParseOptions, parse_inline_ast, parse_markdown_ast, parse_outline_from_markdown_with,
};
use serde_json::json;

/// The events a parse delivered.
type Events = Rc<RefCell<Vec<PluginDiagnostic>>>;

fn recorder() -> (Events, DiagnosticCallback) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    (
        events,
        Rc::new(move |event: &PluginDiagnostic| sink.borrow_mut().push(event.clone())),
    )
}

fn options(plugins: Vec<MarkdownPlugin>, callback: DiagnosticCallback) -> ParseOptions {
    ParseOptions {
        plugins,
        on_plugin_diagnostic: Some(callback),
        ..ParseOptions::default()
    }
}

fn mention_renderers() -> BTreeMap<String, ExtensionRenderer> {
    BTreeMap::from([(
        "mention".to_string(),
        ExtensionRenderer::new(|node| {
            Ok(format!("@{}", node.data["label"].as_str().unwrap_or("")))
        }),
    )])
}

/// The upstream protocol test's `@{label}` mention syntax.
fn mentions(name: &str) -> PluginDefinition {
    let plugin = name.to_string();
    PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["@{"], 80, move |input| {
                let Some(close) = input.source[input.offset..].find('}') else {
                    return Ok(if input.is_final {
                        TokenizeResult::NoMatch
                    } else {
                        TokenizeResult::Defer
                    });
                };
                let close = input.offset + close;
                Ok(TokenizeResult::Match {
                    end: close + 1,
                    node: ExtensionNode {
                        plugin: plugin.clone(),
                        name: "mention".into(),
                        data: json!({ "label": &input.source[input.offset + 2..close] }),
                        display: ExtensionDisplay::Inline,
                        source: None,
                        position: None,
                    },
                })
            })],
            block: vec![],
        }),
        renderers: mention_renderers(),
        ..PluginDefinition::new(name)
    }
}

fn failing(name: &str) -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(|_, _| {
            Err("Private plugin-thrown text".into())
        })),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

#[test]
fn omitted_and_empty_plugin_lists_are_one_pipeline() {
    for source in [
        "# Heading\n\nPlain **text** @{Ada}.",
        "- a\n- b",
        "> q\n\n| a |\n|---|\n| b |",
    ] {
        let omitted = parse_markdown_ast(source, &ParseOptions::default());
        let (events, callback) = recorder();
        let empty = parse_markdown_ast(source, &options(Vec::new(), callback));
        assert_eq!(omitted, empty);
        assert_eq!(
            parse_inline_ast(source, &ParseOptions::default()),
            parse_inline_ast(
                source,
                &ParseOptions {
                    plugins: Vec::new(),
                    ..ParseOptions::default()
                }
            )
        );
        assert!(events.borrow().is_empty());
    }
}

#[test]
fn a_duplicate_name_parses_without_plugins_and_reports_once() {
    let plugin = create_markdown_plugin(mentions("mentions")).unwrap();
    let (events, callback) = recorder();
    let source = "plain @{Ada}";
    let root = parse_markdown_ast(source, &options(vec![plugin.clone(), plugin], callback));
    assert_eq!(root, parse_markdown_ast(source, &ParseOptions::default()));
    assert_eq!(
        *events.borrow(),
        [PluginDiagnostic {
            plugin: "mentions".into(),
            phase: PluginPhase::Configuration,
            code: DiagnosticCode::DuplicateName,
            severity: DiagnosticSeverity::Error,
        }]
    );
}

#[test]
fn each_failure_is_one_source_free_event_per_parse() {
    let (events, callback) = recorder();
    let source = "Private production source";
    let opts = options(vec![failing("production-failure")], callback);
    parse_markdown_ast(source, &opts);
    assert_eq!(events.borrow().len(), 1);
    parse_markdown_ast(source, &opts);
    // Deduplication is per parse: a second parse reports its own failure.
    assert_eq!(events.borrow().len(), 2);
    let event = events.borrow()[0].clone();
    assert_eq!(event.plugin, "production-failure");
    assert_eq!(event.phase, PluginPhase::Transform);
    assert_eq!(event.code, DiagnosticCode::TransformFailed);
    assert_eq!(event.severity, DiagnosticSeverity::Error);
    let text = format!("{event:?} {event}");
    assert!(!text.contains(source));
    assert!(!text.contains("Private plugin-thrown text"));
    assert_eq!(
        event.to_string(),
        "Markdown: plugin \"production-failure\" failed in transform (transform-failed); rendered readable fallback."
    );
}

#[test]
fn diagnostics_are_rate_limited_with_a_suppression_code() {
    let (events, callback) = recorder();
    let plugins = (0..MAX_DIAGNOSTICS_PER_PARSE + 5)
        .map(|index| failing(&format!("failing-{index}")))
        .collect();
    let root = parse_markdown_ast("Keep me", &options(plugins, callback));
    assert_eq!(
        root,
        parse_markdown_ast("Keep me", &ParseOptions::default())
    );
    let events = events.borrow();
    assert_eq!(events.len(), MAX_DIAGNOSTICS_PER_PARSE + 1);
    let last = events.last().unwrap();
    assert_eq!(last.code, DiagnosticCode::Suppressed);
    assert_eq!(last.severity, DiagnosticSeverity::Warning);
    assert_eq!(last.code.as_str(), "diagnostics-suppressed");
}

#[test]
fn a_report_is_an_advisory_without_its_message() {
    let (events, callback) = recorder();
    let plugin = create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(|_, context| {
            context.report("the source says Secret");
            context.report("again");
            Ok(None)
        })),
        ..PluginDefinition::new("advisor")
    })
    .unwrap();
    parse_markdown_ast("Secret", &options(vec![plugin], callback));
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].code, DiagnosticCode::TransformReport);
    assert_eq!(events[0].severity, DiagnosticSeverity::Warning);
}

#[test]
fn syntax_failures_report_in_the_syntax_phase() {
    let (events, callback) = recorder();
    let broken = create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["%%"], 8, |_| Err("text".into()))],
            block: vec![SyntaxContribution::new(["!!"], 8, |input| {
                Ok(TokenizeResult::Match {
                    end: input.offset + 1,
                    node: ExtensionNode {
                        plugin: "broken".into(),
                        name: "bad".into(),
                        data: json!(null),
                        display: ExtensionDisplay::Block,
                        source: None,
                        position: None,
                    },
                })
            })],
        }),
        renderers: BTreeMap::from([(
            "bad".to_string(),
            ExtensionRenderer::new(|_| Ok(String::new())),
        )]),
        ..PluginDefinition::new("broken")
    })
    .unwrap();
    let root = parse_markdown_ast("a %% b\n\n!!x", &options(vec![broken], callback));
    assert_eq!(
        root,
        parse_markdown_ast("a %% b\n\n!!x", &ParseOptions::default())
    );
    let codes: Vec<_> = events
        .borrow()
        .iter()
        .map(|event| (event.phase, event.code))
        .collect();
    assert_eq!(
        codes,
        [
            (PluginPhase::Syntax, DiagnosticCode::TokenizerFailed),
            (PluginPhase::Syntax, DiagnosticCode::IncompleteBlockLines),
        ]
    );
}

#[test]
fn a_failing_to_text_falls_back_to_source_and_reports_render() {
    let (events, callback) = recorder();
    let plugin = create_markdown_plugin(PluginDefinition {
        renderers: BTreeMap::from([(
            "mention".to_string(),
            ExtensionRenderer::new(|_| Err("broken".into())),
        )]),
        ..mentions("mentions")
    })
    .unwrap();
    let outline = parse_outline_from_markdown_with("# Hi @{Ada}", &options(vec![plugin], callback));
    assert_eq!(outline[0].label, "Hi @{Ada}");
    assert_eq!(events.borrow()[0].phase, PluginPhase::Render);
    assert_eq!(events.borrow()[0].code, DiagnosticCode::RenderFailed);
}

fn marker(name: &str, depends_on: &[&str]) -> MarkdownPlugin {
    let label = name.to_string();
    create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(move |root, _| {
            let mut next = root.clone();
            if let Some(Block::Paragraph(paragraph)) = next.children.first_mut() {
                paragraph
                    .children
                    .push(Phrasing::Text(astryx_markdown::ast::Literal {
                        value: format!(" [{label}]"),
                        ..Default::default()
                    }));
            }
            Ok(Some(next))
        })),
        depends_on: depends_on.iter().map(|name| name.to_string()).collect(),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

fn text_of(root: &astryx_markdown::ast::Root) -> String {
    match &root.children[0] {
        Block::Paragraph(paragraph) => astryx_markdown::ast::markdown_ast_text(&paragraph.children),
        _ => String::new(),
    }
}

#[test]
fn depends_on_runs_a_transform_only_after_its_dependencies() {
    let (events, callback) = recorder();
    let root = parse_markdown_ast(
        "x",
        &options(
            vec![marker("first", &[]), marker("second", &["first"])],
            callback,
        ),
    );
    assert_eq!(text_of(&root), "x [first] [second]");
    assert!(events.borrow().is_empty());

    let (events, callback) = recorder();
    let root = parse_markdown_ast(
        "x",
        &options(
            vec![marker("second", &["first"]), marker("first", &[])],
            callback,
        ),
    );
    assert_eq!(text_of(&root), "x [first]");
    assert_eq!(events.borrow()[0].plugin, "second");
    assert_eq!(
        events.borrow()[0].code,
        DiagnosticCode::DependencyMisordered
    );

    let (events, callback) = recorder();
    let root = parse_markdown_ast("x", &options(vec![marker("second", &["absent"])], callback));
    assert_eq!(text_of(&root), "x");
    assert_eq!(events.borrow()[0].code, DiagnosticCode::DependencyMissing);
}

#[test]
fn an_unmet_dependency_keeps_the_plugins_syntax() {
    let plugin = create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(|_, _| Ok(None))),
        depends_on: vec!["absent".into()],
        ..mentions("mentions")
    })
    .unwrap();
    let nodes = parse_inline_ast(
        "@{Ada}",
        &ParseOptions {
            plugins: vec![plugin],
            ..ParseOptions::default()
        },
    );
    assert!(matches!(&nodes[..], [Phrasing::Extension(node)] if node.data["label"] == "Ada"));
}

#[test]
fn parse_identity_holds_options_and_syntax_only() {
    let plain = ParseOptions::default().parse_identity();
    let transform_only = ParseOptions {
        plugins: vec![failing("transform-only")],
        ..ParseOptions::default()
    };
    assert_eq!(transform_only.parse_identity(), plain);
    let with_syntax = |parse_key: &str| ParseOptions {
        plugins: vec![
            create_markdown_plugin(PluginDefinition {
                parse_key: Some(parse_key.into()),
                ..mentions("mentions")
            })
            .unwrap(),
        ],
        ..ParseOptions::default()
    };
    assert_ne!(with_syntax("v1").parse_identity(), plain);
    assert_ne!(
        with_syntax("v1").parse_identity(),
        with_syntax("v2").parse_identity()
    );
    assert!(
        with_syntax("v1")
            .parse_identity()
            .contains("mentions\u{0}1\u{0}v1")
    );
    let lines = ParseOptions {
        plugins: vec![markdown_source_lines_plugin()],
        ..ParseOptions::default()
    };
    assert!(
        lines
            .parse_identity()
            .contains("source-lines\u{0}1\u{0}source-lines")
    );
    for changed in [
        ParseOptions {
            math: true,
            ..ParseOptions::default()
        },
        ParseOptions {
            autolink_gfm: true,
            ..ParseOptions::default()
        },
        ParseOptions {
            is_final: false,
            ..ParseOptions::default()
        },
        ParseOptions {
            source_ids: Some(["a".to_string()].into()),
            ..ParseOptions::default()
        },
    ] {
        assert_ne!(changed.parse_identity(), plain);
    }
}

#[test]
fn streaming_defers_unfinished_syntax() {
    let plugin = create_markdown_plugin(mentions("mentions")).unwrap();
    let streaming = ParseOptions {
        plugins: vec![plugin.clone()],
        is_final: false,
        ..ParseOptions::default()
    };
    // The unfinished mention, and what follows it, wait for more source.
    let nodes = parse_inline_ast("Hi @{Ad", &streaming);
    assert_eq!(
        serde_json::to_value(&nodes).unwrap(),
        json!([{"type": "text", "value": "Hi "}])
    );
    let final_nodes = parse_inline_ast(
        "Hi @{Ad",
        &ParseOptions {
            plugins: vec![plugin],
            ..ParseOptions::default()
        },
    );
    assert_eq!(
        serde_json::to_value(&final_nodes).unwrap(),
        json!([{"type": "text", "value": "Hi @{Ad"}])
    );
}

#[test]
fn admission_refuses_invalid_definitions() {
    let error =
        |definition: PluginDefinition| create_markdown_plugin(definition).unwrap_err().to_string();
    let transform = || Some(Transform::new(|_, _| Ok(None)));
    assert_eq!(
        error(PluginDefinition {
            transform: transform(),
            ..PluginDefinition::new("  ")
        }),
        "Markdown plugin: name must be a non-empty string"
    );
    assert_eq!(
        error(PluginDefinition {
            api_version: 2,
            transform: transform(),
            ..PluginDefinition::new("v")
        }),
        "Markdown plugin: \"v\" uses unsupported apiVersion"
    );
    assert_eq!(
        error(PluginDefinition::new("nothing")),
        "Markdown plugin: \"nothing\" must declare syntax or transform"
    );
    assert_eq!(
        error(PluginDefinition {
            parse_key: Some(" ".into()),
            ..mentions("m")
        }),
        "Markdown plugin: \"m\" parseKey must be non-empty"
    );
    assert_eq!(
        error(PluginDefinition {
            syntax: Some(SyntaxCapability::default()),
            ..mentions("m")
        }),
        "Markdown plugin: \"m\" syntax must declare an inline or block contribution"
    );
    assert_eq!(
        error(PluginDefinition {
            syntax: Some(SyntaxCapability {
                inline: vec![SyntaxContribution::new(["@"], 0, |_| Ok(
                    TokenizeResult::NoMatch
                ))],
                block: vec![],
            }),
            ..mentions("m")
        }),
        "Markdown plugin: \"m\" syntax maxSpan must be positive and finite"
    );
    assert_eq!(
        error(PluginDefinition {
            syntax: Some(SyntaxCapability {
                inline: vec![SyntaxContribution::new(["@@@"], 2, |_| Ok(
                    TokenizeResult::NoMatch
                ))],
                block: vec![],
            }),
            ..mentions("m")
        }),
        "Markdown plugin: \"m\" syntax prefixes must be non-empty and bounded"
    );
    assert_eq!(
        error(PluginDefinition {
            renderers: BTreeMap::new(),
            ..mentions("m")
        }),
        "Markdown plugin: \"m\" syntax must provide renderers"
    );
    assert_eq!(
        error(PluginDefinition {
            depends_on: vec!["d".into()],
            transform: transform(),
            ..PluginDefinition::new("d")
        }),
        "Markdown plugin: \"d\" dependsOn must name other plugins"
    );
}
