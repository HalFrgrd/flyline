You are reviewing the documentation of a Rust project called Flyline.

Check the following documentation sources for consistency with the actual source code:
- README.md (and any other README files)
- Doc comments in src/lib.rs that clap uses to generate CLI help messages
  (look for #[arg(...)], #[command(...)], and /// comments on FlylineArgs fields and Commands variants)
- Shell scripts in examples/

The source code is the source of truth. Specifically verify:
1. Default values mentioned in the documentation match the actual defaults in the code
   (e.g. frame rate, FPS, mouse mode defaults, clap default_value attributes).
2. Keyboard shortcuts and key bindings described in the docs match those handled in the source code.
3. Command-line flag names and descriptions in the README are consistent with the clap definitions in src/lib.rs.
4. Example commands and snippets in README.md and examples/ use correct flag names and syntax.
5. Feature descriptions in the documentation accurately reflect the current implementation.

Fix any inconsistencies you find by editing the documentation files directly.
Do not modify any Rust source code — only update documentation (README.md files, doc comments, and example scripts).
If everything is already consistent, make no changes.
There might not be any changes to make since you have run on previous events.

Find any typos or grammatical errors in the documentation and fix those as well.

After checking, write a brief summary of all changes you made (or confirm no changes were needed) to a file called copilot_summary.md.
