use std::io::Write;

use crossterm::Command;
use crossterm::QueueableCommand;
use crossterm::cursor::{MoveTo, RestorePosition, SavePosition};

use crate::{bash_funcs, bash_symbols};

/// https://code.visualstudio.com/docs/terminal/shell-integration#_supported-escape-sequences
/// https://sw.kovidgoyal.net/kitty/shell-integration/
/// https://ghostty.org/docs/features/shell-integration#troubleshooting
/// Ghostty right click -> Terminal inspector -> Terminal IO -> Filter for osc
/// And you click on the event to see more details.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapeCodes {
    // OSC 7
    CurrentDirectory {
        host: String,
        path: String,
    },
    KittyCurrentDirectory {
        host: String,
        path: String,
    },

    // OSC 133 (FinalTerm)
    PromptStart {
        col: u16,
        row: u16,
    },
    PromptEnd {
        col: u16,
        row: u16,
    },
    PreExecution {
        commandline: Option<String>,
    },
    ExecutionFinished {
        exit_code: Option<i32>,
    },

    // OSC 633 (VS Code)
    VscProperties {
        cwd: String,
        has_rich_command_detection: bool,
    },
    VscPromptStart {
        col: u16,
        row: u16,
    },
    VscPromptEnd {
        col: u16,
        row: u16,
    },
    VscPreExecution, // Vs Code has another escape code for sending the command
    VscExecutionFinished {
        exit_code: Option<i32>,
    },
    VscCommandLine {
        commandline: String,
        nonce: Option<String>,
    },
}

impl EscapeCodes {
    fn osc_133_encode_string(s: &str) -> String {
        // encoded by %q means the encoding produced by the %q format to printf in bash and similar shells
        if s.is_empty() {
            return "''".to_string();
        }

        let needs_quoting = s.chars().any(|c| {
            !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '@' | '%' | '+' | ',' | '.' | '/' | '-' | '_' | ':')
        });

        if !needs_quoting {
            return s.to_string();
        }

        let mut result = String::from("$'");
        for c in s.chars() {
            match c {
                '\x07' => result.push_str("\\a"),
                '\x08' => result.push_str("\\b"),
                '\t' => result.push_str("\\t"),
                '\n' => result.push_str("\\n"),
                '\x0B' => result.push_str("\\v"),
                '\x0C' => result.push_str("\\f"),
                '\r' => result.push_str("\\r"),
                '\x1B' => result.push_str("\\E"),
                '\'' => result.push_str("\\'"),
                '\\' => result.push_str("\\\\"),
                c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                    result.push_str(&format!("\\x{:02x}", c as u32));
                }
                c => result.push(c),
            }
        }
        result.push('\'');
        result
    }

    fn vsc_encode_string(s: &str) -> String {
        // The command line can escape ASCII characters using the \xAB format, where AB are the hexadecimal representation of the character code (case insensitive), and escape the \ character using \\. It's required to escape semi-colon (0x3b) and characters 0x20 and below and this is particularly important for new line and semi-colon.
        s.chars()
            .map(|c| {
                if c == '\\' {
                    "\\\\".to_string()
                } else if c as u32 <= 0x20 || c == ';' {
                    format!("\\x{:02x}", c as u32)
                } else {
                    c.to_string()
                }
            })
            .collect()
    }
}

impl Command for EscapeCodes {
    fn write_ansi(&self, f: &mut impl core::fmt::Write) -> core::fmt::Result {
        let bash_pid = unsafe { bash_symbols::shell_pgrp };

        match self {
            // OSC 7
            EscapeCodes::CurrentDirectory { host, path } => {
                write!(f, "\x1b]7;file://{}{}\x1b\\", host, path)
            }
            EscapeCodes::KittyCurrentDirectory { host, path } => {
                write!(f, "\x1b]7;kitty-shell-cwd://{}{}\x1b\\", host, path)
            }
            // OSC 133
            EscapeCodes::PromptStart { .. } => {
                write!(
                    f,
                    "\x1b]133;A;click_events=1;redraw=1;aid={}\x1b\\",
                    bash_pid
                )
            }
            EscapeCodes::PromptEnd { .. } => f.write_str("\x1b]133;B\x1b\\"),
            EscapeCodes::PreExecution { commandline } => match commandline {
                Some(cmd) => write!(
                    f,
                    "\x1b]133;C;cmdline={}\x1b\\",
                    EscapeCodes::osc_133_encode_string(cmd)
                ),
                None => f.write_str("\x1b]133;C\x1b\\"),
            },
            EscapeCodes::ExecutionFinished { exit_code, .. } => match exit_code {
                Some(code) => write!(f, "\x1b]133;D;{};aid={}\x1b\\", code, bash_pid),
                None => write!(f, "\x1b]133;D;aid={}\x1b\\", bash_pid),
            },

            // OSC 633
            EscapeCodes::VscProperties {
                cwd,
                has_rich_command_detection,
            } => write!(
                f,
                "\x1b]633;P;Cwd={};HasRichCommandDetection={}\x1b\\",
                EscapeCodes::vsc_encode_string(cwd),
                if *has_rich_command_detection {
                    "True"
                } else {
                    "False"
                }
            ),
            EscapeCodes::VscPromptStart { .. } => f.write_str("\x1b]633;A\x1b\\"),
            EscapeCodes::VscPromptEnd { .. } => f.write_str("\x1b]633;B\x1b\\"),
            EscapeCodes::VscPreExecution => f.write_str("\x1b]633;C\x1b\\"),
            EscapeCodes::VscExecutionFinished { exit_code, .. } => match exit_code {
                Some(code) => write!(f, "\x1b]633;D;{}\x1b\\", code),
                None => f.write_str("\x1b]633;D\x1b\\"),
            },
            EscapeCodes::VscCommandLine {
                commandline, nonce, ..
            } => match nonce {
                Some(n) => write!(
                    f,
                    "\x1b]633;E;{};{}\x1b\\",
                    EscapeCodes::vsc_encode_string(commandline),
                    n
                ),
                None => write!(
                    f,
                    "\x1b]633;E;{}\x1b\\",
                    EscapeCodes::vsc_encode_string(commandline)
                ),
            },
        }
    }
}

fn is_vscode() -> bool {
    bash_funcs::get_shell_bar_name().as_deref() == Some("vscode")
}

pub fn write_startup_codes(exit_code: i32, hostname: &str, cwd: &str) -> std::io::Result<()> {
    let codes: Vec<EscapeCodes> = if is_vscode() {
        vec![
            EscapeCodes::VscExecutionFinished {
                exit_code: Some(exit_code),
            },
            EscapeCodes::VscProperties {
                cwd: cwd.to_string(),
                has_rich_command_detection: true,
            },
        ]
    } else {
        vec![
            EscapeCodes::ExecutionFinished {
                exit_code: Some(exit_code),
            },
            EscapeCodes::CurrentDirectory {
                host: hostname.to_string(),
                path: cwd.to_string(),
            },
            EscapeCodes::KittyCurrentDirectory {
                host: hostname.to_string(),
                path: cwd.to_string(),
            },
        ]
    };
    write_escape_codes(&codes)
}

pub fn write_after_rendering_codes(
    prompt_start: Option<(u16, u16)>,
    prompt_end: Option<(u16, u16)>,
) -> std::io::Result<()> {
    let mut codes: Vec<EscapeCodes> = vec![];

    if is_vscode() {
        if let Some((col, row)) = prompt_start {
            codes.push(EscapeCodes::VscPromptStart { col, row });
        }
        if let Some((col, row)) = prompt_end {
            codes.push(EscapeCodes::VscPromptEnd { col, row });
        }
    } else {
        if let Some((col, row)) = prompt_start {
            codes.push(EscapeCodes::PromptStart { col, row });
        }
        if let Some((col, row)) = prompt_end {
            codes.push(EscapeCodes::PromptEnd { col, row });
        }
    }

    write_escape_codes(&codes)
}

pub fn write_on_exit_codes(commandline: Option<&str>) -> std::io::Result<()> {
    let codes: Vec<EscapeCodes> = if is_vscode() {
        let nonce = bash_funcs::get_envvar_value("VSCODE_NONCE");
        log::info!("vscode_nonce: {:?}", nonce);
        match commandline {
            Some(cmd) => vec![
                EscapeCodes::VscCommandLine {
                    commandline: cmd.to_string(),
                    nonce,
                },
                EscapeCodes::VscPreExecution,
            ],
            None => vec![EscapeCodes::VscPreExecution],
        }
    } else {
        vec![EscapeCodes::PreExecution {
            commandline: commandline.map(|s| s.to_string()),
        }]
    };
    write_escape_codes(&codes)
}

pub fn write_escape_codes(codes: &[EscapeCodes]) -> std::io::Result<()> {
    let mut queue = std::io::stdout();
    queue.queue(SavePosition)?;

    for code in codes {
        let position = match code {
            EscapeCodes::PromptStart { col, row }
            | EscapeCodes::PromptEnd { col, row }
            | EscapeCodes::VscPromptStart { col, row }
            | EscapeCodes::VscPromptEnd { col, row } => Some((*col, *row)),
            _ => None,
        };
        if let Some((col, row)) = position {
            log::trace!(
                "Moving cursor to ({}, {}) for escape code: {:?}",
                col,
                row,
                code
            );
            queue.queue(MoveTo(col, row))?;
        }
        log::trace!("Writing escape code: {:?}", code);
        queue.queue(code)?;
    }
    queue.queue(RestorePosition)?;
    queue.flush()?;
    Ok(())
}
