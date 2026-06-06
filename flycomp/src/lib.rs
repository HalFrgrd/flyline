//! Parse a `--help` string into a [`Command`] structure.
//!
//! The entry point is [`parse_help`].  It tries to identify which help format
//! the text comes from (clap, Python argparse, or an unknown generic format)
//! and dispatches to the appropriate sub-parser.
use anyhow::Context;
use chrono::{SecondsFormat, Utc};
use strum::IntoStaticStr;

use std::cell::Cell;

thread_local! {
    static MAN_PAGES_READ: Cell<usize> = Cell::new(0);
    static HELP_RUNS: Cell<usize> = Cell::new(0);
}

pub fn increment_man_pages_read() {
    MAN_PAGES_READ.with(|c| c.set(c.get() + 1));
}

pub fn increment_help_runs() {
    HELP_RUNS.with(|c| c.set(c.get() + 1));
}

pub fn get_man_pages_read() -> usize {
    MAN_PAGES_READ.with(|c| c.get())
}

pub fn get_help_runs() -> usize {
    HELP_RUNS.with(|c| c.get())
}

pub fn reset_stats() {
    MAN_PAGES_READ.with(|c| c.set(0));
    HELP_RUNS.with(|c| c.set(0));
}

mod parse_help;
pub mod parse_man;

pub use parse_help::{parse_help, parse_help_argparse, parse_help_clap, parse_help_generic};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, clap::ValueEnum, IntoStaticStr)]
#[value(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum SynthesisStrategy {
    #[default]
    ManPageThenRunHelp,
    ManPage,
    RunHelp,
}

#[derive(Clone, Debug, clap::ValueEnum)]
#[value(rename_all = "lower")]
pub enum OutputFormat {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
    Json,
}

impl OutputFormat {
    fn shell(self) -> Option<clap_complete::Shell> {
        match self {
            OutputFormat::Bash => Some(clap_complete::Shell::Bash),
            OutputFormat::Elvish => Some(clap_complete::Shell::Elvish),
            OutputFormat::Fish => Some(clap_complete::Shell::Fish),
            OutputFormat::Powershell => Some(clap_complete::Shell::PowerShell),
            OutputFormat::Zsh => Some(clap_complete::Shell::Zsh),
            OutputFormat::Json => None,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Public data structures
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ValueHint {
    #[default]
    Unknown,
    Other,
    AnyPath,
    FilePath,
    DirPath,
    ExecutablePath,
    CommandName,
    CommandString,
    CommandWithArguments,
    Username,
    Hostname,
    Url,
    EmailAddress,
    EnvVar,
    NetworkInterface,
    GitBranch,
    GitRevision,
    SystemdUnit,
}

/// A single command-line argument / flag.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Arg {
    /// Long flag name, e.g. `--verbose`.
    pub long: Option<String>,
    /// Short flag name, e.g. `-v`.
    pub short: Option<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Meta-variable / value name hint (e.g. `<PATH>`, `<N>`).
    pub value_name: Option<String>,
    /// Number of values accepted (e.g. `*`, `+`, `?`, or a count like `"1"`).
    pub num_args: Option<String>,
    /// Possible values for the option.
    pub value_enum: Option<Vec<String>>,
    /// Completion hint for the option value.
    pub value_hint: ValueHint,
}

/// A parsed command (or sub-command).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Command {
    /// Name of the command, if known.
    pub name: Option<String>,
    /// Subcommand aliases.
    pub aliases: Vec<String>,
    /// Author / maintainer information, if present.
    pub author: Option<String>,
    /// Short description / about line.
    pub description: Option<String>,
    /// Recognised arguments / flags.
    pub args: Vec<Arg>,
    /// Recognised sub-commands.
    pub subcommands: Vec<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, IntoStaticStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
enum SynthesisMethod {
    ManPage,
    RunHelp,
}

#[derive(Debug, Clone, PartialEq)]
struct SynthesisOutcome {
    command: Command,
    strategy_used: SynthesisMethod,
}

#[derive(Debug, Clone, serde::Serialize)]
struct JsonMetadata {
    flycomp_version: &'static str,
    git_hash: &'static str,
    build_time: &'static str,
    generated_at: String,
    output_format: &'static str,
    requested_strategy: &'static str,
    strategy_used: &'static str,
    sandboxed: bool,
    timeout_ms: u64,
    command_path: String,
    man_pages_read: usize,
    help_runs: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct JsonCompletionOutput {
    metadata: JsonMetadata,
    command: Command,
}

impl Command {
    pub fn expand_no_options(&mut self) {
        let mut expanded_args = Vec::new();
        for arg in std::mem::take(&mut self.args) {
            if let Some(long) = &arg.long {
                if long.contains("[no-]") {
                    let base = long.replace("[no-]", "");
                    let no_variant = long.replace("[no-]", "no-");

                    let mut arg1 = arg.clone();
                    arg1.long = Some(base);
                    expanded_args.push(arg1);

                    let mut arg2 = arg.clone();
                    arg2.long = Some(no_variant);
                    arg2.short = None;
                    expanded_args.push(arg2);
                } else if long.contains("[no]") {
                    let base = long.replace("[no]", "");
                    let no_variant = long.replace("[no]", "no");

                    let mut arg1 = arg.clone();
                    arg1.long = Some(base);
                    expanded_args.push(arg1);

                    let mut arg2 = arg.clone();
                    arg2.long = Some(no_variant);
                    arg2.short = None;
                    expanded_args.push(arg2);
                } else {
                    expanded_args.push(arg);
                }
            } else {
                expanded_args.push(arg);
            }
        }
        self.args = expanded_args;

        for sub in &mut self.subcommands {
            sub.expand_no_options();
        }
    }

    pub fn populate_possible_values(&mut self) {
        for arg in &mut self.args {
            if arg.value_enum.is_none() {
                arg.value_enum =
                    parse_possible_values(arg.value_name.as_deref(), arg.description.as_deref());
            }
            if arg.value_hint == ValueHint::Unknown {
                arg.value_hint =
                    extract_value_hint(arg.value_name.as_deref(), arg.description.as_deref());
            }
        }
        for sub in &mut self.subcommands {
            sub.populate_possible_values();
        }
    }
}

pub fn extract_value_hint(value_name: Option<&str>, description: Option<&str>) -> ValueHint {
    let name_lower = value_name.map(|s| s.to_lowercase());
    let desc_lower = description.map(|s| s.to_lowercase());

    // Helper to check if name contains a substring
    let name_contains = |substring: &str| {
        if let Some(ref name) = name_lower {
            name.contains(substring)
        } else {
            false
        }
    };

    // Helper to check if desc contains a substring
    let desc_contains = |substring: &str| {
        if let Some(ref desc) = desc_lower {
            desc.contains(substring)
        } else {
            false
        }
    };

    // 1. Check value_name first (high precision)
    if name_contains("url") || name_contains("uri") {
        return ValueHint::Url;
    }
    if name_contains("email") {
        return ValueHint::EmailAddress;
    }
    if name_contains("hostname")
        || name_contains("host")
        || name_contains("domain")
        || name_contains("address")
    {
        return ValueHint::Hostname;
    }
    if name_contains("username") || name_contains("user_name") || name_contains("user-name") {
        return ValueHint::Username;
    }
    if name_contains("command") || name_contains("cmd") {
        if name_contains("line") || name_contains("string") {
            return ValueHint::CommandString;
        }
        return ValueHint::CommandName;
    }
    if name_contains("executable") || name_contains("binary") {
        return ValueHint::ExecutablePath;
    }
    if name_contains("dir") || name_contains("directory") || name_contains("folder") {
        return ValueHint::DirPath;
    }
    if name_contains("env-var") || name_contains("envvar") || name_contains("env_var") {
        return ValueHint::EnvVar;
    }
    if name_contains("interface") || name_contains("iface") {
        return ValueHint::NetworkInterface;
    }
    if name_contains("branch") {
        return ValueHint::GitBranch;
    }
    if name_contains("revision") || name_contains("commit") {
        return ValueHint::GitRevision;
    }
    if name_contains("service") || name_contains("unit") {
        return ValueHint::SystemdUnit;
    }
    // Dict, Log, Archive, and File are FilePaths
    if name_contains("file")
        || name_contains("filename")
        || name_contains("filepath")
        || name_contains("dict")
        || name_contains("dictionary")
        || name_contains("log")
        || name_contains("archive")
    {
        // Exclude "level" to avoid "log-level" or "log_level" matching file
        if !name_contains("level") {
            return ValueHint::FilePath;
        }
    }
    if name_contains("path") {
        if desc_contains("directory") || desc_contains("folder") {
            return ValueHint::DirPath;
        }
        if desc_contains("file") && !desc_contains("files") {
            return ValueHint::FilePath;
        }
        return ValueHint::AnyPath;
    }
    if name_contains("unix:") || name_contains("socket") {
        return ValueHint::AnyPath;
    }

    // 2. Check description (lower precision, needs more specific phrases)
    if desc_contains("http://")
        || desc_contains("https://")
        || desc_contains("url")
        || desc_contains("uri")
        || desc_contains("endpoint")
    {
        return ValueHint::Url;
    }
    if desc_contains("email") || desc_contains("e-mail") {
        return ValueHint::EmailAddress;
    }
    if desc_contains("hostname")
        || desc_contains("host name")
        || desc_contains("domain name")
        || desc_contains("ip address")
        || desc_contains("ipv4")
        || desc_contains("ipv6")
        || desc_contains("dns name")
        || desc_contains("target host")
        || desc_contains("remote host")
        || desc_contains("server address")
    {
        return ValueHint::Hostname;
    }
    if desc_contains("username") || desc_contains("user name") || desc_contains("login name") {
        return ValueHint::Username;
    }
    if desc_contains("command line") || desc_contains("command-line") {
        return ValueHint::CommandString;
    }
    if desc_contains("command name") || desc_contains("cmd name") {
        return ValueHint::CommandName;
    }
    if desc_contains("path to executable")
        || desc_contains("path to binary")
        || desc_contains("path to command")
        || desc_contains("executable file")
        || desc_contains("binary file")
    {
        return ValueHint::ExecutablePath;
    }
    if desc_contains("environment variable")
        || desc_contains("env variable")
        || desc_contains("env-var")
    {
        return ValueHint::EnvVar;
    }
    if desc_contains("network interface") {
        return ValueHint::NetworkInterface;
    }
    if desc_contains("git branch") {
        return ValueHint::GitBranch;
    }
    if desc_contains("git revision") || desc_contains("git commit") {
        return ValueHint::GitRevision;
    }
    if desc_contains("systemd service") || desc_contains("systemd unit") {
        return ValueHint::SystemdUnit;
    }

    // Paths/Directories/Files in description
    if desc_contains("directory path")
        || desc_contains("folder path")
        || desc_contains("path to directory")
        || desc_contains("path to folder")
    {
        return ValueHint::DirPath;
    }
    if desc_contains("file path")
        || desc_contains("path to file")
        || desc_contains("filename")
        || desc_contains("file name")
        || desc_contains("dictionary file")
        || desc_contains("log file")
    {
        return ValueHint::FilePath;
    }
    if desc_contains("path to the chosen file") || desc_contains("write traces to") {
        return ValueHint::FilePath;
    }
    if desc_contains("path to") {
        return ValueHint::AnyPath;
    }

    // Specific heuristics:
    // If the name is "output" or "input" or "filelist" or "list" or "destination" or "source", and description mentions "file"
    if let Some(ref name) = name_lower {
        if name == "output"
            || name == "input"
            || name == "filelist"
            || name == "list"
            || name == "destination"
            || name == "source"
            || name == "rfile"
            || name == "debug-file"
        {
            if desc_contains("directory") || desc_contains("folder") || desc_contains("dir") {
                return ValueHint::DirPath;
            }
            if desc_contains("file")
                || desc_contains("path")
                || desc_contains("output")
                || desc_contains("write")
                || desc_contains("read")
            {
                return ValueHint::FilePath;
            }
        }
        if name == "dir" || name == "directory" || name == "folder" || name == "path" {
            return ValueHint::DirPath;
        }
    }

    ValueHint::Unknown
}

fn parse_bulleted_list_values(desc: &str) -> Option<Vec<String>> {
    let mut values = Vec::new();

    // Pattern 1: Bullet + value + separator (like ':' or ' - ' or 2+ spaces)
    let re_bullet_sep =
        regex::Regex::new(r"(?:^|\s)[-*+o•]\s+([a-zA-Z0-9_-]+)(?:\s*:\s+|\s+-\s+|[ \t]{2,})")
            .unwrap();
    for caps in re_bullet_sep.captures_iter(desc) {
        let val = caps.get(1).unwrap().as_str().to_string();
        if !values.contains(&val) {
            values.push(val);
        }
    }

    if !values.is_empty() {
        return Some(values);
    }

    // Pattern 2: Pure bullet + word lines anywhere in the description
    let re_bullet_pure = regex::Regex::new(r"^\s*[-*+o•]\s+([a-zA-Z0-9_-]+)\s*$").unwrap();
    let mut pure_vals = Vec::new();
    for line in desc.lines() {
        if let Some(caps) = re_bullet_pure.captures(line) {
            let val = caps.get(1).unwrap().as_str().to_string();
            if !pure_vals.contains(&val) {
                pure_vals.push(val);
            }
        }
    }

    if pure_vals.len() >= 2 {
        return Some(pure_vals);
    }

    None
}

pub fn parse_possible_values(
    value_name: Option<&str>,
    description: Option<&str>,
) -> Option<Vec<String>> {
    // 1. Try to parse from value_name (e.g. {info,debug} or info|debug or 0..5)
    if let Some(vt) = value_name {
        let clean_type = vt.trim_matches(|c| c == '[' || c == ']' || c == '<' || c == '>');

        // Try range syntax like 0..5 or 1-5
        let range_re = regex::Regex::new(r"^(\d+)(?:\.\.|-)(\d+)$").unwrap();
        if let Some(caps) = range_re.captures(clean_type) {
            let start: usize = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
            let end: usize = caps.get(2).unwrap().as_str().parse().unwrap_or(0);
            if start < end && end - start <= 20 {
                let range_values: Vec<String> = (start..=end).map(|val| val.to_string()).collect();
                return Some(range_values);
            }
        }

        if clean_type.starts_with('{') && clean_type.ends_with('}') {
            let inner = &clean_type[1..clean_type.len() - 1];
            let parts: Vec<String> = inner
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !parts.is_empty() {
                return Some(parts);
            }
        } else if clean_type.contains('|') {
            let parts: Vec<String> = clean_type
                .split('|')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() > 1
                && parts.iter().all(|s| {
                    s.chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                })
            {
                return Some(parts);
            }
        }
    }

    // 2. Try to parse from description
    if let Some(desc) = description {
        if let Some(list_vals) = parse_bulleted_list_values(desc) {
            return Some(list_vals);
        }
        // regex to find the prefix
        let re_prefix = regex::Regex::new(
            r"(?i)\b(?:possible\s+values|choices|allowed\s+values|valid\s+values|one\s+of|accepts|can\s+be|selected\s+from)\s*(?:=|:|\bare\b)?\s*|\bmust\s+be\s+(?:either\s+|one\s+of\s+)?(?:=|:)?\s*"
        ).unwrap();

        if let Some(mat) = re_prefix.find(desc) {
            let mut remaining = &desc[mat.end()..];

            // If it starts with a bracket or brace, extract the inner content
            if remaining.starts_with('[') {
                if let Some(end_idx) = remaining.find(']') {
                    remaining = &remaining[1..end_idx];
                }
            } else if remaining.starts_with('{') {
                if let Some(end_idx) = remaining.find('}') {
                    remaining = &remaining[1..end_idx];
                }
            } else {
                // Otherwise, take up to the next clause boundary: a period (if followed by space/end), a semicolon,
                // or a bracket, or maybe parenthesis.
                let boundary_re = regex::Regex::new(r"(?:\.(?:\s|$)|\;|\]|\))").unwrap();
                if let Some(mat_boundary) = boundary_re.find(remaining) {
                    remaining = &remaining[..mat_boundary.start()];
                }
            }

            let mut remaining_str = remaining.trim();
            if let Some(stripped) = remaining_str.strip_prefix("either ") {
                remaining_str = stripped.trim();
            }
            if let Some(stripped) = remaining_str.strip_prefix("the following:") {
                remaining_str = stripped.trim();
            }

            // Split the remaining string into tokens.
            let token_sep =
                regex::Regex::new(r"\s*(?:,\s*(?:or|and)?|\b(?:or|and)\b|\||/)\s*").unwrap();
            let raw_tokens: Vec<&str> = token_sep.split(remaining_str).collect();

            let mut values = Vec::new();
            for token in raw_tokens {
                let clean = token
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"' || c == '`');
                let clean = clean.trim();
                if !clean.is_empty()
                    && clean
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    values.push(clean.to_string());
                }
            }

            if !values.is_empty() {
                return Some(values);
            }
        }
    }

    // 3. Heuristic: if we find a list and a mention of "default: x" where x is in that list.
    if let Some(desc) = description {
        let re_default = regex::Regex::new(
            r#"(?i)[(\[]?\bdefault\s*(?::|is)\s*['"`]?([a-zA-Z0-9\-_]+)['"`]?[)\]]?"#,
        )
        .unwrap();

        for caps in re_default.captures_iter(desc) {
            let default_val = caps.get(1).unwrap().as_str();

            let segments: Vec<&str> = desc
                .split(|c| c == '.' || c == ';' || c == '(' || c == ')' || c == '[' || c == ']')
                .collect();

            // Also split by "default" itself if it's not already a separator
            let mut all_clauses = Vec::new();
            for s in segments {
                let re_default_sep = regex::Regex::new(r"(?i)\bdefault\s*(?::|is)\b").unwrap();
                for part in re_default_sep.split(s) {
                    all_clauses.push(part);
                }
            }

            for mut clause in all_clauses {
                clause = clause.trim();
                if clause.is_empty() {
                    continue;
                }

                // Strip common prefixes followed by a colon (e.g. "mode: ")
                if let Some(pos) = clause.find(':') {
                    let prefix = &clause[..pos];
                    if prefix.split_whitespace().count() <= 3 {
                        clause = clause[pos + 1..].trim();
                    }
                }

                // Remove "default:" label to avoid interfering with list parsing.
                let re_default_label = regex::Regex::new(r"(?i)\bdefault\s*(?::|is)\s*").unwrap();
                let clause_cleaned = re_default_label.replace_all(clause, " ");
                clause = clause_cleaned.trim();
                if clause.is_empty() {
                    continue;
                }

                let token_sep =
                    regex::Regex::new(r"\s*(?:,\s*(?:or|and)?|\b(?:or|and)\b|\||/)\s*").unwrap();
                let raw_tokens: Vec<&str> = token_sep.split(clause).collect();

                if raw_tokens.len() >= 2 {
                    let mut values = Vec::new();
                    for (idx, token) in raw_tokens.iter().enumerate() {
                        let mut clean = token
                            .trim()
                            .trim_matches(|c| c == '\'' || c == '"' || c == '`' || c == ',')
                            .trim();

                        if idx == 0 {
                            if let Some(last) = clean.split_whitespace().last() {
                                clean = last;
                            }
                        } else if idx == raw_tokens.len() - 1 {
                            if let Some(first) = clean.split_whitespace().next() {
                                clean = first;
                            }
                        }

                        if !clean.is_empty()
                            && clean
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                        {
                            if idx > 0 && idx < raw_tokens.len() - 1 {
                                if token.trim().contains(char::is_whitespace) {
                                    values.clear();
                                    break;
                                }
                            }
                            values.push(clean.to_string());
                        } else {
                            values.clear();
                            break;
                        }
                    }

                    if values.len() >= 2 && values.iter().any(|v| v == default_val) {
                        return Some(values);
                    }
                }
            }
        }
    }

    None
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
        // disable clap's auto-generated help/version surfaces to avoid
        // duplicate-name panics when the parsed help already includes them.
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .disable_version_flag(true);

    for alias in &cmd.aliases {
        clap_cmd = clap_cmd.visible_alias(alias.clone());
    }

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

        if let Some(value_name) = &arg.value_name {
            // Strip surrounding angle-brackets if present (e.g. `<PATH>` → `PATH`).
            let meta = value_name
                .trim_matches(|c| c == '<' || c == '>')
                .to_string();
            clap_arg = clap_arg.value_name(meta);
            // A value name implies the flag accepts exactly one value by default;
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

        if let Some(value_enum) = &arg.value_enum {
            clap_arg =
                clap_arg.value_parser(clap::builder::PossibleValuesParser::new(value_enum.clone()));
        }

        if arg.value_hint != ValueHint::Unknown {
            let clap_hint = match arg.value_hint {
                ValueHint::Unknown => clap::ValueHint::Unknown,
                ValueHint::Other => clap::ValueHint::Other,
                ValueHint::AnyPath => clap::ValueHint::AnyPath,
                ValueHint::FilePath => clap::ValueHint::FilePath,
                ValueHint::DirPath => clap::ValueHint::DirPath,
                ValueHint::ExecutablePath => clap::ValueHint::ExecutablePath,
                ValueHint::CommandName => clap::ValueHint::CommandName,
                ValueHint::CommandString => clap::ValueHint::CommandString,
                ValueHint::CommandWithArguments => clap::ValueHint::CommandWithArguments,
                ValueHint::Username => clap::ValueHint::Username,
                ValueHint::Hostname => clap::ValueHint::Hostname,
                ValueHint::Url => clap::ValueHint::Url,
                ValueHint::EmailAddress => clap::ValueHint::EmailAddress,
                ValueHint::EnvVar
                | ValueHint::NetworkInterface
                | ValueHint::GitBranch
                | ValueHint::GitRevision
                | ValueHint::SystemdUnit => clap::ValueHint::Other,
            };
            clap_arg = clap_arg.value_hint(clap_hint);
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
    recurse_limit: usize,
) -> anyhow::Result<Command>
where
    F: Fn(&[&str]) -> anyhow::Result<String>,
{
    Ok(synthesize_completion_with(command_path, &help_runner, strategy, recurse_limit)?.command)
}

fn merge_commands(help_cmd: Command, man_cmd: Command) -> Command {
    let mut merged = help_cmd.clone();

    if merged.description.is_none() {
        merged.description = man_cmd.description.clone();
    }

    for arg in &mut merged.args {
        if let Some(man_arg) = man_cmd.args.iter().find(|a| {
            (a.long.is_some() && a.long == arg.long) || (a.short.is_some() && a.short == arg.short)
        }) {
            if arg.description.is_none()
                || arg.description.as_deref().unwrap_or("").trim().is_empty()
            {
                arg.description = man_arg.description.clone();
            }
            if arg.value_enum.is_none() {
                arg.value_enum = man_arg.value_enum.clone();
            }
            if arg.value_hint == ValueHint::Unknown {
                arg.value_hint = man_arg.value_hint;
            }
        }
    }

    for sub in &mut merged.subcommands {
        if let Some(man_sub) = man_cmd.subcommands.iter().find(|s| s.name == sub.name) {
            *sub = merge_commands(sub.clone(), man_sub.clone());
        }
    }

    merged
}

fn synthesize_completion_with<F>(
    command_path: &str,
    help_runner: &F,
    strategy: SynthesisStrategy,
    recurse_limit: usize,
) -> anyhow::Result<SynthesisOutcome>
where
    F: Fn(&[&str]) -> anyhow::Result<String>,
{
    match strategy {
        SynthesisStrategy::RunHelp => Ok(SynthesisOutcome {
            command: synthesize_from_help(command_path, help_runner, recurse_limit)?,
            strategy_used: SynthesisMethod::RunHelp,
        }),
        SynthesisStrategy::ManPage => Ok(SynthesisOutcome {
            command: load_manpage_command(command_path, recurse_limit)?,
            strategy_used: SynthesisMethod::ManPage,
        }),
        SynthesisStrategy::ManPageThenRunHelp => {
            match load_manpage_command(command_path, recurse_limit) {
                Ok(man_command) => {
                    match synthesize_from_help(command_path, help_runner, recurse_limit) {
                        Ok(help_command) => Ok(SynthesisOutcome {
                            command: merge_commands(help_command, man_command),
                            strategy_used: SynthesisMethod::ManPage,
                        }),
                        Err(_) => Ok(SynthesisOutcome {
                            command: man_command,
                            strategy_used: SynthesisMethod::ManPage,
                        }),
                    }
                }
                Err(error) => {
                    log::debug!(
                        "flycomp: falling back to --help for '{}': {}",
                        command_path,
                        error
                    );
                    Ok(SynthesisOutcome {
                        command: synthesize_from_help(command_path, help_runner, recurse_limit)?,
                        strategy_used: SynthesisMethod::RunHelp,
                    })
                }
            }
        }
    }
}

fn render_json_output(
    command_path: &str,
    requested_strategy: SynthesisStrategy,
    strategy_used: SynthesisMethod,
    sandboxed: bool,
    timeout_ms: u64,
    command: Command,
) -> anyhow::Result<String> {
    let payload = JsonCompletionOutput {
        metadata: JsonMetadata {
            flycomp_version: env!("CARGO_PKG_VERSION"),
            git_hash: env!("GIT_HASH"),
            build_time: env!("BUILD_TIME"),
            generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            output_format: "json",
            requested_strategy: requested_strategy.into(),
            strategy_used: strategy_used.into(),
            sandboxed,
            timeout_ms,
            command_path: command_path.to_string(),
            man_pages_read: get_man_pages_read(),
            help_runs: get_help_runs(),
        },
        command,
    };

    serde_json::to_string_pretty(&payload).map_err(Into::into)
}

fn command_basename(command_path: &str) -> String {
    std::path::Path::new(command_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command_path)
        .to_string()
}

fn load_manpage_command(command_path: &str, recurse_limit: usize) -> anyhow::Result<Command> {
    let cmd_name = command_basename(command_path);
    let manpage_path = locate_manpage(&cmd_name)?;
    let manpage_content = read_manpage_source(&manpage_path)?;

    let loader = |name: &str| -> Option<String> {
        let path = locate_manpage(name).ok()?;
        read_manpage_source(&path).ok()
    };

    parse_man::parse_manpage_recursive(&cmd_name, &manpage_content, recurse_limit, &loader)
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
    increment_man_pages_read();
    let decomp_cmd = if manpage_path.ends_with(".gz") {
        Some(("gzip", vec!["-cd", manpage_path]))
    } else if manpage_path.ends_with(".zst") {
        Some(("zstd", vec!["-cd", manpage_path]))
    } else if manpage_path.ends_with(".xz") {
        Some(("xz", vec!["-cd", manpage_path]))
    } else {
        None
    };

    if let Some((cmd, args)) = decomp_cmd {
        let output = std::process::Command::new(cmd)
            .args(args)
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

fn synthesize_from_help<F>(
    command_path: &str,
    help_runner: &F,
    recurse_limit: usize,
) -> anyhow::Result<Command>
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
    // Seed the stack with every top-level subcommand.
    let mut stack: Vec<Vec<String>> = root
        .subcommands
        .iter()
        .filter_map(|s| s.name.clone().map(|n| vec![n]))
        .collect();

    while let Some(path) = stack.pop() {
        if path.len() > recurse_limit {
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
pub fn run_help(
    command_path: &str,
    extra_args: &[&str],
    sandbox: bool,
    timeout_ms: u64,
) -> anyhow::Result<String> {
    increment_help_runs();
    let mut actual_command = command_path.to_string();

    // If command_path is a simple name (no path separators), check CWD.
    if !command_path.contains(std::path::MAIN_SEPARATOR) {
        let cwd_path = std::env::current_dir()?.join(command_path);
        if cwd_path.exists() {
            use is_executable::IsExecutable;
            if cwd_path.is_executable() {
                actual_command = format!(".{}{}", std::path::MAIN_SEPARATOR, command_path);
            }
        }
    }

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
            &actual_command,
        ]);
        cmd
    } else {
        std::process::Command::new(&actual_command)
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
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("command '{}' not found in PATH or CWD", actual_command)
            } else {
                anyhow::anyhow!("failed to spawn '{}': {}", actual_command, e)
            }
        })?;

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

    let timeout = std::time::Duration::from_millis(timeout_ms);
    let start = std::time::Instant::now();
    let mut exit_status = None;
    let mut exited = false;
    while start.elapsed() < timeout {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| anyhow::anyhow!("failed to wait: {}", e))?
        {
            exit_status = Some(status);
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

    if use_sandbox {
        if let Some(status) = exit_status {
            if !status.success() {
                if stderr.contains("bwrap:") || stderr.contains("bubblewrap:") {
                    let code = status.code().unwrap_or(-1);
                    eprintln!("bubblewrap error (exit code {}): {}", code, stderr.trim());
                    anyhow::bail!(
                        "bubblewrap exited with error code {}: {}",
                        code,
                        stderr.trim()
                    );
                }
            }
        }
    }

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
    timeout_ms: u64,
    recurse_limit: usize,
) -> anyhow::Result<String> {
    reset_stats();
    let parsed_cmd = synthesize_completion(
        command_path,
        |args| run_help(command_path, args, sandbox, timeout_ms),
        strategy,
        recurse_limit,
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

/// Generate completion output for a command as either a shell script or JSON.
pub fn generate_completion_output(
    command_path: &str,
    output: OutputFormat,
    strategy: SynthesisStrategy,
    sandbox: bool,
    timeout_ms: u64,
    recurse_limit: usize,
) -> anyhow::Result<String> {
    reset_stats();
    if matches!(output, OutputFormat::Json) {
        let outcome = synthesize_completion_with(
            command_path,
            &|extra_args| run_help(command_path, extra_args, sandbox, timeout_ms),
            strategy,
            recurse_limit,
        )?;
        render_json_output(
            command_path,
            strategy,
            outcome.strategy_used,
            sandbox,
            timeout_ms,
            outcome.command,
        )
    } else {
        let shell = output.shell().expect("non-JSON output has shell mapping");
        generate_completion_script(
            command_path,
            shell,
            strategy,
            sandbox,
            timeout_ms,
            recurse_limit,
        )
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct ExpectedArg<'a> {
        pub arg: Arg,
        pub description_contains: &'a str,
    }

    pub fn normalize_desc(desc: Option<&str>) -> String {
        desc.unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn find_arg<'a>(cmd: &'a Command, expected: &ExpectedArg<'_>) -> &'a Arg {
        cmd.args
            .iter()
            .find(|arg| arg.short == expected.arg.short && arg.long == expected.arg.long)
            .or_else(|| {
                cmd.args.iter().find(|arg| {
                    (expected.arg.short.is_some() && arg.short == expected.arg.short)
                        || (expected.arg.long.is_some() && arg.long == expected.arg.long)
                })
            })
            .unwrap_or_else(|| {
                panic!(
                    "Could not find argument matching expected: short={:?}, long={:?}",
                    expected.arg.short, expected.arg.long
                );
            })
    }

    pub fn assert_expected_args(cmd: &Command, expected: &[ExpectedArg<'_>]) {
        assert_eq!(
            cmd.args.len(),
            expected.len(),
            "Number of arguments mismatch: expected {}, got {}. Args in cmd: {:#?}",
            expected.len(),
            cmd.args.len(),
            cmd.args
        );
        assert_contains_expected_args(cmd, expected);
    }

    pub fn assert_contains_expected_args(cmd: &Command, expected: &[ExpectedArg<'_>]) {
        for expected_arg in expected {
            let arg = find_arg(cmd, expected_arg);
            assert_eq!(
                arg.short, expected_arg.arg.short,
                "short mismatch for arg {:?}",
                expected_arg.arg
            );
            assert_eq!(
                arg.long, expected_arg.arg.long,
                "long mismatch for arg {:?}",
                expected_arg.arg
            );
            assert_eq!(
                arg.value_name, expected_arg.arg.value_name,
                "value_name mismatch for arg {:?}",
                expected_arg.arg
            );
            assert_eq!(
                arg.num_args, expected_arg.arg.num_args,
                "num_args mismatch for arg {:?}",
                expected_arg.arg
            );
            if expected_arg.arg.value_hint != ValueHint::Unknown {
                assert_eq!(
                    arg.value_hint, expected_arg.arg.value_hint,
                    "value_hint mismatch for arg {:?}",
                    expected_arg.arg
                );
            } else {
                assert_eq!(
                    arg.value_hint,
                    crate::extract_value_hint(
                        arg.value_name.as_deref(),
                        arg.description.as_deref()
                    ),
                    "ValueHint mismatch for arg {:?} / {:?}",
                    arg.short,
                    arg.long
                );
            }
            if let Some(expected_enum) = &expected_arg.arg.value_enum {
                assert_eq!(
                    arg.value_enum.as_ref(),
                    Some(expected_enum),
                    "value_enum mismatch for arg {:?}",
                    expected_arg.arg
                );
            }
            let description = normalize_desc(arg.description.as_deref());
            assert!(
                description.contains(expected_arg.description_contains),
                "Expected description of {:?}/{:?} to contain {:?}, but got {:?}",
                expected_arg.arg.short,
                expected_arg.arg.long,
                expected_arg.description_contains,
                description
            );
        }
    }

    pub fn assert_expected_subcommands(cmd: &Command, expected: &[(&str, &str)]) {
        assert_eq!(
            cmd.subcommands.len(),
            expected.len(),
            "Number of subcommands mismatch: expected {}, got {}. Subcommands: {:#?}",
            expected.len(),
            cmd.subcommands.len(),
            cmd.subcommands
        );
        assert_contains_subcommands(cmd, expected);
    }

    pub fn assert_contains_subcommands(cmd: &Command, expected: &[(&str, &str)]) {
        for (name, description_contains) in expected {
            let subcommand = cmd
                .subcommands
                .iter()
                .find(|subcommand| subcommand.name.as_deref() == Some(*name))
                .unwrap_or_else(|| {
                    panic!("Could not find subcommand matching: {:?}", name);
                });
            let description = normalize_desc(subcommand.description.as_deref());
            assert!(!description.is_empty());
            assert!(
                description.contains(description_contains),
                "Expected subcommand {:?} description to contain {:?}, but got {:?}",
                name,
                description_contains,
                description
            );
        }
    }
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
                    value_name: None,
                    num_args: None,
                    ..Default::default()
                },
                Arg {
                    long: Some("--output".to_string()),
                    short: None,
                    description: Some("Output file.".to_string()),
                    value_name: Some("<PATH>".to_string()),
                    num_args: None,
                    ..Default::default()
                },
            ],
            subcommands: vec![Command {
                name: Some("sub".to_string()),
                description: Some("A subcommand.".to_string()),
                ..Command::default()
            }],
            ..Command::default()
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
                    value_name: None,
                    num_args: None,
                    ..Default::default()
                },
                Arg {
                    long: Some("--debug-dump".to_string()),
                    short: Some("-w".to_string()),
                    description: Some("DWARF debug dump mode.".to_string()),
                    value_name: Some("links".to_string()),
                    num_args: None,
                    ..Default::default()
                },
            ],
            ..Command::default()
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
                    value_name: Some("a/".to_string()),
                    num_args: None,
                    ..Default::default()
                },
                Arg {
                    long: Some("--debug-dump".to_string()),
                    short: None,
                    description: Some("DWARF debug dump links mode.".to_string()),
                    value_name: Some("links".to_string()),
                    num_args: None,
                    ..Default::default()
                },
            ],
            ..Command::default()
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
            subcommands: vec![Command {
                name: Some("child".to_string()),
                subcommands: vec![Command {
                    name: Some("grandchild".to_string()),
                    ..Command::default()
                }],
                ..Command::default()
            }],
            ..Command::default()
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
            value_name: None,
            num_args: None,
            ..Default::default()
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
            ..Command::default()
        };
        // A path that doesn't exist should return None.
        let path = vec!["nonexistent".to_string()];
        assert!(find_subcommand_mut(&mut root, &path).is_none());
    }

    #[test]
    fn test_read_manpage_source_compressed() {
        use std::process::Command;
        // Create a temporary directory in target
        let temp_dir = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_temp");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let base_path = temp_dir.join("test_manpage");
        let content = ".TH TEST 1\n.SH NAME\ntest - a test tool";
        std::fs::write(&base_path, content).unwrap();

        // Compress with gzip
        let gz_path = temp_dir.join("test_manpage.gz");
        let _ = Command::new("gzip")
            .args(["-c"])
            .stdout(std::fs::File::create(&gz_path).unwrap())
            .stdin(std::fs::File::open(&base_path).unwrap())
            .status();

        // Compress with zstd if zstd is available
        let zst_path = temp_dir.join("test_manpage.zst");
        let zstd_ok = Command::new("zstd")
            .args(["-c", "-q"])
            .stdout(std::fs::File::create(&zst_path).unwrap())
            .stdin(std::fs::File::open(&base_path).unwrap())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        // Compress with xz if xz is available
        let xz_path = temp_dir.join("test_manpage.xz");
        let xz_ok = Command::new("xz")
            .args(["-c", "-q"])
            .stdout(std::fs::File::create(&xz_path).unwrap())
            .stdin(std::fs::File::open(&base_path).unwrap())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        // Test read_manpage_source for gz
        if gz_path.exists() {
            let res = read_manpage_source(gz_path.to_str().unwrap()).unwrap();
            assert_eq!(res, content);
        }

        // Test read_manpage_source for zst
        if zstd_ok && zst_path.exists() {
            let res = read_manpage_source(zst_path.to_str().unwrap()).unwrap();
            assert_eq!(res, content);
        }

        // Test read_manpage_source for xz
        if xz_ok && xz_path.exists() {
            let res = read_manpage_source(xz_path.to_str().unwrap()).unwrap();
            assert_eq!(res, content);
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_run_help_in_cwd() {
        use std::io::Write;
        let temp_dir = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test_run_help_cwd");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let cmd_path = temp_dir.join("dummy_cmd");

        let script = "#!/bin/sh\necho \"Usage: dummy_cmd [OPTIONS]\n\nOptions:\n  --foo  bar\"";
        {
            let mut f = std::fs::File::create(&cmd_path).unwrap();
            f.write_all(script.as_bytes()).unwrap();
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&cmd_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&cmd_path, perms).unwrap();
        }

        let old_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let res = run_help("dummy_cmd", &[], false, 5000);

        std::env::set_current_dir(old_cwd).unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);

        let output = res.expect("run_help should succeed");
        assert!(output.contains("--foo"));
    }

    #[test]
    fn test_to_clap_command_subcommand_aliases() {
        let cmd = Command {
            name: Some("cargo".to_string()),
            subcommands: vec![Command {
                name: Some("build".to_string()),
                aliases: vec!["b".to_string()],
                ..Command::default()
            }],
            ..Command::default()
        };
        let clap_cmd = to_clap_command(&cmd);
        let sub = clap_cmd.find_subcommand("build").unwrap();
        let visible_aliases: Vec<&str> = sub.get_visible_aliases().collect();
        assert_eq!(visible_aliases, vec!["b"]);
    }

    #[test]
    fn test_render_json_output_includes_metadata() {
        let json = render_json_output(
            "cargo",
            SynthesisStrategy::ManPageThenRunHelp,
            SynthesisMethod::RunHelp,
            true,
            15000,
            Command {
                name: Some("cargo".to_string()),
                description: Some("Rust package manager".to_string()),
                ..Command::default()
            },
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value["metadata"]["flycomp_version"],
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(value["metadata"]["git_hash"], env!("GIT_HASH"));
        assert_eq!(value["metadata"]["build_time"], env!("BUILD_TIME"));
        assert_eq!(value["metadata"]["output_format"], "json");
        assert_eq!(
            value["metadata"]["requested_strategy"],
            "man-page-then-run-help"
        );
        assert_eq!(value["metadata"]["strategy_used"], "run-help");
        assert_eq!(value["metadata"]["sandboxed"], true);
        assert_eq!(value["metadata"]["timeout_ms"], 15000);
        assert_eq!(value["metadata"]["command_path"], "cargo");
        assert_eq!(value["metadata"]["man_pages_read"], 0);
        assert_eq!(value["metadata"]["help_runs"], 0);
        assert_eq!(value["command"]["name"], "cargo");
        assert!(value["metadata"]["generated_at"].as_str().is_some());
    }

    #[test]
    fn test_parse_possible_values_extraction() {
        // Test parsing from value_name
        assert_eq!(
            parse_possible_values(Some("{info,debug,trace}"), None),
            Some(vec![
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(Some("info|debug|trace"), None),
            Some(vec![
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(Some("[info|debug|trace]"), None),
            Some(vec![
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(Some("0..5"), None),
            Some(vec![
                "0".to_string(),
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "5".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(Some("<1-3>"), None),
            Some(vec!["1".to_string(), "2".to_string(), "3".to_string()])
        );
        assert_eq!(
            parse_possible_values(Some("0..100"), None), // too large, should be None
            None
        );

        // Test parsing from description
        assert_eq!(
            parse_possible_values(
                None,
                Some("Set the logging level [possible values: error, warn, info, debug, trace]")
            ),
            Some(vec![
                "error".to_string(),
                "warn".to_string(),
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(None, Some("Choices: info, debug, trace")),
            Some(vec![
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(
                None,
                Some("allowed values are 'info', 'debug', or 'trace'.")
            ),
            Some(vec![
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(None, Some("must be either info or debug")),
            Some(vec!["info".to_string(), "debug".to_string()])
        );
        assert_eq!(
            parse_possible_values(None, Some("valid values: info/debug/trace")),
            Some(vec![
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(None, Some("one of: info, debug, trace")),
            Some(vec![
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(None, Some("accepts: \"info\", \"debug\", \"trace\"")),
            Some(vec![
                "info".to_string(),
                "debug".to_string(),
                "trace".to_string()
            ])
        );
        assert_eq!(
            parse_possible_values(None, Some("can be either fast or slow")),
            Some(vec!["fast".to_string(), "slow".to_string()])
        );
        assert_eq!(
            parse_possible_values(None, Some("selected from: apple, banana")),
            Some(vec!["apple".to_string(), "banana".to_string()])
        );

        // Test bulleted list parsing
        assert_eq!(
            parse_possible_values(
                None,
                Some("Supported formats:\n  - json: JSON format\n  - yaml: YAML format")
            ),
            Some(vec!["json".to_string(), "yaml".to_string()])
        );
        assert_eq!(
            parse_possible_values(
                None,
                Some("Supported formats: - json: JSON format - yaml: YAML format")
            ),
            Some(vec!["json".to_string(), "yaml".to_string()])
        );
        assert_eq!(
            parse_possible_values(None, Some("Modes:\n  * fast\n  * slow")),
            Some(vec!["fast".to_string(), "slow".to_string()])
        );

        // Test none when no matches or junk
        assert_eq!(
            parse_possible_values(None, Some("This option can be specified multiple times.")),
            None
        );
    }

    #[test]
    fn test_parse_possible_values_heuristic_default() {
        // Test: list + default: x
        assert_eq!(
            parse_possible_values(None, Some("mode: fast, slow, or medium. default: fast")),
            Some(vec![
                "fast".to_string(),
                "slow".to_string(),
                "medium".to_string()
            ])
        );

        // Test: list + default is x
        assert_eq!(
            parse_possible_values(None, Some("color: red|green|blue (default is green)")),
            Some(vec![
                "red".to_string(),
                "green".to_string(),
                "blue".to_string()
            ])
        );

        // Test: list + [default: x]
        assert_eq!(
            parse_possible_values(None, Some("type: apple/banana/cherry [default: banana]")),
            Some(vec![
                "apple".to_string(),
                "banana".to_string(),
                "cherry".to_string()
            ])
        );

        // Test: list + (default: x)
        assert_eq!(
            parse_possible_values(None, Some("strategy: eager, lazy (default: eager)")),
            Some(vec!["eager".to_string(), "lazy".to_string()])
        );

        // Test: default value NOT in the list (should return None)
        assert_eq!(
            parse_possible_values(None, Some("mode: fast, slow. default: medium")),
            None
        );

        // Test: default value with no obvious list (should return None)
        assert_eq!(
            parse_possible_values(None, Some("Set the timeout in seconds. default: 30")),
            None
        );

        // Test: multiple lists, default matches one
        assert_eq!(
            parse_possible_values(
                None,
                Some("format: json, xml, yaml. output: file, stdout. default: xml")
            ),
            Some(vec![
                "json".to_string(),
                "xml".to_string(),
                "yaml".to_string()
            ])
        );

        // Test: list with 'and' conjunction
        assert_eq!(
            parse_possible_values(None, Some("choose from red, green and blue. default: red")),
            Some(vec![
                "red".to_string(),
                "green".to_string(),
                "blue".to_string()
            ])
        );

        // Test: quoted values in list and default
        assert_eq!(
            parse_possible_values(
                None,
                Some("modes: 'active', 'inactive'. default is 'active'")
            ),
            Some(vec!["active".to_string(), "inactive".to_string()])
        );

        // Test: values with dashes and underscores
        assert_eq!(
            parse_possible_values(
                None,
                Some("use: my-value, other_value. default: other_value")
            ),
            Some(vec!["my-value".to_string(), "other_value".to_string()])
        );

        // Test: default at the beginning
        assert_eq!(
            parse_possible_values(None, Some("default: fast. choices are fast, slow.")),
            Some(vec!["fast".to_string(), "slow".to_string()])
        );

        // Test: list in parentheses
        assert_eq!(
            parse_possible_values(None, Some("the mode (fast, slow, medium). default: slow")),
            Some(vec![
                "fast".to_string(),
                "slow".to_string(),
                "medium".to_string()
            ])
        );

        // Test: default is x, where x is in the list with other text
        assert_eq!(
            parse_possible_values(
                None,
                Some("Available options are: first, second, third. The default is second.")
            ),
            Some(vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string()
            ])
        );

        // Test: multiple potential lists, should pick the one containing default
        assert_eq!(
            parse_possible_values(
                None,
                Some("birds: eagle, hawk. fish: shark, trout. default: shark")
            ),
            Some(vec!["shark".to_string(), "trout".to_string()])
        );
    }

    #[test]
    fn test_extract_value_hint() {
        // Test URL
        assert_eq!(extract_value_hint(Some("url"), None), ValueHint::Url);
        assert_eq!(
            extract_value_hint(None, Some("the API endpoint")),
            ValueHint::Url
        );

        // Test Hostname
        assert_eq!(extract_value_hint(Some("host"), None), ValueHint::Hostname);
        assert_eq!(
            extract_value_hint(None, Some("bind to the server ip address")),
            ValueHint::Hostname
        );
        assert_eq!(
            extract_value_hint(None, Some("target host remote name")),
            ValueHint::Hostname
        );

        // Test ExecutablePath
        assert_eq!(
            extract_value_hint(Some("executable"), None),
            ValueHint::ExecutablePath
        );
        assert_eq!(
            extract_value_hint(None, Some("path to executable command")),
            ValueHint::ExecutablePath
        );
        assert_eq!(
            extract_value_hint(None, Some("path to binary")),
            ValueHint::ExecutablePath
        );

        // Test DirPath vs FilePath in output/input specific heuristic
        assert_eq!(
            extract_value_hint(Some("output"), Some("write output to directory")),
            ValueHint::DirPath
        );
        assert_eq!(
            extract_value_hint(Some("output"), Some("write output to file")),
            ValueHint::FilePath
        );

        // Test EnvVar
        assert_eq!(extract_value_hint(Some("env-var"), None), ValueHint::EnvVar);
        assert_eq!(
            extract_value_hint(None, Some("read from environment variable")),
            ValueHint::EnvVar
        );

        // Test NetworkInterface
        assert_eq!(
            extract_value_hint(Some("iface"), None),
            ValueHint::NetworkInterface
        );
        assert_eq!(
            extract_value_hint(None, Some("bind network interface")),
            ValueHint::NetworkInterface
        );

        // Test GitBranch / GitRevision
        assert_eq!(
            extract_value_hint(Some("branch"), None),
            ValueHint::GitBranch
        );
        assert_eq!(
            extract_value_hint(None, Some("git commit hash")),
            ValueHint::GitRevision
        );

        // Test SystemdUnit
        assert_eq!(
            extract_value_hint(Some("service"), None),
            ValueHint::SystemdUnit
        );
        assert_eq!(
            extract_value_hint(None, Some("systemd unit service")),
            ValueHint::SystemdUnit
        );
    }

    #[test]
    fn test_merge_commands() {
        let help_cmd = Command {
            name: Some("test".to_string()),
            description: None,
            args: vec![Arg {
                long: Some("--config".to_string()),
                description: Some("Config path".to_string()),
                value_name: Some("PATH".to_string()),
                value_hint: ValueHint::Unknown,
                ..Default::default()
            }],
            ..Command::default()
        };

        let man_cmd = Command {
            name: Some("test".to_string()),
            description: Some("A parsed test tool description".to_string()),
            args: vec![Arg {
                long: Some("--config".to_string()),
                description: None,
                value_name: Some("PATH".to_string()),
                value_hint: ValueHint::FilePath,
                value_enum: Some(vec!["config.toml".to_string()]),
                ..Default::default()
            }],
            ..Command::default()
        };

        let merged = merge_commands(help_cmd, man_cmd);

        assert_eq!(
            merged.description,
            Some("A parsed test tool description".to_string())
        );
        assert_eq!(merged.args[0].value_hint, ValueHint::FilePath);
        assert_eq!(
            merged.args[0].value_enum,
            Some(vec!["config.toml".to_string()])
        );
        assert_eq!(merged.args[0].description, Some("Config path".to_string()));
    }
}
