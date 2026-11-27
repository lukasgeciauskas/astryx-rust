//! Markdown's plugin protocol, upstream `Markdown/plugins/protocol.ts`: opaque plugin entries
//! (`createMarkdownPlugin`), their capabilities, preparation of an ordered list (admission,
//! duplicate names, API version, syntax indexes, parse identity), and the immutable
//! transform pipeline with upstream's validation and ownership rules
//! (`applyMarkdownTransforms`, `validateAst`), the helper-node adoption first-party helpers
//! use, URL normalization of plugin output and extension text.
//!
//! Upstream freezes every tree a plugin may observe and validates what it returns. Rust
//! gives the same guarantees by ownership: a transform receives `&Root` and returns a new
//! tree or nothing, so it cannot change what an earlier or later stage observes, and a
//! failure (an `Err` or an invalid tree) keeps the last valid document. JavaScript-only
//! guards (frozen objects, promise defusing, prototype and cycle checks) have nothing to
//! guard here; what the type system does not rule out (content categories of extension
//! nodes, nested links, positions, URLs, ownership, heading identity) is checked as
//! upstream checks it.
//!
//! Renderers: upstream's renderer pairs a React `render` with `toText`. This crate has no UI, so
//! [`ExtensionRenderer`] keeps `to_text` and an opaque `render` slot
//! (`Rc<dyn Any>`) for the UI layer: the components crate stores its own closure type there
//! and downcasts it back. Markdown never calls or inspects the slot.

use std::any::Any;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::diagnostics::{DiagnosticCallback, DiagnosticCode, Diagnostics, PluginPhase};
use crate::ast::{
    Block, Code, ExtensionDisplay, ExtensionNode, NodeRef, NodeType, Phrasing, Position, Root,
    SourceHeading,
};
use crate::url::{is_safe_markdown_parser_url, normalized_markdown_destination};

/// The one protocol version (upstream `apiVersion: 1`).
pub const MARKDOWN_PLUGIN_API_VERSION: u32 = 1;

/// Upstream's `fail`: why a plugin, a helper or a list was refused. Upstream throws it; the
/// port returns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginError {
    message: String,
}

impl PluginError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        PluginError {
            message: message.into(),
        }
    }

    /// Upstream's message, e.g. `Markdown plugin: duplicate name "a"`.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PluginError {}

/// Upstream `fail`: a protocol error, with its `Markdown plugin: ` prefix.
fn fail<T>(message: impl std::fmt::Display) -> Result<T, PluginError> {
    Err(PluginError::new(format!("Markdown plugin: {message}")))
}

/// Upstream `MarkdownTokenizerInput`: what a tokenizer sees at a candidate offset. Offsets are
/// UTF-8 bytes into `source` (upstream's are UTF-16 units).
#[derive(Clone, Copy, Debug)]
pub struct TokenizerInput<'a> {
    /// The text being scanned, cut at `end`: a block's inline text, or the document with its
    /// link reference definitions removed.
    pub source: &'a str,
    /// Where the candidate starts; one of the contribution's prefixes starts here.
    pub offset: usize,
    /// The exclusive bound visible to this tokenizer: `offset + max_span`, or the end of the
    /// text, whichever comes first (on a character boundary).
    pub end: usize,
    /// Whether more source may follow (false while streaming).
    pub is_final: bool,
    pub context: ExtensionDisplay,
    /// Where the candidate's line starts.
    pub line_start: usize,
    /// `offset - line_start`.
    pub column: usize,
}

/// Upstream `MarkdownTokenizeResult`.
#[derive(Clone, Debug, PartialEq)]
// Returned once per candidate and never stored, so the node stays unboxed.
#[allow(clippy::large_enum_variant)]
pub enum TokenizeResult {
    /// Not this contribution's syntax here.
    NoMatch,
    /// The syntax may complete once more source arrives (honoured only while streaming,
    /// and only when the candidate reaches the end of the source).
    Defer,
    /// The syntax spans `offset..end` and reads as `node`. The node's `source` and `position` are
    /// Core's to assign and are ignored.
    Match { end: usize, node: ExtensionNode },
}

/// A tokenizer. An `Err` is a failure (upstream: a throw); its text is never reported.
pub type Tokenizer = dyn Fn(&TokenizerInput<'_>) -> Result<TokenizeResult, String>;

/// Upstream `MarkdownSyntaxContribution`.
#[derive(Clone)]
pub struct SyntaxContribution {
    /// The prefixes a candidate starts with; each non-empty and no longer than `max_span`.
    pub starts_with: Vec<String>,
    /// The most bytes Core exposes from a candidate offset (upstream: UTF-16 units).
    pub max_span: usize,
    pub tokenize: Rc<Tokenizer>,
}

impl SyntaxContribution {
    pub fn new<S: Into<String>>(
        starts_with: impl IntoIterator<Item = S>,
        max_span: usize,
        tokenize: impl Fn(&TokenizerInput<'_>) -> Result<TokenizeResult, String> + 'static,
    ) -> Self {
        SyntaxContribution {
            starts_with: starts_with.into_iter().map(Into::into).collect(),
            max_span,
            tokenize: Rc::new(tokenize),
        }
    }
}

impl fmt::Debug for SyntaxContribution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxContribution")
            .field("starts_with", &self.starts_with)
            .field("max_span", &self.max_span)
            .finish_non_exhaustive()
    }
}

/// Upstream `MarkdownSyntaxCapability`: inline and block contributions. Block syntax applies
/// at the top level only, after the built-in blocks; inline syntax applies in every
/// inline text except link text and image descriptions, after the built-ins.
#[derive(Clone, Debug, Default)]
pub struct SyntaxCapability {
    pub inline: Vec<SyntaxContribution>,
    pub block: Vec<SyntaxContribution>,
}

/// Upstream `MarkdownTransformContext`, with the ownership facts upstream hides behind
/// registered symbols (`getMarkdownHelperOwnership`).
pub struct TransformContext<'a> {
    source: &'a str,
    is_final: bool,
    display: ExtensionDisplay,
    owner: Option<Owner<'a>>,
}

struct Owner<'a> {
    plugin_name: &'a str,
    plugins: &'a PreparedMarkdownPlugins,
    source_unchanged: bool,
    diagnostics: &'a Diagnostics,
}

impl<'a> TransformContext<'a> {
    /// A context for running a transform outside Markdown's pipeline: it owns no plugin
    /// and no renderer, and `report` goes nowhere.
    pub fn new(source: &'a str, is_final: bool, display: ExtensionDisplay) -> Self {
        TransformContext {
            source,
            is_final,
            display,
            owner: None,
        }
    }

    /// The source the document was parsed from.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Whether the source is complete (false while streaming).
    pub fn is_final(&self) -> bool {
        self.is_final
    }

    /// Block for `parse_markdown_ast`, inline for `parse_inline_ast` (whose document is one
    /// paragraph, and must stay one).
    pub fn display(&self) -> ExtensionDisplay {
        self.display
    }

    /// Upstream `report`: one advisory diagnostic for this plugin. The message is never
    /// delivered (diagnostics are source-free), so it may quote the document.
    pub fn report(&self, message: &str) {
        let _ = message;
        if let Some(owner) = &self.owner {
            owner.diagnostics.report(
                owner.plugin_name,
                PluginPhase::Transform,
                DiagnosticCode::TransformReport,
            );
        }
    }

    /// Upstream `getMarkdownTransformPluginName`: the plugin running this transform.
    pub fn plugin_name(&self) -> Option<&'a str> {
        self.owner.as_ref().map(|owner| owner.plugin_name)
    }

    /// Upstream `getMarkdownHelperOwnership`.
    pub(crate) fn ownership(&self) -> Option<HelperOwnership<'a>> {
        self.owner.as_ref().map(|owner| HelperOwnership {
            plugin_name: owner.plugin_name,
            plugins: Some(owner.plugins),
            source_unchanged: owner.source_unchanged,
        })
    }
}

/// Upstream `MarkdownHelperOwnership`: what a Core helper needs to validate the nodes its
/// caller's callback handed it.
#[derive(Clone, Copy)]
pub(crate) struct HelperOwnership<'a> {
    pub(crate) plugin_name: &'a str,
    plugins: Option<&'a PreparedMarkdownPlugins>,
    /// Whether the source still describes the transform's input.
    pub(crate) source_unchanged: bool,
}

impl HelperOwnership<'_> {
    /// Upstream's `UNOWNED`: for a helper run outside the pipeline, owning nothing.
    pub(crate) const UNOWNED: HelperOwnership<'static> = HelperOwnership {
        plugin_name: "\0unowned",
        plugins: None,
        source_unchanged: false,
    };

    pub(crate) fn has_renderer(&self, node_name: &str) -> bool {
        self.plugins
            .is_some_and(|plugins| plugins.has_renderer(self.plugin_name, node_name))
    }
}

/// What a transform returns: `Ok(None)` when it leaves the document as it is (upstream:
/// returns its input), `Ok(Some(root))` for a new document, `Err` when it fails (upstream:
/// throws; the text is never reported).
pub type TransformResult = Result<Option<Root>, String>;

/// Upstream `MarkdownTransform`: a synchronous function from the document to a new one. It
/// runs again for every streamed update, so it must be idempotent and convergent.
/// A transform's function.
pub type TransformFn = dyn Fn(&Root, &TransformContext<'_>) -> TransformResult;

/// A transform's source claim.
pub type SourceClaim = dyn Fn(&str) -> bool;

#[derive(Clone)]
pub struct Transform {
    run: Rc<TransformFn>,
    /// Upstream `markMarkdownTransformClaim`: a cheap check that the source may hold
    /// something for the transform; `false` skips it while no earlier transform changed the
    /// tree.
    claims: Option<Rc<SourceClaim>>,
    /// Upstream `markMarkdownTransformTrusted`: a Core helper whose output needs no
    /// validation. Only this crate's helpers are trusted.
    trusted: bool,
}

impl Transform {
    pub fn new(run: impl Fn(&Root, &TransformContext<'_>) -> TransformResult + 'static) -> Self {
        Transform {
            run: Rc::new(run),
            claims: None,
            trusted: false,
        }
    }

    /// Runs the transform.
    pub fn run(&self, root: &Root, context: &TransformContext<'_>) -> TransformResult {
        (self.run)(root, context)
    }

    pub(crate) fn trusted(mut self) -> Self {
        self.trusted = true;
        self
    }

    pub(crate) fn with_claim(mut self, claims: impl Fn(&str) -> bool + 'static) -> Self {
        self.claims = Some(Rc::new(claims));
        self
    }
}

impl fmt::Debug for Transform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transform")
            .field("trusted", &self.trusted)
            .finish_non_exhaustive()
    }
}

/// A renderer's text projection. An `Err` is a failure; the node's source stands in.
pub type ToText = dyn Fn(&ExtensionNode) -> Result<String, String>;

/// Upstream `MarkdownExtensionRenderer`: how one extension node kind reads as text, and an
/// opaque slot for the UI layer's renderer.
///
/// The `render` contract: this crate stores the value and hands it back by reference
/// ([`PreparedMarkdownPlugins::renderer`]); it never calls or inspects it. The UI layer
/// (`rustryx-components`) defines its own render closure type, stores it here wrapped in an
/// `Rc`, and downcasts it back with `Rc::downcast` or `<dyn Any>::downcast_ref`. A slot of
/// another type, or none, means "no renderer": the UI layer renders the node's readable
/// fallback (its `to_text` or source), as upstream does for a renderer that throws.
#[derive(Clone)]
pub struct ExtensionRenderer {
    pub to_text: Rc<ToText>,
    pub render: Option<Rc<dyn Any>>,
}

impl ExtensionRenderer {
    /// A renderer with `to_text` and no UI renderer yet.
    pub fn new(to_text: impl Fn(&ExtensionNode) -> Result<String, String> + 'static) -> Self {
        ExtensionRenderer {
            to_text: Rc::new(to_text),
            render: None,
        }
    }

    /// Sets the UI layer's renderer (see the type's documentation).
    pub fn with_render(mut self, render: Rc<dyn Any>) -> Self {
        self.render = Some(render);
        self
    }
}

impl fmt::Debug for ExtensionRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionRenderer")
            .field("render", &self.render.is_some())
            .finish_non_exhaustive()
    }
}

/// Upstream `MarkdownPluginDefinition` (syntax and transform plugins in one shape). Build it
/// with [`PluginDefinition::new`] and struct update syntax, then admit it with
/// [`create_markdown_plugin`].
#[derive(Clone, Debug)]
pub struct PluginDefinition {
    pub name: String,
    /// Must be [`MARKDOWN_PLUGIN_API_VERSION`].
    pub api_version: u32,
    /// Required with `syntax`: the syntax's version in parse identity (FR15).
    pub parse_key: Option<String>,
    pub syntax: Option<SyntaxCapability>,
    pub transform: Option<Transform>,
    /// By extension node name. Required (non-empty) with `syntax`; every extension node a
    /// plugin creates must have one.
    pub renderers: BTreeMap<String, ExtensionRenderer>,
    /// Plugins whose extension nodes this plugin's transform reads (AST-036 FR27): each
    /// must be installed earlier in the list, or this transform does not run. Not in
    /// upstream's code yet; its spec's rule.
    pub depends_on: Vec<String>,
}

impl PluginDefinition {
    /// An empty definition named `name` at the current API version.
    pub fn new(name: impl Into<String>) -> Self {
        PluginDefinition {
            name: name.into(),
            ..PluginDefinition::default()
        }
    }
}

impl Default for PluginDefinition {
    fn default() -> Self {
        PluginDefinition {
            name: String::new(),
            api_version: MARKDOWN_PLUGIN_API_VERSION,
            parse_key: None,
            syntax: None,
            transform: None,
            renderers: BTreeMap::new(),
            depends_on: Vec::new(),
        }
    }
}

/// A first-party entry's internal role, which upstream carries on hidden definition keys.
#[derive(Clone, Debug)]
pub(crate) enum EntryRole {
    Ordinary,
    /// Upstream `markdownSourceLinesDefinition`: the source-lines parse option.
    SourceLines,
    /// Upstream's portable heading-links config.
    HeadingLinks(Rc<crate::plugins::heading_links::HeadingLinksConfig>),
}

/// Upstream `MarkdownPluginEntry`: an opaque, admitted plugin, cheap to clone and share
/// between Markdown, the parser entry points and Outline.
#[derive(Clone, Debug)]
pub struct MarkdownPlugin {
    definition: Rc<PluginDefinition>,
    pub(crate) role: EntryRole,
}

impl MarkdownPlugin {
    pub fn name(&self) -> &str {
        &self.definition.name
    }

    pub fn api_version(&self) -> u32 {
        self.definition.api_version
    }

    pub(crate) fn definition(&self) -> &PluginDefinition {
        &self.definition
    }

    /// Whether two entries are the same admitted plugin (one [`create_markdown_plugin`] call).
    pub fn ptr_eq(&self, other: &MarkdownPlugin) -> bool {
        Rc::ptr_eq(&self.definition, &other.definition)
    }
}

/// Upstream `MarkdownPluginCapabilities`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginCapabilities {
    /// The plugin declares inline or block syntax.
    pub syntax: bool,
    /// The plugin declares a transform.
    pub transform: bool,
}

/// Upstream `getMarkdownPluginCapabilities`.
pub fn get_markdown_plugin_capabilities(plugin: &MarkdownPlugin) -> PluginCapabilities {
    PluginCapabilities {
        syntax: plugin.definition.syntax.is_some(),
        transform: plugin.definition.transform.is_some(),
    }
}

/// Upstream `isMarkdownExtensionNode`: whether `node` is `plugin`'s extension node `name`.
pub fn is_markdown_extension_node(node: NodeRef<'_>, plugin: &str, name: &str) -> bool {
    matches!(node, NodeRef::Extension(extension) if extension.is(plugin, name))
}

/// Upstream `validateSyntax`.
fn validate_syntax(name: &str, syntax: &SyntaxCapability) -> Result<(), PluginError> {
    if syntax.inline.is_empty() && syntax.block.is_empty() {
        return fail(format!(
            "\"{name}\" syntax must declare an inline or block contribution"
        ));
    }
    for contribution in syntax.inline.iter().chain(&syntax.block) {
        if contribution.max_span == 0 {
            return fail(format!(
                "\"{name}\" syntax maxSpan must be positive and finite"
            ));
        }
        if contribution
            .starts_with
            .iter()
            .any(|prefix| prefix.is_empty() || prefix.len() > contribution.max_span)
        {
            return fail(format!(
                "\"{name}\" syntax prefixes must be non-empty and bounded"
            ));
        }
    }
    Ok(())
}

/// Upstream `createMarkdownPlugin`: admits a definition, or says why not.
pub fn create_markdown_plugin(definition: PluginDefinition) -> Result<MarkdownPlugin, PluginError> {
    create_entry(definition, EntryRole::Ordinary)
}

pub(crate) fn create_entry(
    definition: PluginDefinition,
    role: EntryRole,
) -> Result<MarkdownPlugin, PluginError> {
    let name = &definition.name;
    if crate::js::is_blank(name) {
        return fail("name must be a non-empty string");
    }
    if definition.api_version != MARKDOWN_PLUGIN_API_VERSION {
        return fail(format!("\"{name}\" uses unsupported apiVersion"));
    }
    if let Some(syntax) = &definition.syntax {
        if definition
            .parse_key
            .as_deref()
            .is_none_or(crate::js::is_blank)
        {
            return fail(format!("\"{name}\" parseKey must be non-empty"));
        }
        validate_syntax(name, syntax)?;
        if definition.renderers.is_empty() {
            return fail(format!("\"{name}\" syntax must provide renderers"));
        }
    } else if definition.transform.is_none() {
        return fail(format!("\"{name}\" must declare syntax or transform"));
    }
    if definition
        .depends_on
        .iter()
        .any(|dependency| crate::js::is_blank(dependency) || dependency == name)
    {
        return fail(format!("\"{name}\" dependsOn must name other plugins"));
    }
    Ok(MarkdownPlugin {
        definition: Rc::new(definition),
        role,
    })
}

/// Upstream `PreparedSyntaxContribution`.
pub(crate) struct PreparedSyntax {
    pub(crate) plugin_name: String,
    pub(crate) contribution: SyntaxContribution,
}

pub(crate) struct PreparedTransform {
    plugin_name: String,
    transform: Transform,
    /// A `depends_on` failure: the transform is skipped with this diagnostic.
    dependency_failure: Option<DiagnosticCode>,
}

/// Upstream `PreparedMarkdownPlugins`: an ordered, admitted plugin list, indexed for the parser
/// and the transform pipeline.
pub struct PreparedMarkdownPlugins {
    entries: Vec<MarkdownPlugin>,
    pub(crate) syntax: Vec<PreparedSyntax>,
    /// Inline contributions by the first character of a prefix, in list order.
    pub(crate) inline_by_first_char: HashMap<char, Vec<usize>>,
    /// Block contributions by the first character of a prefix, in list order.
    pub(crate) block_by_first_char: HashMap<char, Vec<usize>>,
    transforms: Vec<PreparedTransform>,
    /// Whether any transform is plugin-authored and so validated.
    has_untrusted_transform: bool,
    syntax_identity: String,
    renderers: BTreeMap<String, BTreeMap<String, ExtensionRenderer>>,
    /// Whether the source-lines plugin is installed (upstream `module:Markdown/sourceLines`).
    pub(crate) source_lines: bool,
}

impl fmt::Debug for PreparedMarkdownPlugins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedMarkdownPlugins")
            .field(
                "entries",
                &self
                    .entries
                    .iter()
                    .map(MarkdownPlugin::name)
                    .collect::<Vec<_>>(),
            )
            .field("syntax_identity", &self.syntax_identity)
            .finish_non_exhaustive()
    }
}

/// Upstream `prepareMarkdownPlugins`: refuses a list with two entries of one name, and
/// indexes the rest. An empty list prepares to nothing to do, which every entry point
/// treats as omitted (FR12).
pub fn prepare_markdown_plugins(
    plugins: &[MarkdownPlugin],
) -> Result<PreparedMarkdownPlugins, PluginError> {
    prepare(plugins).map_err(|(error, _)| error)
}

/// [`prepare_markdown_plugins`], with the plugin a failure names.
pub(crate) fn prepare(
    plugins: &[MarkdownPlugin],
) -> Result<PreparedMarkdownPlugins, (PluginError, String)> {
    let mut names = HashSet::new();
    let mut prepared = PreparedMarkdownPlugins {
        entries: Vec::with_capacity(plugins.len()),
        syntax: Vec::new(),
        inline_by_first_char: HashMap::new(),
        block_by_first_char: HashMap::new(),
        transforms: Vec::new(),
        has_untrusted_transform: false,
        syntax_identity: String::new(),
        renderers: BTreeMap::new(),
        source_lines: false,
    };
    let mut identity = Vec::new();
    for (index, entry) in plugins.iter().enumerate() {
        let definition = entry.definition();
        if !names.insert(definition.name.as_str()) {
            return Err((
                PluginError::new(format!(
                    "Markdown plugin: duplicate name \"{}\"",
                    definition.name
                )),
                definition.name.clone(),
            ));
        }
        prepared.entries.push(entry.clone());
        if let Some(transform) = &definition.transform {
            let dependency_failure = definition.depends_on.iter().find_map(|dependency| {
                match plugins.iter().position(|other| other.name() == dependency) {
                    None => Some(DiagnosticCode::DependencyMissing),
                    Some(at) if at > index => Some(DiagnosticCode::DependencyMisordered),
                    Some(_) => None,
                }
            });
            if dependency_failure.is_none() && !transform.trusted {
                prepared.has_untrusted_transform = true;
            }
            prepared.transforms.push(PreparedTransform {
                plugin_name: definition.name.clone(),
                transform: transform.clone(),
                dependency_failure,
            });
        }
        if !definition.renderers.is_empty() {
            prepared
                .renderers
                .insert(definition.name.clone(), definition.renderers.clone());
        }
        if matches!(entry.role, EntryRole::SourceLines) {
            prepared.source_lines = true;
            identity.push(format!(
                "{}\0{}\0source-lines",
                definition.name, definition.api_version
            ));
        }
        if let Some(syntax) = &definition.syntax {
            identity.push(format!(
                "{}\0{}\0{}",
                definition.name,
                definition.api_version,
                definition.parse_key.as_deref().unwrap_or_default()
            ));
            for (contributions, display) in [
                (&syntax.inline, ExtensionDisplay::Inline),
                (&syntax.block, ExtensionDisplay::Block),
            ] {
                for contribution in contributions {
                    let at = prepared.syntax.len();
                    prepared.syntax.push(PreparedSyntax {
                        plugin_name: definition.name.clone(),
                        contribution: contribution.clone(),
                    });
                    let index = match display {
                        ExtensionDisplay::Inline => &mut prepared.inline_by_first_char,
                        ExtensionDisplay::Block => &mut prepared.block_by_first_char,
                    };
                    let mut firsts: Vec<char> = Vec::new();
                    for prefix in &contribution.starts_with {
                        if let Some(first) = prefix.chars().next()
                            && !firsts.contains(&first)
                        {
                            firsts.push(first);
                            index.entry(first).or_default().push(at);
                        }
                    }
                }
            }
        }
    }
    prepared.syntax_identity = identity.join("\u{1}");
    Ok(prepared)
}

impl PreparedMarkdownPlugins {
    /// The entries, in order.
    pub fn entries(&self) -> &[MarkdownPlugin] {
        &self.entries
    }

    /// Upstream `syntaxIdentity`: each syntax-bearing plugin's name, API version and parse
    /// key, in order. What a parse cache keys syntax on (FR15).
    pub fn syntax_identity(&self) -> &str {
        &self.syntax_identity
    }

    /// Whether any plugin has a transform.
    pub fn has_transforms(&self) -> bool {
        !self.transforms.is_empty()
    }

    /// Whether `plugin` registered a renderer for nodes named `node_name`.
    pub fn has_renderer(&self, plugin: &str, node_name: &str) -> bool {
        self.renderers
            .get(plugin)
            .is_some_and(|renderers| renderers.contains_key(node_name))
    }

    /// Upstream `getMarkdownExtensionRenderer`: the renderer `node`'s plugin registered for it.
    pub fn renderer(&self, node: &ExtensionNode) -> Option<&ExtensionRenderer> {
        self.renderers.get(&node.plugin)?.get(&node.name)
    }

    pub(crate) fn heading_links_config(
        &self,
    ) -> Option<&crate::plugins::heading_links::HeadingLinksConfig> {
        self.entries.iter().find_map(|entry| match &entry.role {
            EntryRole::HeadingLinks(config) => Some(config.as_ref()),
            _ => None,
        })
    }
}

/// Upstream `markdownExtensionText`: `node` as plain text, its renderer's `to_text`, or its
/// source when it has no renderer or the renderer fails (reported as
/// [`DiagnosticCode::RenderFailed`]).
pub fn markdown_extension_text(
    plugins: Option<&PreparedMarkdownPlugins>,
    node: &ExtensionNode,
    on_diagnostic: Option<&DiagnosticCallback>,
) -> String {
    let diagnostics = Diagnostics::new(on_diagnostic.cloned());
    extension_text(plugins, node, &diagnostics)
}

pub(crate) fn extension_text(
    plugins: Option<&PreparedMarkdownPlugins>,
    node: &ExtensionNode,
    diagnostics: &Diagnostics,
) -> String {
    let fallback = || node.source.clone().unwrap_or_default();
    match plugins.and_then(|plugins| plugins.renderer(node)) {
        None => fallback(),
        Some(renderer) => (renderer.to_text)(node).unwrap_or_else(|_| {
            diagnostics.report(
                &node.plugin,
                PluginPhase::Render,
                DiagnosticCode::RenderFailed,
            );
            fallback()
        }),
    }
}

// --- Destinations ---------------------------------------------------------------------

/// Upstream `normalizeAstDestinations`: every link and image URL in the normalized form the
/// parser stores, so a renderer can use each as it is. The input must be validated.
pub fn normalize_ast_destinations(mut root: Root) -> Root {
    normalize_blocks(&mut root.children);
    root
}

fn normalize_blocks(blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::Heading(node) => normalize_phrasing(&mut node.children),
            Block::Paragraph(node) => normalize_phrasing(&mut node.children),
            Block::Blockquote(node) => normalize_blocks(&mut node.children),
            Block::List(node) => {
                for item in &mut node.children {
                    normalize_blocks(&mut item.children);
                }
            }
            Block::Table(node) => {
                for row in &mut node.children {
                    for cell in &mut row.children {
                        normalize_phrasing(&mut cell.children);
                    }
                }
            }
            Block::Image(node) => node.url = normalized_markdown_destination(&node.url),
            Block::Code(_) | Block::Math(_) | Block::ThematicBreak(_) | Block::Extension(_) => {}
        }
    }
}

/// [`normalize_ast_destinations`] for phrasing nodes.
pub(crate) fn normalize_phrasing(nodes: &mut [Phrasing]) {
    for node in nodes {
        match node {
            Phrasing::Link(link) => {
                link.url = normalized_markdown_destination(&link.url);
                normalize_phrasing(&mut link.children);
            }
            Phrasing::Image(image) => image.url = normalized_markdown_destination(&image.url),
            Phrasing::Strong(parent) | Phrasing::Emphasis(parent) | Phrasing::Delete(parent) => {
                normalize_phrasing(&mut parent.children)
            }
            _ => {}
        }
    }
}

// --- Helper adoption ------------------------------------------------------------------

/// Upstream `adoptMarkdownHelperNodes`: whether phrasing nodes a helper's callback authored
/// may go straight into the helper's output, under the rules validation applies to new
/// nodes: no authored provenance, no foreign or unrenderable extension node, safe
/// destinations, no nested link, and at most 10,000 nodes.
pub(crate) fn adopt_markdown_helper_nodes(
    nodes: &[Phrasing],
    ownership: &HelperOwnership<'_>,
    inside_link: bool,
) -> Result<(), String> {
    let mut count = 0;
    fn visit(
        node: &Phrasing,
        within_link: bool,
        ownership: &HelperOwnership<'_>,
        count: &mut usize,
    ) -> bool {
        *count += 1;
        if *count > 10_000 {
            return false;
        }
        let (position, children, child_link) = match node {
            Phrasing::Text(n) | Phrasing::InlineCode(n) | Phrasing::InlineMath(n) => {
                (n.position, None, within_link)
            }
            Phrasing::Strong(n) | Phrasing::Emphasis(n) | Phrasing::Delete(n) => {
                (n.position, Some(&n.children), within_link)
            }
            Phrasing::Link(n) => {
                if within_link || !is_safe_markdown_parser_url(&n.url) {
                    return false;
                }
                (n.position, Some(&n.children), true)
            }
            Phrasing::Image(n) => {
                if !is_safe_markdown_parser_url(&n.url) {
                    return false;
                }
                (n.position, None, within_link)
            }
            Phrasing::Citation(n) => (n.position, None, within_link),
            Phrasing::Break(n) => (n.position, None, within_link),
            Phrasing::Extension(n) => {
                if n.plugin != ownership.plugin_name
                    || !ownership.has_renderer(&n.name)
                    || n.display != ExtensionDisplay::Inline
                    || n.source.is_some()
                {
                    return false;
                }
                (n.position, None, within_link)
            }
        };
        if position.is_some() {
            return false;
        }
        children.is_none_or(|children| {
            children
                .iter()
                .all(|child| visit(child, child_link, ownership, count))
        })
    }
    if nodes
        .iter()
        .all(|node| visit(node, inside_link, ownership, &mut count))
    {
        Ok(())
    } else {
        Err("Markdown helper callback returned an unrepresentable node".to_string())
    }
}

// --- Validation -----------------------------------------------------------------------

/// Upstream `positionKey`: a source-backed node's identity, its offsets, or (for a nested
/// block while source lines are on) its lines.
fn position_key(position: Option<&Position>) -> Option<String> {
    let position = position?;
    if let (Some(start), Some(end)) = (position.start.offset, position.end.offset) {
        return Some(format!("{start}:{end}"));
    }
    match (position.start.line, position.end.line) {
        (Some(start), Some(end)) => Some(format!("line {start}:{end}")),
        _ => None,
    }
}

/// Upstream `isValidPosition`: both points measure the same things and never run backwards.
fn is_valid_position(position: &Position) -> bool {
    let mut measured = false;
    for (start, end, minimum) in [
        (position.start.offset, position.end.offset, 0),
        (position.start.line, position.end.line, 1),
    ] {
        match (start, end) {
            (None, None) => continue,
            (Some(start), Some(end)) if start >= minimum && end >= start => measured = true,
            _ => return false,
        }
    }
    measured
}

/// Upstream `SourceInvariant`: what a positioned node may not change.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Invariant {
    node_type: NodeType,
    start_line: Option<usize>,
    end_line: Option<usize>,
    depth: Option<u8>,
    ordered: Option<bool>,
    start: Option<i64>,
    delimiter: Option<char>,
    plugin: Option<String>,
    name: Option<String>,
    display: Option<ExtensionDisplay>,
}

pub(crate) fn node_position<'a>(node: NodeRef<'a>) -> Option<&'a Position> {
    match node {
        NodeRef::Root(n) => n.position.as_ref(),
        NodeRef::Text(n) | NodeRef::InlineCode(n) | NodeRef::InlineMath(n) | NodeRef::Math(n) => {
            n.position.as_ref()
        }
        NodeRef::Strong(n) | NodeRef::Emphasis(n) | NodeRef::Delete(n) => n.position.as_ref(),
        NodeRef::Break(n) | NodeRef::ThematicBreak(n) => n.position.as_ref(),
        NodeRef::Link(n) => n.position.as_ref(),
        NodeRef::Image(n) => n.position.as_ref(),
        NodeRef::Citation(n) => n.position.as_ref(),
        NodeRef::Heading(n) => n.position.as_ref(),
        NodeRef::Paragraph(n) => n.position.as_ref(),
        NodeRef::Code(n) => n.position.as_ref(),
        NodeRef::Blockquote(n) => n.position.as_ref(),
        NodeRef::List(n) => n.position.as_ref(),
        NodeRef::ListItem(n) => n.position.as_ref(),
        NodeRef::Table(n) => n.position.as_ref(),
        NodeRef::TableRow(n) => n.position.as_ref(),
        NodeRef::TableCell(n) => n.position.as_ref(),
        NodeRef::Extension(n) => n.position.as_ref(),
    }
}

fn invariant(node: NodeRef<'_>, position: &Position) -> Invariant {
    let mut invariant = Invariant {
        node_type: node.node_type(),
        start_line: position.start.line,
        end_line: position.end.line,
        depth: None,
        ordered: None,
        start: None,
        delimiter: None,
        plugin: None,
        name: None,
        display: None,
    };
    match node {
        NodeRef::Heading(n) => invariant.depth = Some(n.depth),
        NodeRef::List(n) => {
            invariant.ordered = Some(n.ordered);
            invariant.start = n.start;
            invariant.delimiter = n.delimiter;
        }
        NodeRef::Extension(n) => {
            invariant.plugin = Some(n.plugin.clone());
            invariant.name = Some(n.name.clone());
            invariant.display = Some(n.display);
        }
        _ => {}
    }
    invariant
}

fn walk<'a>(node: NodeRef<'a>, visit: &mut dyn FnMut(NodeRef<'a>)) {
    visit(node);
    for child in node.children() {
        walk(child, visit);
    }
}

/// Upstream `collectSourceInvariants`.
fn collect_source_invariants(root: &Root) -> HashMap<String, Vec<Invariant>> {
    let mut invariants: HashMap<String, Vec<Invariant>> = HashMap::new();
    walk(NodeRef::Root(root), &mut |node| {
        let position = node_position(node);
        if let (Some(key), Some(position)) = (position_key(position), position) {
            invariants
                .entry(key)
                .or_default()
                .push(invariant(node, position));
        }
    });
    invariants
}

/// Upstream `extensionSignature`.
fn extension_signature(node: &ExtensionNode) -> String {
    serde_json::to_string(&(
        &node.plugin,
        &node.name,
        node.display,
        &node.data,
        &node.source,
        position_key(node.position.as_ref()),
    ))
    .unwrap_or_default()
}

/// Upstream `collectExtensionSignatures`: each extension node's signature, its plugin, and
/// how many nodes carry it.
fn collect_extension_signatures(root: &Root) -> HashMap<String, (String, usize)> {
    let mut signatures: HashMap<String, (String, usize)> = HashMap::new();
    walk(NodeRef::Root(root), &mut |node| {
        if let NodeRef::Extension(extension) = node {
            let entry = signatures
                .entry(extension_signature(extension))
                .or_insert_with(|| (extension.plugin.clone(), 0));
            entry.0 = extension.plugin.clone();
            entry.1 += 1;
        }
    });
    signatures
}

fn is_phrasing_parent(node_type: NodeType) -> bool {
    matches!(
        node_type,
        NodeType::Heading
            | NodeType::Paragraph
            | NodeType::Strong
            | NodeType::Emphasis
            | NodeType::Delete
            | NodeType::Link
            | NodeType::TableCell
    )
}

fn is_block_parent(node_type: NodeType) -> bool {
    matches!(
        node_type,
        NodeType::Root | NodeType::Blockquote | NodeType::ListItem
    )
}

/// Upstream `validateAst`'s state.
struct Validator<'a> {
    plugins: &'a PreparedMarkdownPlugins,
    positions: HashMap<String, Vec<Invariant>>,
    /// The run's source headings: each one's parsed depth, by index.
    run: u64,
    heading_depths: &'a [u8],
    seen_headings: HashSet<usize>,
    active_plugin: &'a str,
    existing_extensions: HashMap<String, (String, usize)>,
    count: usize,
}

impl Validator<'_> {
    fn visit(&mut self, node: NodeRef<'_>, parent: Option<NodeType>, inside_link: bool) -> bool {
        self.count += 1;
        if self.count > 100_000 {
            return false;
        }
        let node_type = node.node_type();
        if let Some(parent) = parent {
            if let NodeRef::Extension(extension) = node {
                let display = extension.display;
                if (is_phrasing_parent(parent) && display != ExtensionDisplay::Inline)
                    || (is_block_parent(parent) && display != ExtensionDisplay::Block)
                {
                    return false;
                }
            }
            if inside_link && node_type == NodeType::Link {
                return false;
            }
        }
        let position = node_position(node);
        if position.is_some_and(|position| !is_valid_position(position)) {
            return false;
        }
        if let (Some(key), Some(position)) = (position_key(position), position) {
            let expected = invariant(node, position);
            let Some(invariants) = self.positions.get_mut(&key) else {
                return false;
            };
            let Some(index) = invariants.iter().position(|known| *known == expected) else {
                return false;
            };
            invariants.remove(index);
        }
        match node {
            NodeRef::Root(_) if parent.is_some() => return false,
            NodeRef::Heading(heading) => {
                if !(1..=6).contains(&heading.depth) {
                    return false;
                }
                if let Some(SourceHeading { run, index, depth }) = heading.identity.0
                    && (run != self.run
                        || self.heading_depths.get(index) != Some(&depth)
                        || depth != heading.depth
                        || !self.seen_headings.insert(index))
                {
                    return false;
                }
            }
            NodeRef::Link(link) if !is_safe_markdown_parser_url(&link.url) => return false,
            NodeRef::Image(image) if !is_safe_markdown_parser_url(&image.url) => return false,
            NodeRef::List(list) if list.delimiter.is_some_and(|d| d != '.' && d != ')') => {
                return false;
            }
            NodeRef::Extension(extension) => {
                if !self
                    .plugins
                    .entries
                    .iter()
                    .any(|entry| entry.name() == extension.plugin)
                    || !self
                        .plugins
                        .has_renderer(&extension.plugin, &extension.name)
                {
                    return false;
                }
                let signature = extension_signature(extension);
                match self.existing_extensions.get_mut(&signature) {
                    Some((_, count)) if *count > 0 => *count -= 1,
                    _ => {
                        if extension.plugin != self.active_plugin
                            || extension.source.is_some()
                            || extension.position.is_some()
                        {
                            return false;
                        }
                    }
                }
            }
            _ => {}
        }
        let inside_link = inside_link || node_type == NodeType::Link;
        node.children()
            .into_iter()
            .all(|child| self.visit(child, Some(node_type), inside_link))
    }
}

/// The source headings' identities, so a plugin-authored transform cannot re-level, forge
/// or duplicate one (upstream `markSourceHeadings`). Returns each heading's depth by index.
fn mark_source_headings(blocks: &mut [Block], run: u64, depths: &mut Vec<u8>) {
    for block in blocks {
        match block {
            Block::Heading(heading) => {
                if heading.identity.0.is_none() {
                    heading.identity.0 = Some(SourceHeading {
                        run,
                        index: depths.len(),
                        depth: heading.depth,
                    });
                    depths.push(heading.depth);
                }
            }
            Block::Blockquote(quote) => mark_source_headings(&mut quote.children, run, depths),
            Block::List(list) => {
                for item in &mut list.children {
                    mark_source_headings(&mut item.children, run, depths);
                }
            }
            _ => {}
        }
    }
}

fn for_each_code_mut(blocks: &mut [Block], visit: &mut dyn FnMut(&mut Code)) {
    for block in blocks {
        match block {
            Block::Code(code) => visit(code),
            Block::Blockquote(quote) => for_each_code_mut(&mut quote.children, visit),
            Block::List(list) => {
                for item in &mut list.children {
                    for_each_code_mut(&mut item.children, visit);
                }
            }
            _ => {}
        }
    }
}

/// Upstream `codeSourceSignature`.
fn code_signature(code: &Code) -> String {
    serde_json::to_string(&(&code.lang, &code.meta, &code.value, &code.position))
        .unwrap_or_default()
}

/// Upstream `restoreCodeInternalProperties`: a code node a plugin-authored transform rebuilt
/// keeps the fence proposal its source code node had, matched by language, meta, value
/// and position in document order. (Upstream also restores the released language
/// projection; the port derives that from `lang`, which the match holds fixed.)
fn restore_code_proposals(previous: &Root, next: &mut Root) {
    let mut queues: HashMap<String, VecDeque<Option<crate::ast::FenceProposal>>> = HashMap::new();
    let mut previous = previous.clone();
    for_each_code_mut(&mut previous.children, &mut |code| {
        let legacy = code.legacy_language().map(str::to_string);
        if code.proposal.node().is_some() || legacy != code.lang {
            queues
                .entry(code_signature(code))
                .or_default()
                .push_back(code.proposal.node().map(|_| code.proposal.clone()));
        }
    });
    if queues.is_empty() {
        return;
    }
    for_each_code_mut(&mut next.children, &mut |code| {
        if let Some(Some(Some(proposal))) = queues
            .get_mut(&code_signature(code))
            .map(VecDeque::pop_front)
            && code.proposal != proposal
        {
            code.proposal = proposal;
        }
    });
}

static RUNS: AtomicU64 = AtomicU64::new(1);

/// Upstream `applyMarkdownTransforms`: runs every transform in plugin order. A transform that
/// fails, or whose plugin-authored output fails validation, is reported and leaves the
/// last valid document in place.
pub(crate) fn apply_markdown_transforms(
    root: Root,
    plugins: &PreparedMarkdownPlugins,
    source: &str,
    is_final: bool,
    display: ExtensionDisplay,
    diagnostics: &Diagnostics,
) -> Root {
    if plugins.transforms.is_empty() {
        return root;
    }
    let mut document = root;
    let run = RUNS.fetch_add(1, Ordering::Relaxed);
    let mut heading_depths = Vec::new();
    if plugins.has_untrusted_transform {
        mark_source_headings(&mut document.children, run, &mut heading_depths);
    }
    let mut prior_transform_changed_tree = false;
    for prepared in &plugins.transforms {
        if let Some(code) = prepared.dependency_failure {
            diagnostics.report(&prepared.plugin_name, PluginPhase::Transform, code);
            continue;
        }
        let transform = &prepared.transform;
        if !prior_transform_changed_tree
            && transform
                .claims
                .as_ref()
                .is_some_and(|claims| !claims(source))
        {
            continue;
        }
        let context = TransformContext {
            source,
            is_final,
            display,
            owner: Some(Owner {
                plugin_name: &prepared.plugin_name,
                plugins,
                source_unchanged: !prior_transform_changed_tree,
                diagnostics,
            }),
        };
        let next = match transform.run(&document, &context) {
            Err(_) => {
                diagnostics.report(
                    &prepared.plugin_name,
                    PluginPhase::Transform,
                    DiagnosticCode::TransformFailed,
                );
                continue;
            }
            Ok(None) => continue,
            Ok(Some(next)) => next,
        };
        if transform.trusted {
            prior_transform_changed_tree = true;
            document = next;
            continue;
        }
        let mut validator = Validator {
            plugins,
            positions: collect_source_invariants(&document),
            run,
            heading_depths: &heading_depths,
            seen_headings: HashSet::new(),
            active_plugin: &prepared.plugin_name,
            existing_extensions: collect_extension_signatures(&document),
            count: 0,
        };
        let valid = validator.visit(NodeRef::Root(&next), None, false)
            && validator
                .existing_extensions
                .values()
                .all(|(plugin, count)| *plugin == prepared.plugin_name || *count == 0)
            && (display == ExtensionDisplay::Block
                || matches!(next.children.as_slice(), [Block::Paragraph(_)]));
        if !valid {
            diagnostics.report(
                &prepared.plugin_name,
                PluginPhase::Transform,
                DiagnosticCode::InvalidDocument,
            );
            continue;
        }
        prior_transform_changed_tree = true;
        let mut next = normalize_ast_destinations(next);
        restore_code_proposals(&document, &mut next);
        document = next;
    }
    document
}
