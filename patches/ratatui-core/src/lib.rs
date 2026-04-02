// Re-export the custom ratatui-core fork so that crates depending on the
// crates.io ratatui-core (e.g. ansi-to-tui) work with the same types as the
// rest of the project, which uses the Marlinski ratatui fork.
pub use ratatui_core_impl::*;
