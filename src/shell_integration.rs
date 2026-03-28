use std::io::Write;

use crossterm::Command;
use crossterm::QueueableCommand;
use crossterm::cursor::MoveTo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapeCodes {
    PromptStart(u16, u16),
    PromptEnd(u16, u16),
    // PreExecution(u16, u16),
    // PostExecution(u16, u16, Option<i32>), // The optional i32 is the exit code for PostExecution
    // CurrentWorkingDirectory(String),
}

impl Command for EscapeCodes {
    fn write_ansi(&self, f: &mut impl core::fmt::Write) -> core::fmt::Result {
        match self {
            EscapeCodes::PromptStart(_, _) => f.write_str("\x1b]133;A\x1b\\"),
            EscapeCodes::PromptEnd(_, _) => f.write_str("\x1b]133;B\x1b\\"),
            // EscapeCodes::PreExecution(row, col) => write!(f, "\x1b]133;C;{};{}\x1b\\", row, col),
            // EscapeCodes::PostExecution(row, col, exit_code) => {
            //     if let Some(code) = exit_code {
            //         write!(f, "\x1b]133;D;{};{};{}\x1b\\", row, col, code)
            //     } else {
            //         write!(f, "\x1b]133;D;{};{}\x1b\\", row, col)
            //     }
            // }
            // EscapeCodes::CurrentWorkingDirectory(path) => write!(f, "\x1b]133;P;{}\x1b\\", path),
        }
    }
}

pub fn write_escape_codes(codes: &[EscapeCodes]) -> std::io::Result<()> {
    let mut queue = std::io::stdout();

    for code in codes {
        let position_command = match code {
            EscapeCodes::PromptStart(row, col) => Some((*row, *col)),
            EscapeCodes::PromptEnd(row, col) => Some((*row, *col)),
            // EscapeCodes::PreExecution(row, col) => Some((*row, *col)),
            // EscapeCodes::PostExecution(row, col, exit_code) => Some((*row, *col)),
            // EscapeCodes::CurrentWorkingDirectory(path) => Some((*row, *col)),
        };

        if let Some((col, row)) = position_command {
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
