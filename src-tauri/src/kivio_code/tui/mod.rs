//! `kivio-code` terminal UI library (Phase 4).
//!
//! This module will hold the Rust TUI library for the terminal coding agent: a
//! crossterm-based terminal layer (raw I/O + key decoding + Kitty protocol
//! negotiation) and a PI-style differential line renderer
//! (`Component.render(width) -> Vec<String>`, with frame-to-frame diffing to
//! minimize ANSI output), plus the component set — Text/Box/Spacer, Input,
//! SelectList, Editor, Loader, Markdown, overlays, SettingsList, and themes.
//! Deliberately avoids ratatui's cell-grid model.
//!
//! Currently a scaffolding stub: no real types are defined yet.

#[cfg(test)]
mod tests {
    #[test]
    fn stub() {}
}
