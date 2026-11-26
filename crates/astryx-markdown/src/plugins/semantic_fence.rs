//! Semantic fences, upstream `Markdown/plugins/semanticFence.ts`
//! (`createMarkdownFenceTransform`): a transform that annotates eligible fenced-code nodes
//! with a proposal (an owned block extension node), which a renderer shows in the fence's
//! place. The code node itself, its source and its released projection stay as parsed.

use std::collections::HashSet;
use std::rc::Rc;

use super::{PluginError, Transform, TransformContext};
use crate::ast::{Block, Code, ExtensionDisplay, ExtensionNode, FenceProposal, Root};

/// Upstream `MarkdownFenceContext`: what `create_node` sees of a fence.
#[derive(Clone, Copy, Debug)]
pub struct FenceContext<'a> {
    pub code: &'a str,
    pub language: &'a str,
    pub meta: Option<&'a str>,
}

/// Upstream `createNode`: the plugin's block extension node for a fence (no `source`, no
/// `position`), or `None` to keep the ordinary code block. An `Err` fails the transform.
pub type FenceCreateNode = dyn Fn(&FenceContext<'_>) -> Result<Option<ExtensionNode>, String>;

/// Upstream `MarkdownFenceTransformOptions`.
#[derive(Clone)]
pub struct FenceTransformOptions {
    /// Exact, case-sensitive fence languages this helper owns.
    pub languages: Vec<String>,
    pub create_node: Rc<FenceCreateNode>,
}

const INVALID_NODE: &str = "Markdown fence createNode must return an owned block extension node with finite data and a registered renderer";

/// Upstream `annotateCode`. Returns whether the node changed.
fn annotate_code(
    code: &mut Code,
    languages: &HashSet<String>,
    context: &TransformContext<'_>,
    options: &FenceTransformOptions,
) -> Result<bool, String> {
    let ownership = context.ownership().expect("checked by the transform");
    let Some(lang) = code.lang.as_deref() else {
        return Ok(false);
    };
    if !languages.contains(lang) || code.proposal.node().is_some() {
        return Ok(false);
    }
    let fence = FenceContext {
        code: &code.value,
        language: lang,
        meta: code.meta.as_deref(),
    };
    let Some(node) = (options.create_node)(&fence)? else {
        return Ok(false);
    };
    // Every extension node must have a renderer; checking it here is what lets Core run
    // this helper on its trusted path.
    if node.plugin != ownership.plugin_name
        || crate::js::is_blank(&node.name)
        || !ownership.has_renderer(&node.name)
        || node.display != ExtensionDisplay::Block
        || node.source.is_some()
        || node.position.is_some()
    {
        return Err(INVALID_NODE.to_string());
    }
    // The fence's Core-authored source lines, recorded only with the source-lines plugin.
    let position = code
        .position
        .filter(|position| position.start.line.is_some());
    code.proposal = FenceProposal(Some(Box::new(ExtensionNode {
        position,
        source: None,
        ..node
    })));
    Ok(true)
}

/// Upstream `transformBlocks`: fences at any depth of blockquotes and lists.
fn transform_blocks(
    blocks: &mut [Block],
    languages: &HashSet<String>,
    context: &TransformContext<'_>,
    options: &FenceTransformOptions,
) -> Result<bool, String> {
    let mut changed = false;
    for block in blocks {
        changed |= match block {
            Block::Code(code) => annotate_code(code, languages, context, options)?,
            Block::Blockquote(quote) => {
                transform_blocks(&mut quote.children, languages, context, options)?
            }
            Block::List(list) => {
                let mut changed = false;
                for item in &mut list.children {
                    changed |= transform_blocks(&mut item.children, languages, context, options)?;
                }
                changed
            }
            _ => false,
        };
    }
    Ok(changed)
}

/// Upstream `createMarkdownFenceTransform`: a trusted transform that runs only through a
/// plugin (it needs the plugin's ownership) and claims source holding `\`\`\`lang` or
/// `~~~lang` for a declared language.
pub fn create_markdown_fence_transform(
    options: FenceTransformOptions,
) -> Result<Transform, PluginError> {
    if options.languages.is_empty() || options.languages.iter().any(|l| crate::js::is_blank(l)) {
        return Err(PluginError::new(
            "Markdown fence languages must be non-empty strings",
        ));
    }
    let languages: HashSet<String> = options.languages.iter().cloned().collect();
    if languages.len() != options.languages.len() {
        return Err(PluginError::new("Markdown fence languages must be unique"));
    }
    let needles: Vec<String> = options
        .languages
        .iter()
        .flat_map(|language| [format!("```{language}"), format!("~~~{language}")])
        .collect();
    let transform = Transform::new(move |root: &Root, context: &TransformContext<'_>| {
        if context.ownership().is_none() {
            return Err(
                "Markdown fence transforms must run through createMarkdownPlugin".to_string(),
            );
        }
        let mut next = root.clone();
        Ok(transform_blocks(&mut next.children, &languages, context, &options)?.then_some(next))
    });
    Ok(transform
        .with_claim(move |source| {
            needles
                .iter()
                .any(|needle| source.contains(needle.as_str()))
        })
        .trusted())
}
