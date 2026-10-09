//! Modifier keys that change how a click or a navigation key affects a selection.

/// Which modifiers were held for a selection gesture, independent of any GUI toolkit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectModifiers {
    /// Ctrl (Cmd on macOS): toggle the item instead of replacing the selection.
    pub toggle: bool,
    /// Shift: select the range from the pivot to the item.
    pub range: bool,
}
