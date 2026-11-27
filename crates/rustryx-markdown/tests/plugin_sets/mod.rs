//! The exporter's test plugins (`tools/export/export-markdown.mjs`, `pluginSets`), defined
//! again in Rust, shared by the plugin and streaming fixture tests.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use regex::Regex;
use rustryx_markdown::ast::{
    Block, ExtensionDisplay, ExtensionNode, Literal, Parent, Phrasing, Root,
};
use rustryx_markdown::plugins::{
    ExtensionRenderer, FenceTransformOptions, HeadingLinksOptions, MarkdownPlugin,
    PluginDefinition, SourceDecorationOptions, SourceDecorationRange, SyntaxCapability,
    SyntaxContribution, TextTransformOptions, TokenizeResult, Transform,
    create_markdown_fence_transform, create_markdown_frontmatter, create_markdown_heading_links,
    create_markdown_plugin, create_markdown_source_decoration, create_markdown_text_transform,
    markdown_soft_breaks_plugin, markdown_source_lines_plugin,
};
use rustryx_markdown::remark::create_markdown_remark_transform;
use serde_json::{Value, json};

fn renderer(to_text: impl Fn(&ExtensionNode) -> String + 'static) -> ExtensionRenderer {
    ExtensionRenderer::new(move |node| Ok(to_text(node)))
}

fn renderers(entries: Vec<(&str, ExtensionRenderer)>) -> BTreeMap<String, ExtensionRenderer> {
    entries
        .into_iter()
        .map(|(name, renderer)| (name.to_string(), renderer))
        .collect()
}

fn extension(plugin: &str, name: &str, display: ExtensionDisplay, data: Value) -> ExtensionNode {
    ExtensionNode {
        plugin: plugin.into(),
        name: name.into(),
        data,
        display,
        source: None,
        position: None,
    }
}

fn text(value: &str) -> Phrasing {
    Phrasing::Text(Literal {
        value: value.into(),
        ..Literal::default()
    })
}

fn label(node: &ExtensionNode, key: &str) -> String {
    node.data[key].as_str().unwrap_or_default().to_string()
}

fn mentions() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["@{"], 80, |input| {
                let close = input.source[input.offset + 2..]
                    .find('}')
                    .map(|at| input.offset + 2 + at);
                let Some(close) = close.filter(|&close| close < input.end) else {
                    return Ok(if input.is_final {
                        TokenizeResult::NoMatch
                    } else {
                        TokenizeResult::Defer
                    });
                };
                Ok(TokenizeResult::Match {
                    end: close + 1,
                    node: extension(
                        "mentions",
                        "mention",
                        ExtensionDisplay::Inline,
                        json!({ "label": &input.source[input.offset + 2..close] }),
                    ),
                })
            })],
            block: vec![],
        }),
        renderers: renderers(vec![(
            "mention",
            renderer(|node| format!("@{}", label(node, "label"))),
        )]),
        ..PluginDefinition::new("mentions")
    })
    .unwrap()
}

fn handles() -> MarkdownPlugin {
    let pattern = Regex::new(r"^@([A-Za-z0-9_]+)").unwrap();
    create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["@"], 32, move |input| {
                let Some(found) = pattern.captures(&input.source[input.offset..input.end]) else {
                    return Ok(TokenizeResult::NoMatch);
                };
                Ok(TokenizeResult::Match {
                    end: input.offset + found[0].len(),
                    node: extension(
                        "handles",
                        "handle",
                        ExtensionDisplay::Inline,
                        json!({ "user": &found[1] }),
                    ),
                })
            })],
            block: vec![],
        }),
        renderers: renderers(vec![(
            "handle",
            renderer(|node| format!("@{}", label(node, "user"))),
        )]),
        ..PluginDefinition::new("handles")
    })
    .unwrap()
}

/// JavaScript's `trim` (whitespace includes U+FEFF and not U+0085).
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() && c != '\u{85}' || c == '\u{feff}')
}

fn callouts() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![],
            block: vec![SyntaxContribution::new([":::note"], 500, |input| {
                let close = input.source[input.offset + 7..]
                    .find("\n:::")
                    .map(|at| input.offset + 7 + at);
                let Some(close) = close.filter(|&close| close + 4 <= input.end) else {
                    return Ok(if input.is_final {
                        TokenizeResult::NoMatch
                    } else {
                        TokenizeResult::Defer
                    });
                };
                Ok(TokenizeResult::Match {
                    end: close + 4,
                    node: extension(
                        "callouts",
                        "callout",
                        ExtensionDisplay::Block,
                        json!({ "body": js_trim(&input.source[input.offset + 7..close]) }),
                    ),
                })
            })],
        }),
        renderers: renderers(vec![("callout", renderer(|node| label(node, "body")))]),
        ..PluginDefinition::new("callouts")
    })
    .unwrap()
}

fn broken_syntax() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["%%"], 16, |_| {
                Err("broken tokenizer".into())
            })],
            block: vec![SyntaxContribution::new(["%%%"], 16, |input| {
                Ok(TokenizeResult::Match {
                    end: input.offset + 2,
                    node: extension("broken-syntax", "bad", ExtensionDisplay::Block, Value::Null),
                })
            })],
        }),
        renderers: renderers(vec![("bad", renderer(|_| String::new()))]),
        ..PluginDefinition::new("broken-syntax")
    })
    .unwrap()
}

/// A transform over the top-level blocks.
fn map_blocks(root: &Root, map: &dyn Fn(Block) -> Block) -> Root {
    Root {
        children: root.children.iter().cloned().map(map).collect(),
        ..root.clone()
    }
}

fn transform_plugin(
    name: &str,
    run: impl Fn(&Root, &rustryx_markdown::plugins::TransformContext<'_>) -> Root + 'static,
) -> PluginDefinition {
    PluginDefinition {
        transform: Some(Transform::new(move |root, context| {
            Ok(Some(run(root, context)))
        })),
        ..PluginDefinition::new(name)
    }
}

fn badges() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        renderers: renderers(vec![("badge", renderer(|node| label(node, "label")))]),
        ..transform_plugin("badges", |root, _| {
            map_blocks(root, &|block| match block {
                Block::Paragraph(mut paragraph) => {
                    paragraph.children.push(Phrasing::Extension(extension(
                        "badges",
                        "badge",
                        ExtensionDisplay::Inline,
                        json!({ "label": "New" }),
                    )));
                    Block::Paragraph(paragraph)
                }
                other => other,
            })
        })
    })
    .unwrap()
}

fn replace_texts(children: &mut [Phrasing]) {
    for child in children {
        if let Phrasing::Text(literal) = child {
            literal.value = literal.value.replace("Draft", "Final");
        }
    }
}

fn replace_draft() -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin("replace-draft", |root, _| {
        map_blocks(root, &|block| match block {
            Block::Paragraph(mut node) => {
                replace_texts(&mut node.children);
                Block::Paragraph(node)
            }
            Block::Heading(mut node) => {
                replace_texts(&mut node.children);
                Block::Heading(node)
            }
            other => other,
        })
    }))
    .unwrap()
}

fn finality() -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin("finality", |root, context| {
        let value = if context.is_final() { "Final" } else { "Draft" };
        map_blocks(root, &|block| match block {
            Block::Heading(mut heading) => {
                heading.children = vec![text(value)];
                Block::Heading(heading)
            }
            other => other,
        })
    }))
    .unwrap()
}

fn relevel() -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin("relevel", |root, _| {
        map_blocks(root, &|block| match block {
            Block::Heading(mut heading) => {
                heading.depth = 6;
                Block::Heading(heading)
            }
            other => other,
        })
    }))
    .unwrap()
}

fn add_heading() -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin("add-heading", |root, _| {
        let mut next = root.clone();
        next.children
            .push(Block::Heading(rustryx_markdown::ast::Heading {
                depth: 2,
                children: vec![text("Generated")],
                ..Default::default()
            }));
        next
    }))
    .unwrap()
}

fn remove_headings() -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin("remove-headings", |root, _| Root {
        children: root
            .children
            .iter()
            .filter(|block| !matches!(block, Block::Heading(_)))
            .cloned()
            .collect(),
        ..root.clone()
    }))
    .unwrap()
}

fn delete_foreign() -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin("delete-foreign", |root, _| {
        map_blocks(root, &|block| match block {
            Block::Paragraph(mut paragraph) => {
                paragraph
                    .children
                    .retain(|node| !matches!(node, Phrasing::Extension(_)));
                Block::Paragraph(paragraph)
            }
            other => other,
        })
    }))
    .unwrap()
}

fn set_link(url: &'static str) -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin("set-link", move |root, _| {
        map_blocks(root, &|block| match block {
            Block::Paragraph(mut paragraph) => {
                paragraph
                    .children
                    .push(Phrasing::Link(rustryx_markdown::ast::Link {
                        url: url.into(),
                        children: vec![text("go")],
                        ..Default::default()
                    }));
                Block::Paragraph(paragraph)
            }
            other => other,
        })
    }))
    .unwrap()
}

fn failing() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(|_, _| Err("private failure text".into()))),
        ..PluginDefinition::new("failing")
    })
    .unwrap()
}

fn with_transform(name: &str, transform: Transform) -> PluginDefinition {
    PluginDefinition {
        transform: Some(transform),
        ..PluginDefinition::new(name)
    }
}

fn shortcodes() -> MarkdownPlugin {
    let transform = create_markdown_text_transform(TextTransformOptions {
        required_substrings: Some(vec![":".into()]),
        ..TextTransformOptions::new(Regex::new(":([a-z]+):").unwrap(), |found, _| {
            vec![Phrasing::Strong(Parent {
                children: vec![text(&found[1].to_uppercase())],
                ..Parent::default()
            })]
        })
    })
    .unwrap();
    create_markdown_plugin(with_transform("shortcodes", transform)).unwrap()
}

fn todo_links() -> MarkdownPlugin {
    let transform = create_markdown_text_transform(TextTransformOptions::new(
        Regex::new(r"(?-u:\b)TODO(?-u:\b)").unwrap(),
        |_, _| {
            vec![Phrasing::Link(rustryx_markdown::ast::Link {
                url: "  /todo  ".into(),
                children: vec![text("todo")],
                ..Default::default()
            })]
        },
    ))
    .unwrap();
    create_markdown_plugin(with_transform("todo-links", transform)).unwrap()
}

fn drafts() -> MarkdownPlugin {
    let transform = create_markdown_text_transform(TextTransformOptions {
        required_substrings: Some(vec!["raft".into()]),
        ..TextTransformOptions::new(Regex::new("(?i)draft").unwrap(), |found, _| {
            vec![Phrasing::Emphasis(Parent {
                children: vec![text(&found[0])],
                ..Parent::default()
            })]
        })
    })
    .unwrap();
    create_markdown_plugin(with_transform("drafts", transform)).unwrap()
}

fn charts() -> MarkdownPlugin {
    let transform = create_markdown_fence_transform(FenceTransformOptions {
        languages: vec!["chart".into()],
        create_node: Rc::new(|fence| {
            Ok((!js_trim(fence.code).is_empty()).then(|| {
                extension(
                    "charts",
                    "chart",
                    ExtensionDisplay::Block,
                    json!({ "code": fence.code, "meta": fence.meta }),
                )
            }))
        }),
    })
    .unwrap();
    create_markdown_plugin(PluginDefinition {
        renderers: renderers(vec![("chart", renderer(|node| label(node, "code")))]),
        ..with_transform("charts", transform)
    })
    .unwrap()
}

fn review() -> MarkdownPlugin {
    let range = |start, end, data| SourceDecorationRange { start, end, data };
    let transform = create_markdown_source_decoration(SourceDecorationOptions {
        name: "review-hit".into(),
        ranges: vec![
            range(12, 30, None),
            range(0, 3, Some(json!({ "reviewer": "ada" }))),
            range(0, 3, Some(json!({ "reviewer": "ada" }))),
        ],
    })
    .unwrap();
    create_markdown_plugin(with_transform("review", transform)).unwrap()
}

fn frontmatter() -> MarkdownPlugin {
    create_markdown_frontmatter("meta", |fields| Ok(fields.clone()))
        .unwrap()
        .plugin
}

/// Upstream's remark plugin shape: mutate the tree in place.
fn remark(name: &str, mutate: impl Fn(&mut Value) + 'static) -> MarkdownPlugin {
    let transform = create_markdown_remark_transform(move |mut tree, _file| {
        mutate(&mut tree);
        Ok(tree)
    });
    create_markdown_plugin(with_transform(name, transform)).unwrap()
}

fn shout() -> MarkdownPlugin {
    fn visit(node: &mut Value) {
        if node["type"] == "heading"
            && let Some(children) = node["children"].as_array_mut()
        {
            for child in children {
                if child["type"] == "text" {
                    let upper = child["value"].as_str().unwrap_or_default().to_uppercase();
                    child["value"] = upper.into();
                }
            }
        }
        if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
            children.iter_mut().for_each(visit);
        }
    }
    remark("shout", visit)
}

fn remark_html() -> MarkdownPlugin {
    remark("remark-html", |tree| {
        if let Some(paragraph) = tree["children"]
            .as_array_mut()
            .and_then(|children| children.iter_mut().find(|node| node["type"] == "paragraph"))
            && let Some(children) = paragraph["children"].as_array_mut()
        {
            children.push(json!({ "type": "html", "value": "<b>x</b>" }));
        }
    })
}

fn remark_data() -> MarkdownPlugin {
    remark("remark-data", |tree| {
        let children = tree["children"].as_array_mut().unwrap();
        for node in children.iter_mut() {
            node["data"] = json!({ "seen": true });
        }
        children.push(json!({
            "type": "paragraph",
            "children": [{
                "type": "link",
                "url": " https://example.com/x ",
                "title": null,
                "children": [{ "type": "text", "value": "added" }],
            }],
        }));
    })
}

/// The exporter's plugin sets, by name.
pub fn plugin_sets() -> HashMap<&'static str, Vec<MarkdownPlugin>> {
    let heading_links = || {
        create_markdown_heading_links(HeadingLinksOptions {
            heading_id_prefix: Some("doc".into()),
            permalink_base_url: Some("https://example.com/page#old".into()),
        })
        .unwrap()
    };
    HashMap::from([
        ("mentions", vec![mentions()]),
        ("handles", vec![handles()]),
        ("callouts", vec![callouts()]),
        ("mentions+callouts", vec![mentions(), callouts()]),
        ("broken-syntax", vec![broken_syntax()]),
        ("badges", vec![badges()]),
        ("replace-draft", vec![replace_draft()]),
        ("finality", vec![finality()]),
        ("relevel", vec![relevel()]),
        ("add-heading", vec![add_heading()]),
        ("remove-headings", vec![remove_headings()]),
        (
            "mentions+delete-foreign",
            vec![mentions(), delete_foreign()],
        ),
        ("unsafe-link", vec![set_link("javascript:alert(1)")]),
        ("padded-link", vec![set_link("  /docs  ")]),
        ("failing", vec![failing()]),
        ("soft-breaks", vec![markdown_soft_breaks_plugin()]),
        ("source-lines", vec![markdown_source_lines_plugin()]),
        (
            "source-lines+callouts",
            vec![markdown_source_lines_plugin(), callouts()],
        ),
        ("shortcodes", vec![shortcodes()]),
        ("todo-links", vec![todo_links()]),
        ("drafts", vec![drafts()]),
        ("charts", vec![charts()]),
        (
            "charts+source-lines",
            vec![charts(), markdown_source_lines_plugin()],
        ),
        ("review", vec![review()]),
        ("frontmatter", vec![frontmatter()]),
        (
            "frontmatter+source-lines",
            vec![frontmatter(), markdown_source_lines_plugin()],
        ),
        ("heading-links", vec![heading_links()]),
        (
            "heading-links-plain",
            vec![create_markdown_heading_links(HeadingLinksOptions::default()).unwrap()],
        ),
        ("heading-links+mentions", vec![mentions(), heading_links()]),
        ("shout", vec![shout()]),
        ("remark-html", vec![remark_html()]),
        ("remark-data", vec![remark_data()]),
        (
            "badges+replace-draft+shout",
            vec![badges(), replace_draft(), shout()],
        ),
    ])
}
