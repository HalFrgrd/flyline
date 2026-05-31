//! Parse a `--help` string into a [`Command`] structure.
//!
//! The entry point is [`parse_help`].  It tries to identify which help format
//! the text comes from (clap, Python argparse, or an unknown generic format)
//! and dispatches to the appropriate sub-parser.
use anyhow::Context;

mod parse_help;
pub mod parse_man;

pub use parse_help::{parse_help, parse_help_argparse, parse_help_clap, parse_help_generic};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SynthesisStrategy {
    #[default]
    ManPageThenRunHelp,
    ManPage,
    RunHelp,
}

// ──────────────────────────────────────────────────────────────────────────────
// Public data structures
// ──────────────────────────────────────────────────────────────────────────────

/// A single command-line argument / flag.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Arg {
    /// Long flag name, e.g. `--verbose`.
    pub long: Option<String>,
    /// Short flag name, e.g. `-v`.
    pub short: Option<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Meta-variable / value type hint (e.g. `<PATH>`, `<N>`).
    pub value_type: Option<String>,
    /// Number of values accepted (e.g. `*`, `+`, `?`, or a count like `"1"`).
    pub num_args: Option<String>,
}

/// A parsed command (or sub-command).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Command {
    /// Name of the command, if known.
    pub name: Option<String>,
    /// Author / maintainer information, if present.
    pub author: Option<String>,
    /// Short description / about line.
    pub description: Option<String>,
    /// Recognised arguments / flags.
    pub args: Vec<Arg>,
    /// Recognised sub-commands.
    pub subcommands: Vec<Command>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Clap command conversion
// ──────────────────────────────────────────────────────────────────────────────

/// Convert a parsed [`Command`] tree into a [`clap::Command`] that can be used
/// to generate shell completion scripts via [`clap_complete`].
///
/// Argument names are derived from the long flag (stripping the leading `--`),
/// falling back to the short flag (stripping `-`), and finally `"arg"` for
/// purely positional arguments. Short and long flags are attached when present.
/// `value_type` is used as the `value_name` meta-variable and also implies
/// `num_args(1)` unless `num_args` overrides it explicitly.
///
/// This uses clap's `string` feature to dynamically allocate and assign owned
/// strings to avoid any memory leakages.
pub fn to_clap_command(cmd: &Command) -> clap::Command {
    let name = cmd.name.clone().unwrap_or_else(|| "unknown".to_string());
    let mut clap_cmd = clap::Command::new(name)
        // The parsed args already include `--help`/`--version` when present, so
        // disable clap's auto-generated flags to avoid duplicate-name panics.
        .disable_help_flag(true)
        .disable_version_flag(true);

    if let Some(desc) = &cmd.description {
        clap_cmd = clap_cmd.about(desc.clone());
    }

    let mut used_short_flags = std::collections::HashSet::new();
    let mut used_long_flags = std::collections::HashSet::new();
    let mut used_arg_ids = std::collections::HashSet::new();

    for arg in &cmd.args {
        // Strip leading dashes from the long flag once; reuse for both the
        // argument identifier and the `.long()` call.
        let long_bare: Option<String> = arg
            .long
            .as_deref()
            .map(|l| l.trim_start_matches('-').to_string());

        if let Some(long) = &long_bare {
            if !used_long_flags.insert(long.clone()) {
                log::debug!("flycomp: dropping duplicate long flag '--{}'", long);
                continue;
            }
        }

        // Derive a stable identifier for the argument, then make it unique for
        // clap even when the parsed help contains repeated or unnamed args.
        let base_id = long_bare
            .clone()
            .or_else(|| {
                arg.short
                    .as_deref()
                    .map(|s| s.trim_start_matches('-').to_string())
            })
            .unwrap_or_else(|| "arg".to_string());

        let mut id = base_id.clone();
        let mut suffix = 2;
        while !used_arg_ids.insert(id.clone()) {
            id = format!("{}-{}", base_id, suffix);
            suffix += 1;
        }

        let mut clap_arg = clap::Arg::new(id.clone());

        if let Some(long) = &long_bare {
            clap_arg = clap_arg.long(long.clone());
        }

        if let Some(short) = &arg.short {
            if let Some(c) = short.trim_start_matches('-').chars().next() {
                if used_short_flags.insert(c) {
                    clap_arg = clap_arg.short(c);
                } else {
                    log::debug!(
                        "flycomp: dropping duplicate short flag '-{}' for arg {:?}",
                        c,
                        id
                    );
                }
            }
        }

        if let Some(desc) = &arg.description {
            clap_arg = clap_arg.help(desc.clone());
        }

        if let Some(value_type) = &arg.value_type {
            // Strip surrounding angle-brackets if present (e.g. `<PATH>` → `PATH`).
            let meta = value_type
                .trim_matches(|c| c == '<' || c == '>')
                .to_string();
            clap_arg = clap_arg.value_name(meta);
            // A value type implies the flag accepts exactly one value by default;
            // this may be overridden below by an explicit `num_args`.
            clap_arg = clap_arg.num_args(1);
        }

        if let Some(num_args_str) = &arg.num_args {
            clap_arg = match num_args_str.as_str() {
                "*" => clap_arg.num_args(0..),
                "+" => clap_arg.num_args(1..),
                "?" => clap_arg.num_args(0..=1),
                s => {
                    if let Ok(n) = s.parse::<usize>() {
                        clap_arg.num_args(n)
                    } else {
                        clap_arg
                    }
                }
            };
        }

        clap_cmd = clap_cmd.arg(clap_arg);
    }

    for sub in &cmd.subcommands {
        clap_cmd = clap_cmd.subcommand(to_clap_command(sub));
    }

    clap_cmd
}

// ──────────────────────────────────────────────────────────────────────────────
// Synthesis: run a command and build its completion spec
// ──────────────────────────────────────────────────────────────────────────────

/// Invoke `help_runner` to obtain help text, parse it, and flesh out each
/// discovered subcommand by calling `help_runner` again with the subcommand
/// path.  Subcommands are explored iteratively using a work-stack so that
/// nested subcommands (sub-sub-commands, etc.) are also populated, up to a
/// maximum nesting depth of [`MAX_SUBCOMMAND_DEPTH`].
///
/// The `name` field of the returned [`Command`] is always set to the basename
/// of `command_path` so that the generated completion script uses the correct
/// name regardless of what the help text says.
///
/// `help_runner` is called with the subcommand path (e.g. `&["remote", "add"]`)
/// and must return the corresponding `--help` output.  For the top-level
/// command the slice is empty (`&[]`).
pub fn synthesize_completion<F>(
    command_path: &str,
    help_runner: F,
    strategy: SynthesisStrategy,
) -> anyhow::Result<Command>
where
    F: Fn(&[&str]) -> anyhow::Result<String>,
{
    synthesize_completion_with(command_path, &help_runner, strategy)
}

fn synthesize_completion_with<F>(
    command_path: &str,
    help_runner: &F,
    strategy: SynthesisStrategy,
) -> anyhow::Result<Command>
where
    F: Fn(&[&str]) -> anyhow::Result<String>,
{
    match strategy {
        SynthesisStrategy::RunHelp => synthesize_from_help(command_path, help_runner),
        SynthesisStrategy::ManPage => load_manpage_command(command_path),
        SynthesisStrategy::ManPageThenRunHelp => match load_manpage_command(command_path) {
            Ok(command) => Ok(command),
            Err(error) => {
                log::debug!(
                    "flycomp: falling back to --help for '{}': {}",
                    command_path,
                    error
                );
                synthesize_from_help(command_path, help_runner)
            }
        },
    }
}

fn command_basename(command_path: &str) -> String {
    std::path::Path::new(command_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command_path)
        .to_string()
}

fn load_manpage_command(command_path: &str) -> anyhow::Result<Command> {
    let cmd_name = command_basename(command_path);
    let manpage_path = locate_manpage(&cmd_name)?;
    let manpage_content = read_manpage_source(&manpage_path)?;

    parse_man::parse_manpage(&cmd_name, &manpage_content)
        .ok_or_else(|| anyhow::anyhow!("failed to parse man page for '{}'", cmd_name))
}

fn locate_manpage(command_name: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("man")
        .args(["-w", command_name])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to locate man page for '{}': {}", command_name, e))?;

    if !output.status.success() {
        anyhow::bail!("man page not found for '{}'", command_name);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| anyhow::anyhow!("man page path missing for '{}'", command_name))?;

    Ok(path.to_string())
}

fn read_manpage_source(manpage_path: &str) -> anyhow::Result<String> {
    if manpage_path.ends_with(".gz") {
        let output = std::process::Command::new("gzip")
            .args(["-cd", manpage_path])
            .output()
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to read compressed man page '{}': {}",
                    manpage_path,
                    e
                )
            })?;

        if !output.status.success() {
            anyhow::bail!("failed to decompress man page '{}'", manpage_path);
        }

        String::from_utf8(output.stdout)
            .with_context(|| format!("man page '{}' is not valid UTF-8", manpage_path))
    } else {
        std::fs::read_to_string(manpage_path)
            .with_context(|| format!("failed to read man page '{}'", manpage_path))
    }
}

fn synthesize_from_help<F>(command_path: &str, help_runner: &F) -> anyhow::Result<Command>
where
    F: Fn(&[&str]) -> anyhow::Result<String>,
{
    // ── top-level help ───────────────────────────────────────────────────────
    let top_help = help_runner(&[])?;
    let mut root = parse_help(&top_help);

    // Always use the basename of the supplied path as the canonical name.
    let cmd_name = command_basename(command_path);
    root.name = Some(cmd_name);

    // ── iterative subcommand exploration ─────────────────────────────────────
    // Each stack entry is a path of subcommand names from the root, e.g.
    // `["remote", "add"]`.  We use this path both to locate the node in the
    // `Command` tree and to build the argv for the `--help` invocation.
    // Five levels of nesting covers the vast majority of real CLI tools while
    // keeping synthesis time bounded.
    const MAX_SUBCOMMAND_DEPTH: usize = 5;

    // Seed the stack with every top-level subcommand.
    let mut stack: Vec<Vec<String>> = root
        .subcommands
        .iter()
        .filter_map(|s| s.name.clone().map(|n| vec![n]))
        .collect();

    while let Some(path) = stack.pop() {
        if path.len() > MAX_SUBCOMMAND_DEPTH {
            continue;
        }

        // Build the argv slice for the help invocation.
        let path_strs: Vec<&str> = path.iter().map(String::as_str).collect();
        let help_output = match help_runner(&path_strs) {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => continue,
            Err(e) => {
                log::debug!("flycomp: skipping '{}': {}", path_strs.join(" "), e);
                continue;
            }
        };

        let parsed = parse_help(&help_output);

        // Navigate to the target node in the tree and update it.
        if let Some(node) = find_subcommand_mut(&mut root, &path) {
            // Push newly discovered sub-subcommands onto the stack before
            // overwriting them so we can explore them later.
            for child in &parsed.subcommands {
                if let Some(child_name) = &child.name {
                    let mut child_path = path.clone();
                    child_path.push(child_name.clone());
                    stack.push(child_path);
                }
            }

            node.args = parsed.args;
            node.subcommands = parsed.subcommands;
            if node.description.is_none() {
                node.description = parsed.description;
            }
        }
    }

    Ok(root)
}

/// Navigate the [`Command`] tree following `path` (a slice of subcommand
/// names) and return a mutable reference to the deepest node, or `None` if
/// any step along the path cannot be found.
fn find_subcommand_mut<'a>(root: &'a mut Command, path: &[String]) -> Option<&'a mut Command> {
    let mut current = root;
    for name in path {
        current = current
            .subcommands
            .iter_mut()
            .find(|s| s.name.as_deref() == Some(name.as_str()))?;
    }
    Some(current)
}

/// Invoke `command_path [extra_args...] --help` and return the combined output.
///
/// Many tools print their help to *stderr* rather than *stdout*; this function
/// returns whichever stream is non-empty (preferring stdout).
pub fn run_help(command_path: &str, extra_args: &[&str], sandbox: bool) -> anyhow::Result<String> {
    let use_sandbox = sandbox && {
        // Test if bwrap exists in PATH by trying to spawn it with --version
        match std::process::Command::new("bwrap")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                let _ = child.wait();
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::warn!(
                    "bubblewrap (bwrap) not found in PATH; running completion check unsandboxed."
                );
                false
            }
            Err(_) => false,
        }
    };

    let mut child = if use_sandbox {
        let mut cmd = std::process::Command::new("bwrap");
        cmd.args([
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--unshare-all",
            "--",
            command_path,
        ]);
        cmd
    } else {
        std::process::Command::new(command_path)
    };

    let mut child = child
        .args(extra_args)
        .arg("--help")
        .env("PAGER", "cat")
        .env("MANPAGER", "cat")
        .env("SYSTEMD_PAGER", "cat")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn '{}': {}", command_path, e))?;

    let mut stdout_handle = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;
    let mut stderr_handle = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture stderr"))?;

    let stdout_thread = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout_handle, &mut out);
        out
    });

    let stderr_thread = std::thread::spawn(move || {
        let mut err = String::new();
        let _ = std::io::Read::read_to_string(&mut stderr_handle, &mut err);
        err
    });

    let timeout = std::time::Duration::from_millis(1500); // 1.5 seconds timeout
    let start = std::time::Instant::now();
    let mut exited = false;

    while start.elapsed() < timeout {
        if let Some(_status) = child
            .try_wait()
            .map_err(|e| anyhow::anyhow!("failed to wait: {}", e))?
        {
            exited = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    if !exited {
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("command '{}' timed out", command_path);
    }

    let stdout = stdout_thread
        .join()
        .map_err(|_| anyhow::anyhow!("stdout thread panicked"))?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| anyhow::anyhow!("stderr thread panicked"))?;

    // Some tools (e.g. git) write help to stdout when `--help` is passed as a
    // flag, but others write to stderr.  Prefer stdout when it has content.
    Ok(if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    })
}

/// Run `command_path --help`, synthesize its completion model, and render a
/// shell completion script.
pub fn generate_completion_script(
    command_path: &str,
    shell: clap_complete::Shell,
    strategy: SynthesisStrategy,
    sandbox: bool,
) -> anyhow::Result<String> {
    let parsed_cmd = synthesize_completion(
        command_path,
        |args| run_help(command_path, args, sandbox),
        strategy,
    )?;
    let cmd_name = command_basename(command_path);

    let mut clap_cmd = to_clap_command(&parsed_cmd);
    let mut output = Vec::new();
    clap_complete::generate(shell, &mut clap_cmd, &cmd_name, &mut output);

    let script = std::str::from_utf8(&output)
        .map_err(|e| anyhow::anyhow!("failed to encode completion script: {}", e))?
        .to_string();
    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── to_clap_command ───────────────────────────────────────────────────────

    #[test]
    fn test_to_clap_command_basic() {
        let cmd = Command {
            name: Some("mytool".to_string()),
            description: Some("A test tool.".to_string()),
            args: vec![
                Arg {
                    long: Some("--verbose".to_string()),
                    short: Some("-v".to_string()),
                    description: Some("Enable verbose output.".to_string()),
                    value_type: None,
                    num_args: None,
                },
                Arg {
                    long: Some("--output".to_string()),
                    short: None,
                    description: Some("Output file.".to_string()),
                    value_type: Some("<PATH>".to_string()),
                    num_args: None,
                },
            ],
            subcommands: vec![Command {
                name: Some("sub".to_string()),
                description: Some("A subcommand.".to_string()),
                args: vec![],
                subcommands: vec![],
                author: None,
            }],
            author: None,
        };

        let clap_cmd = to_clap_command(&cmd);

        assert_eq!(clap_cmd.get_name(), "mytool");

        // Check args are present.
        let arg_ids: Vec<&str> = clap_cmd
            .get_arguments()
            .map(|a| a.get_id().as_str())
            .collect();
        assert!(
            arg_ids.contains(&"verbose"),
            "verbose arg missing: {arg_ids:?}"
        );
        assert!(
            arg_ids.contains(&"output"),
            "output arg missing: {arg_ids:?}"
        );

        // Check subcommand is present.
        let sub_names: Vec<&str> = clap_cmd.get_subcommands().map(|s| s.get_name()).collect();
        assert!(
            sub_names.contains(&"sub"),
            "sub subcommand missing: {sub_names:?}"
        );
    }

    #[test]
    fn test_to_clap_command_generates_bash_completion() {
        // Parse a simple clap-style help and verify that the generated Bash
        // completion script is non-empty and references the command name.
        const HELP: &str = r#"Usage: greet [OPTIONS]

Options:
  -n, --name <NAME>  Name to greet
  -h, --help         Print help
"#;
        let cmd = parse_help(HELP);
        let mut clap_cmd = to_clap_command(&cmd);
        let bin_name = clap_cmd.get_name().to_string();
        let mut out = Vec::new();
        clap_complete::generate(
            clap_complete::Shell::Bash,
            &mut clap_cmd,
            &bin_name,
            &mut out,
        );
        let script = String::from_utf8(out).expect("completion output is valid utf-8");
        assert!(!script.is_empty());
        assert!(script.contains("greet"));
    }

    #[test]
    fn test_to_clap_command_drops_duplicate_short_flags() {
        let cmd = Command {
            name: Some("readelf".to_string()),
            description: Some("A test tool with duplicate short flags.".to_string()),
            args: vec![
                Arg {
                    long: Some("--debug-dump[a/".to_string()),
                    short: Some("-w".to_string()),
                    description: Some("DWARF debug dump selector.".to_string()),
                    value_type: None,
                    num_args: None,
                },
                Arg {
                    long: Some("--debug-dump".to_string()),
                    short: Some("-w".to_string()),
                    description: Some("DWARF debug dump mode.".to_string()),
                    value_type: Some("links".to_string()),
                    num_args: None,
                },
            ],
            subcommands: vec![],
            author: None,
        };

        let mut clap_cmd = to_clap_command(&cmd);
        let bin_name = clap_cmd.get_name().to_string();
        let mut out = Vec::new();
        clap_complete::generate(
            clap_complete::Shell::Bash,
            &mut clap_cmd,
            &bin_name,
            &mut out,
        );
        let script = String::from_utf8(out).expect("completion output is valid utf-8");
        assert!(!script.is_empty());
        assert!(script.contains("readelf"));
    }

    #[test]
    fn test_to_clap_command_drops_duplicate_long_flags() {
        let cmd = Command {
            name: Some("readelf".to_string()),
            description: Some("A test tool with duplicate long flags.".to_string()),
            args: vec![
                Arg {
                    long: Some("--debug-dump".to_string()),
                    short: Some("-w".to_string()),
                    description: Some("DWARF debug dump selector.".to_string()),
                    value_type: Some("a/".to_string()),
                    num_args: None,
                },
                Arg {
                    long: Some("--debug-dump".to_string()),
                    short: None,
                    description: Some("DWARF debug dump links mode.".to_string()),
                    value_type: Some("links".to_string()),
                    num_args: None,
                },
            ],
            subcommands: vec![],
            author: None,
        };

        let mut clap_cmd = to_clap_command(&cmd);
        let bin_name = clap_cmd.get_name().to_string();
        let mut out = Vec::new();
        clap_complete::generate(
            clap_complete::Shell::Bash,
            &mut clap_cmd,
            &bin_name,
            &mut out,
        );
        let script = String::from_utf8(out).expect("completion output is valid utf-8");
        assert!(!script.is_empty());
        assert!(script.contains("readelf"));
    }

    // ── find_subcommand_mut ───────────────────────────────────────────────────

    #[test]
    fn test_find_subcommand_mut_nested() {
        // Build a two-level tree: root → child → grandchild.
        let mut root = Command {
            name: Some("root".to_string()),
            description: None,
            args: vec![],
            subcommands: vec![Command {
                name: Some("child".to_string()),
                description: None,
                args: vec![],
                subcommands: vec![Command {
                    name: Some("grandchild".to_string()),
                    description: None,
                    args: vec![],
                    subcommands: vec![],
                    author: None,
                }],
                author: None,
            }],
            author: None,
        };

        // Navigate to grandchild via find_subcommand_mut.
        let path = vec!["child".to_string(), "grandchild".to_string()];
        let node = find_subcommand_mut(&mut root, &path).expect("grandchild should be found");
        assert_eq!(node.name.as_deref(), Some("grandchild"));

        // Populate it with an arg.
        node.args.push(Arg {
            long: Some("--verbose".to_string()),
            short: Some("-v".to_string()),
            description: Some("Be verbose".to_string()),
            value_type: None,
            num_args: None,
        });

        // Verify through the tree.
        let grandchild = root
            .subcommands
            .first()
            .and_then(|c| c.subcommands.first())
            .expect("grandchild should exist");
        assert_eq!(grandchild.args.len(), 1);
        assert_eq!(grandchild.args[0].long.as_deref(), Some("--verbose"));
    }

    #[test]
    fn test_find_subcommand_mut_missing() {
        let mut root = Command {
            name: Some("root".to_string()),
            description: None,
            args: vec![],
            subcommands: vec![],
            author: None,
        };
        // A path that doesn't exist should return None.
        let path = vec!["nonexistent".to_string()];
        assert!(find_subcommand_mut(&mut root, &path).is_none());
    }
}
