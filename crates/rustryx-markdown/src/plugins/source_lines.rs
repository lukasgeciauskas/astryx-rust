//! The source-lines plugin, upstream `Markdown/plugins/sourceLines.ts`: a parse option, not
//! syntax. Installed, it makes the parse Markdown already runs record the 1-based,
//! inclusive source lines of every block (nested blocks and list items included) as
//! `position` lines; without it no line work happens. It enters parse identity.

use super::{EntryRole, MarkdownPlugin, PluginDefinition, Transform, create_entry};

/// Upstream `markdownSourceLinesPlugin` (`source-lines`, upstream
/// `createMarkdownSourceLinesEntry`): its transform is a trusted identity that claims no
/// source, so the pipeline skips it.
pub fn markdown_source_lines_plugin() -> MarkdownPlugin {
    create_entry(
        PluginDefinition {
            transform: Some(
                Transform::new(|_, _| Ok(None))
                    .trusted()
                    .with_claim(|_| false),
            ),
            ..PluginDefinition::new("source-lines")
        },
        EntryRole::SourceLines,
    )
    .expect("the source-lines plugin is valid")
}
