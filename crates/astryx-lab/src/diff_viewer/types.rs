//! DiffViewer's data model (upstream lab `DiffViewer/diffTypes.ts`): what
//! [`super::parse_unified_diff`] and [`super::create_diff_file`] produce and
//! [`super::DiffViewer`] renders.

use gpui::SharedString;

/// Upstream `DiffViewerLineKind`: what one line of a hunk does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiffViewerLineKind {
    Added,
    Removed,
    Context,
}

/// Upstream `DiffViewerFileStatus`: how the file changed as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiffViewerFileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
}

/// Upstream `DiffViewerSide`: which version a line number belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiffViewerSide {
    /// The original (left in split view).
    Old,
    /// The changed version (right in split view).
    New,
}

impl DiffViewerSide {
    /// Upstream's name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }
}

/// Upstream `DiffViewerLine`: one line of a hunk, without its leading glyph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffViewerLine {
    pub kind: DiffViewerLineKind,
    /// The line's text with the diff glyph removed.
    pub content: SharedString,
    /// 1-based line number in the old file; `None` for an added line.
    pub old_line_number: Option<u64>,
    /// 1-based line number in the new file; `None` for a removed line.
    pub new_line_number: Option<u64>,
}

/// Upstream `DiffViewerHunk`: one `@@` hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffViewerHunk {
    /// The hunk header as written, e.g. `@@ -12,7 +12,9 @@ function save()`.
    pub header: SharedString,
    pub old_start: u64,
    pub old_lines: u64,
    pub new_start: u64,
    pub new_lines: u64,
    pub lines: Vec<DiffViewerLine>,
    /// Upstream `isReplacedBlock`: the change was too large to compare line by line,
    /// so the hunk lists every old line as removed and every new line as added;
    /// DiffViewer folds them behind a labeled control.
    pub is_replaced_block: bool,
}

/// Upstream `DiffViewerFile`: one file's changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffViewerFile {
    /// The path before the change; `None` when the file was added.
    pub old_path: Option<SharedString>,
    /// The path after the change; `None` when the file was deleted.
    pub new_path: Option<SharedString>,
    pub status: DiffViewerFileStatus,
    /// The patch reports a binary change with no line hunks.
    pub is_binary: bool,
    pub hunks: Vec<DiffViewerHunk>,
}

/// Upstream `DiffViewerLineRef`: one line of a rendered diff, for annotations and
/// line-number clicks. An added line is `New`, a removed line `Old`, a context line
/// `New`: the same in unified and split view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffViewerLineRef {
    pub side: DiffViewerSide,
    pub line_number: u64,
    pub line: DiffViewerLine,
    pub hunk_index: usize,
}
