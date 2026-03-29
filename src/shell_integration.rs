use std::io::Write;

use crossterm::Command;
use crossterm::QueueableCommand;
use crossterm::cursor::MoveTo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapeCodes {
    // OSC 133 (FinalTerm)
    PromptStart {
        col: u16,
        row: u16,
    },
    PromptEnd {
        col: u16,
        row: u16,
    },
    PreExecution,
    ExecutionFinished {
        exit_code: Option<i32>,
    },

    // OSC 633 (VS Code)
    VscPromptStart {
        col: u16,
        row: u16,
    },
    VscPromptEnd {
        col: u16,
        row: u16,
    },
    VscPreExecution,
    VscExecutionFinished {
        exit_code: Option<i32>,
    },
    VscCommandLine {
        commandline: String,
        nonce: Option<String>,
    },
}

impl Command for EscapeCodes {
    fn write_ansi(&self, f: &mut impl core::fmt::Write) -> core::fmt::Result {
        match self {
            // OSC 133
            EscapeCodes::PromptStart { .. } => f.write_str("\x1b]133;A\x1b\\"),
            EscapeCodes::PromptEnd { .. } => f.write_str("\x1b]133;B\x1b\\"),
            EscapeCodes::PreExecution => f.write_str("\x1b]133;C\x1b\\"),
            EscapeCodes::ExecutionFinished { exit_code, .. } => match exit_code {
                Some(code) => write!(f, "\x1b]133;D;{}\x1b\\", code),
                None => f.write_str("\x1b]133;D\x1b\\"),
            },

            // OSC 633
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
                Some(n) => write!(f, "\x1b]633;E;{};{}\x1b\\", commandline, n),
                None => write!(f, "\x1b]633;E;{}\x1b\\", commandline),
            },
        }
    }
}

pub fn write_escape_codes(codes: &[EscapeCodes]) -> std::io::Result<()> {
    let mut queue = std::io::stdout();

    for code in codes {
        let position = match code {
            EscapeCodes::PromptStart { col, row }
            | EscapeCodes::PromptEnd { col, row }
            | EscapeCodes::VscPromptStart { col, row }
            | EscapeCodes::VscPromptEnd { col, row } => Some((*col, *row)),
            _ => None,
        };
        if let Some((col, row)) = position {
            log::debug!(
                "Moving cursor to ({}, {}) for escape code: {:?}",
                col,
                row,
                code
            );
            queue.queue(MoveTo(col, row))?;
        }
        log::debug!("Writing escape code: {:?}", code);
        queue.queue(code)?;
    }
    queue.flush()?;
    Ok(())
}
