//! Markdown plugins in the UI layer, upstream `plugin-renderer/MarkdownPluginNodeRenderer.tsx`:
//! the render closure a plugin's renderer carries, and rendering one extension node the
//! way Markdown does, with its readable text when the renderer fails.

use std::any::Any;
use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, RenderOnce, SharedString, Window};
use rustryx_markdown::ast::ExtensionNode;
use rustryx_markdown::plugins::diagnostics::{
    DiagnosticCallback, DiagnosticCode, PluginDiagnostic, PluginPhase,
};
use rustryx_markdown::plugins::{
    MarkdownPlugin, PreparedMarkdownPlugins, markdown_extension_text, prepare_markdown_plugins,
};

/// A plugin's element for one extension node. An `Err` is upstream's thrown render:
/// Markdown shows the node's readable text instead and reports `render-failed`.
pub type MarkdownExtensionRender =
    dyn Fn(&ExtensionNode, &mut Window, &mut App) -> Result<AnyElement, String>;

/// The UI half of a plugin's renderer, for
/// `rustryx_markdown::plugins::ExtensionRenderer::with_render`.
///
/// ```ignore
/// ExtensionRenderer::new(|node| Ok(format!("@{}", node.data["label"])))
///     .with_render(markdown_extension_render(|node, _, _| {
///         Ok(Badge::new(node.data["label"].as_str().unwrap_or("").to_string()).into_any_element())
///     }))
/// ```
pub fn markdown_extension_render(
    render: impl Fn(&ExtensionNode, &mut Window, &mut App) -> Result<AnyElement, String> + 'static,
) -> Rc<dyn Any> {
    let render: Rc<MarkdownExtensionRender> = Rc::new(render);
    Rc::new(render)
}

/// Reports a render failure on the diagnostic channel (FR19).
pub(crate) fn report_render_failure(callback: Option<&DiagnosticCallback>, plugin: &str) {
    if let Some(callback) = callback {
        callback(&PluginDiagnostic {
            plugin: plugin.to_string(),
            phase: PluginPhase::Render,
            code: DiagnosticCode::RenderFailed,
            severity: DiagnosticCode::RenderFailed.severity(),
        });
    }
}

/// Upstream `renderMarkdownPluginNode`: the plugin's element for `node`, or `fallback`
/// when no renderer owns it or the renderer fails.
pub(crate) fn render_plugin_node(
    plugins: Option<&PreparedMarkdownPlugins>,
    node: &ExtensionNode,
    fallback: impl FnOnce() -> AnyElement,
    on_diagnostic: Option<&DiagnosticCallback>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let render = plugins
        .and_then(|plugins| plugins.renderer(node))
        .and_then(|renderer| renderer.render.as_ref())
        .and_then(|render| render.downcast_ref::<Rc<MarkdownExtensionRender>>())
        .cloned();
    let Some(render) = render else {
        return fallback();
    };
    match render(node, window, cx) {
        Ok(element) => element,
        Err(_) => {
            report_render_failure(on_diagnostic, &node.plugin);
            fallback()
        }
    }
}

/// Upstream `MarkdownPluginNodeRenderer`: one parsed extension node, rendered exactly as
/// Markdown renders it, with no element of its own. Without a renderer it shows the
/// node's source, or its plugin's text projection for a node with no source.
#[derive(IntoElement)]
pub struct MarkdownPluginNodeRenderer {
    plugins: Vec<MarkdownPlugin>,
    node: ExtensionNode,
}

impl MarkdownPluginNodeRenderer {
    /// Upstream `plugins` (as passed to Markdown) and `node` (from a parse with them).
    pub fn new(plugins: impl IntoIterator<Item = MarkdownPlugin>, node: ExtensionNode) -> Self {
        MarkdownPluginNodeRenderer {
            plugins: plugins.into_iter().collect(),
            node,
        }
    }
}

impl RenderOnce for MarkdownPluginNodeRenderer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let prepared = prepare_markdown_plugins(&self.plugins).ok();
        let text = self
            .node
            .source
            .clone()
            .unwrap_or_else(|| markdown_extension_text(prepared.as_ref(), &self.node, None));
        render_plugin_node(
            prepared.as_ref(),
            &self.node,
            || SharedString::from(text).into_any_element(),
            None,
            window,
            cx,
        )
    }
}
