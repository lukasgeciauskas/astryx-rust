//! Plugins' extension syntax, upstream `Markdown/parser.ts` (`matchExtensionSyntax`):
//! the contributions whose prefix starts at an offset, tried in plugin order, each shown
//! at most its `max_span` of the text, and each result checked before it becomes a node.

use super::ResolvedOptions;
use crate::ast::{ExtensionDisplay, ExtensionNode};
use crate::plugins::diagnostics::{DiagnosticCode, PluginPhase};
use crate::plugins::{TokenizeResult, TokenizerInput};

/// Upstream `ExtensionMatch`.
#[derive(Clone, Debug)]
pub(super) enum ExtensionMatch {
    None,
    /// The text may hold the syntax once more of it arrives: the parse stops here.
    Defer,
    Match {
        node: Box<ExtensionNode>,
        end: usize,
    },
}

/// Upstream `matchExtensionSyntax`: the extension syntax at `offset` of `source`.
pub(super) fn match_extension_syntax(
    source: &str,
    offset: usize,
    context: ExtensionDisplay,
    opts: &ResolvedOptions,
) -> ExtensionMatch {
    let Some(plugins) = opts.plugins else {
        return ExtensionMatch::None;
    };
    let by_first_char = match context {
        ExtensionDisplay::Inline => &plugins.inline_by_first_char,
        ExtensionDisplay::Block if !opts.allow_block_syntax => return ExtensionMatch::None,
        ExtensionDisplay::Block => &plugins.block_by_first_char,
    };
    let rest = &source[offset..];
    let Some(candidates) = rest.chars().next().and_then(|c| by_first_char.get(&c)) else {
        return ExtensionMatch::None;
    };
    for &index in candidates {
        let candidate = &plugins.syntax[index];
        let contribution = &candidate.contribution;
        let mut full_prefix = false;
        for prefix in &contribution.starts_with {
            if rest.starts_with(prefix.as_str()) {
                full_prefix = true;
                break;
            }
            if !opts.is_final && rest.len() < prefix.len() && prefix.starts_with(rest) {
                return ExtensionMatch::Defer;
            }
        }
        if !full_prefix {
            continue;
        }
        let mut end = source.len().min(offset + contribution.max_span);
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        let line_start = source[..offset].rfind('\n').map_or(0, |at| at + 1);
        let input = TokenizerInput {
            source: &source[..end],
            offset,
            end,
            is_final: opts.is_final,
            context,
            line_start,
            column: offset - line_start,
        };
        let result = match (contribution.tokenize)(&input) {
            Ok(result) => result,
            Err(_) => {
                opts.diagnostics.report(
                    &candidate.plugin_name,
                    PluginPhase::Syntax,
                    DiagnosticCode::TokenizerFailed,
                );
                continue;
            }
        };
        match result {
            TokenizeResult::NoMatch => continue,
            TokenizeResult::Defer => {
                return if !opts.is_final
                    && end == source.len()
                    && source.len() - offset < contribution.max_span
                {
                    ExtensionMatch::Defer
                } else {
                    ExtensionMatch::None
                };
            }
            TokenizeResult::Match {
                end: match_end,
                node,
            } => {
                if match_end <= offset
                    || match_end > end
                    || !source.is_char_boundary(match_end)
                    || node.plugin != candidate.plugin_name
                    || node.display != context
                    || crate::js::is_blank(&node.name)
                    || !plugins.has_renderer(&candidate.plugin_name, &node.name)
                {
                    opts.diagnostics.report(
                        &candidate.plugin_name,
                        PluginPhase::Syntax,
                        DiagnosticCode::InvalidExtensionNode,
                    );
                    continue;
                }
                // Provenance is Core's: a tokenizer's own `source` and `position` go.
                return ExtensionMatch::Match {
                    node: Box::new(ExtensionNode {
                        source: Some(source[offset..match_end].to_string()),
                        position: None,
                        ..node
                    }),
                    end: match_end,
                };
            }
        }
    }
    ExtensionMatch::None
}
