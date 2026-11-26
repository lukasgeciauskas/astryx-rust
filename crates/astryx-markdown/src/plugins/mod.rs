//! Markdown's plugins, upstream `Markdown/plugins/`: the protocol (`protocol.ts`), the
//! diagnostic channel, and the first-party plugins and helpers (`textTransform.ts`,
//! `semanticFence.ts`, `sourceDecoration.ts`, `frontmatter.ts`, `softBreaks.ts`,
//! `sourceLines.ts`, `headingLinks.ts`).

pub mod diagnostics;
mod frontmatter;
pub(crate) mod heading_links;
mod protocol;
mod semantic_fence;
mod soft_breaks;
mod source_decoration;
mod source_lines;
mod text_transform;

pub use diagnostics::{
    DiagnosticCallback, DiagnosticCode, DiagnosticSeverity, PluginDiagnostic, PluginPhase,
};
pub use frontmatter::{
    FrontmatterFields, FrontmatterParseResult, MarkdownFrontmatter, create_markdown_frontmatter,
};
pub use heading_links::{HeadingLinksOptions, create_markdown_heading_links};
pub(crate) use protocol::{
    EntryRole, HelperOwnership, adopt_markdown_helper_nodes, apply_markdown_transforms,
    create_entry, extension_text, node_position, normalize_phrasing, prepare,
};
pub use protocol::{
    ExtensionRenderer, MARKDOWN_PLUGIN_API_VERSION, MarkdownPlugin, PluginCapabilities,
    PluginDefinition, PluginError, PreparedMarkdownPlugins, SourceClaim, SyntaxCapability,
    SyntaxContribution, ToText, TokenizeResult, Tokenizer, TokenizerInput, Transform,
    TransformContext, TransformFn, TransformResult, create_markdown_plugin,
    get_markdown_plugin_capabilities, is_markdown_extension_node, markdown_extension_text,
    normalize_ast_destinations, prepare_markdown_plugins,
};
pub use semantic_fence::{FenceContext, FenceTransformOptions, create_markdown_fence_transform};
pub use soft_breaks::markdown_soft_breaks_plugin;
pub use source_decoration::{
    SourceDecoration, SourceDecorationOptions, SourceDecorationRange,
    create_markdown_source_decoration, get_markdown_source_decorations,
};
pub use source_lines::markdown_source_lines_plugin;
pub use text_transform::{
    TextParentType, TextTransformContext, TextTransformOptions, create_markdown_text_transform,
};
