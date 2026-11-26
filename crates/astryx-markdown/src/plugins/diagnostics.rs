//! The plugin diagnostic channel, upstream `Markdown/plugins/protocol.ts`
//! (`reportMarkdownPluginFailure`) as the component spec's FR19 states it
//! (`onPluginDiagnostic`): one source-free event per failure, advisory or degradation,
//! naming the plugin, the phase, a stable code and a severity, rate-limited with a
//! suppression code.
//!
//! Upstream's code reports through `warnOnce` (development) and one `console.error` per
//! plugin and phase (production), with a readable sentence. A library has no console, so
//! the port delivers the same facts as data to the caller's callback, and only those:
//! no event carries document source, a tokenizer's or transform's error text, or a
//! message a transform reported. The codes are this port's names for upstream's failure
//! sites; each is documented where it is raised.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;

/// The caller's diagnostic callback (`ParseOptions::on_plugin_diagnostic`).
pub type DiagnosticCallback = Rc<dyn Fn(&PluginDiagnostic)>;

/// How many distinct events one parse delivers before it delivers one
/// [`DiagnosticCode::Suppressed`] event and goes quiet.
pub const MAX_DIAGNOSTICS_PER_PARSE: usize = 16;

/// The plugin name a configuration failure that names no one plugin carries, as
/// upstream's Outline reports a failed preparation (`'configuration'`).
pub const CONFIGURATION_PLUGIN: &str = "configuration";

/// The phase a diagnostic happened in: upstream's `'syntax' | 'transform' | 'render'`,
/// and preparing the plugin list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PluginPhase {
    /// Admitting and ordering the plugin list (`prepareMarkdownPlugins`).
    Configuration,
    /// A tokenizer, during the parse.
    Syntax,
    /// A transform, after the parse.
    Transform,
    /// A renderer's text projection (`toText`).
    Render,
}

impl PluginPhase {
    /// The phase's name.
    pub fn as_str(self) -> &'static str {
        match self {
            PluginPhase::Configuration => "configuration",
            PluginPhase::Syntax => "syntax",
            PluginPhase::Transform => "transform",
            PluginPhase::Render => "render",
        }
    }
}

/// How bad a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    /// Something failed and Markdown fell back: the last valid document, the readable
    /// source, or no plugins at all.
    Error,
    /// Advice: a transform reported a problem it handled, or diagnostics were suppressed.
    Warning,
}

/// A diagnostic's stable code. [`DiagnosticCode::as_str`] is the wire form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// Two entries share a name; the parse ran without plugins (upstream throws
    /// `duplicate name`).
    DuplicateName,
    /// A plugin's `depends_on` names a plugin that is not installed; its transform did
    /// not run.
    DependencyMissing,
    /// A plugin's `depends_on` names a plugin installed after it; its transform did not
    /// run.
    DependencyMisordered,
    /// A tokenizer returned an error (upstream: it threw).
    TokenizerFailed,
    /// A tokenizer's match was not a valid node of its own (upstream `Tokenizer returned
    /// an invalid extension node`).
    InvalidExtensionNode,
    /// A block tokenizer's match ended inside a line (upstream `Block syntax must
    /// consume complete lines`).
    IncompleteBlockLines,
    /// A transform returned an error (upstream: it threw).
    TransformFailed,
    /// A plugin-authored transform returned a document that failed validation (upstream
    /// `Transform returned an invalid Markdown document`).
    InvalidDocument,
    /// A transform reported a problem through its context (upstream
    /// `context.report`).
    TransformReport,
    /// A renderer's `to_text` returned an error; the node's source stood in.
    RenderFailed,
    /// This parse had more distinct diagnostics than [`MAX_DIAGNOSTICS_PER_PARSE`]; the
    /// rest were dropped.
    Suppressed,
}

impl DiagnosticCode {
    /// The code's stable name.
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::DuplicateName => "duplicate-name",
            DiagnosticCode::DependencyMissing => "dependency-missing",
            DiagnosticCode::DependencyMisordered => "dependency-misordered",
            DiagnosticCode::TokenizerFailed => "tokenizer-failed",
            DiagnosticCode::InvalidExtensionNode => "invalid-extension-node",
            DiagnosticCode::IncompleteBlockLines => "incomplete-block-lines",
            DiagnosticCode::TransformFailed => "transform-failed",
            DiagnosticCode::InvalidDocument => "invalid-document",
            DiagnosticCode::TransformReport => "transform-report",
            DiagnosticCode::RenderFailed => "render-failed",
            DiagnosticCode::Suppressed => "diagnostics-suppressed",
        }
    }

    /// The severity every event with this code carries.
    pub fn severity(self) -> DiagnosticSeverity {
        match self {
            DiagnosticCode::TransformReport | DiagnosticCode::Suppressed => {
                DiagnosticSeverity::Warning
            }
            _ => DiagnosticSeverity::Error,
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One source-free plugin diagnostic (FR19).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginDiagnostic {
    /// The plugin's name ([`CONFIGURATION_PLUGIN`] when none is to blame).
    pub plugin: String,
    pub phase: PluginPhase,
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
}

impl fmt::Display for PluginDiagnostic {
    /// Upstream's sentence, without any text from the document or the plugin.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Markdown: plugin \"{}\" failed in {} ({}); rendered readable fallback.",
            self.plugin,
            self.phase.as_str(),
            self.code.as_str()
        )
    }
}

/// One parse's diagnostic sink: the caller's callback, deduplicated by plugin, phase and
/// code (upstream's `warnOnce` key is plugin and phase), and capped.
#[derive(Default)]
pub(crate) struct Diagnostics {
    callback: Option<DiagnosticCallback>,
    seen: RefCell<HashSet<(String, PluginPhase, DiagnosticCode)>>,
    suppressed: Cell<bool>,
}

impl Diagnostics {
    pub(crate) fn new(callback: Option<DiagnosticCallback>) -> Rc<Self> {
        Rc::new(Diagnostics {
            callback,
            ..Diagnostics::default()
        })
    }

    /// Reports `code` against `plugin` in `phase`, once per parse.
    pub(crate) fn report(&self, plugin: &str, phase: PluginPhase, code: DiagnosticCode) {
        let Some(callback) = &self.callback else {
            return;
        };
        if self.suppressed.get() {
            return;
        }
        let mut seen = self.seen.borrow_mut();
        let key = (plugin.to_string(), phase, code);
        if seen.contains(&key) {
            return;
        }
        if seen.len() >= MAX_DIAGNOSTICS_PER_PARSE {
            self.suppressed.set(true);
            drop(seen);
            callback(&PluginDiagnostic {
                plugin: CONFIGURATION_PLUGIN.to_string(),
                phase,
                code: DiagnosticCode::Suppressed,
                severity: DiagnosticCode::Suppressed.severity(),
            });
            return;
        }
        seen.insert(key);
        drop(seen);
        callback(&PluginDiagnostic {
            plugin: plugin.to_string(),
            phase,
            code,
            severity: code.severity(),
        });
    }
}
