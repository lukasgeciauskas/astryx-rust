//! Keyboard navigation in a grid of cells, upstream `useGridFocus`: the arrow keys move by
//! one cell or one row, Home and End to the ends of the row (with Ctrl or Cmd, of the
//! grid), skipping cells that take no focus without collapsing the grid's geometry; past
//! the first or last cell the owner's `before` and `after` callbacks run. Right to left,
//! the horizontal arrows swap so they follow the visual direction.
//!
//! EmojiPicker and ChatEmojiPicker share it: the cells are every position of the grid in
//! order, padding included, each focusable or not.

use gpui::Keystroke;

/// A key the grid handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridKey {
    Left,
    Right,
    Up,
    Down,
    /// Home: the first focusable cell of the row; with Ctrl or Cmd, of the grid.
    Home {
        grid: bool,
    },
    /// End: the last focusable cell of the row; with Ctrl or Cmd, of the grid.
    End {
        grid: bool,
    },
    PageUp,
    PageDown,
}

impl GridKey {
    /// The grid key a keystroke is, if any.
    pub fn of(keystroke: &Keystroke) -> Option<Self> {
        let grid = keystroke.modifiers.control || keystroke.modifiers.platform;
        Some(match keystroke.key.as_str() {
            "left" => GridKey::Left,
            "right" => GridKey::Right,
            "up" => GridKey::Up,
            "down" => GridKey::Down,
            "home" => GridKey::Home { grid },
            "end" => GridKey::End { grid },
            "pageup" => GridKey::PageUp,
            "pagedown" => GridKey::PageDown,
            _ => return None,
        })
    }
}

/// What a key does in the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridMove {
    /// Focus the cell at this index.
    Focus(usize),
    /// Navigation would go before the first cell (upstream `onNavigateBefore(column,
    /// offset)`).
    Before {
        column: usize,
        offset: usize,
    },
    /// Navigation would go after the last cell (upstream `onNavigateAfter`).
    After {
        column: usize,
        offset: usize,
    },
    /// Page Up or Page Down (upstream `onPageUp` / `onPageDown`).
    PageUp,
    PageDown,
    /// The key is handled and moves nothing.
    Stay,
}

/// The next focusable cell from `start`, moving by `step` (upstream
/// `findFocusableInDirection`); none past either end.
pub fn find_focusable_in_direction(focusable: &[bool], start: isize, step: isize) -> Option<usize> {
    let mut index = start;
    while index >= 0 && (index as usize) < focusable.len() {
        if focusable[index as usize] {
            return Some(index as usize);
        }
        index += step;
    }
    None
}

/// The first focusable cell (upstream `focusFirst`).
pub fn first_focusable(focusable: &[bool]) -> Option<usize> {
    find_focusable_in_direction(focusable, 0, 1)
}

/// The last focusable cell (upstream `focusLast`).
pub fn last_focusable(focusable: &[bool]) -> Option<usize> {
    find_focusable_in_direction(focusable, focusable.len() as isize - 1, -1)
}

/// What `key` does with focus on cell `current` of a grid `columns` wide (upstream
/// `handleKeyDown`). `focusable` holds every cell in order. `None` for an empty grid or
/// a `current` outside it, where upstream handles nothing.
pub fn grid_move(
    focusable: &[bool],
    current: usize,
    columns: usize,
    key: GridKey,
    rtl: bool,
) -> Option<GridMove> {
    if focusable.is_empty() || current >= focusable.len() || columns == 0 {
        return None;
    }
    let (row, col) = (current / columns, current % columns);
    let total_rows = focusable.len().div_ceil(columns);
    let c = current as isize;
    let cols = columns as isize;
    // In RTL, Left and Right swap so horizontal navigation follows visual direction.
    let key = match (key, rtl) {
        (GridKey::Left, true) => GridKey::Right,
        (GridKey::Right, true) => GridKey::Left,
        (key, _) => key,
    };
    let focus_or =
        |target: Option<usize>, otherwise: GridMove| target.map_or(otherwise, GridMove::Focus);
    Some(match key {
        GridKey::Right => focus_or(
            find_focusable_in_direction(focusable, c + 1, 1),
            GridMove::After {
                column: (col + 1) % columns,
                offset: 1,
            },
        ),
        GridKey::Left => focus_or(
            find_focusable_in_direction(focusable, c - 1, -1),
            GridMove::Before {
                column: if col == 0 { columns - 1 } else { col - 1 },
                offset: 1,
            },
        ),
        GridKey::Down => {
            let after = GridMove::After {
                column: col,
                offset: columns,
            };
            if row + 1 < total_rows {
                focus_or(
                    find_focusable_in_direction(focusable, c + cols, cols),
                    after,
                )
            } else {
                after
            }
        }
        GridKey::Up => {
            let before = GridMove::Before {
                column: col,
                offset: columns,
            };
            if row > 0 {
                focus_or(
                    find_focusable_in_direction(focusable, c - cols, -cols),
                    before,
                )
            } else {
                before
            }
        }
        GridKey::Home { grid: true } => {
            first_focusable(focusable).map_or(GridMove::Stay, GridMove::Focus)
        }
        GridKey::End { grid: true } => {
            last_focusable(focusable).map_or(GridMove::Stay, GridMove::Focus)
        }
        GridKey::Home { grid: false } => {
            let start = row * columns;
            let end = (start + columns - 1).min(focusable.len() - 1);
            match find_focusable_in_direction(focusable, start as isize, 1) {
                Some(t) if t <= end => GridMove::Focus(t),
                _ => GridMove::Stay,
            }
        }
        GridKey::End { grid: false } => {
            let start = row * columns;
            let end = (start + columns - 1).min(focusable.len() - 1);
            match find_focusable_in_direction(focusable, end as isize, -1) {
                Some(t) if t >= start => GridMove::Focus(t),
                _ => GridMove::Stay,
            }
        }
        GridKey::PageUp => GridMove::PageUp,
        GridKey::PageDown => GridMove::PageDown,
    })
}
