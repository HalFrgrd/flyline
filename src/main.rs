use clap::{Parser, Subcommand};
use daemonize::Daemonize;
use std::fs::File;
use std::process;
use nix;
use std::fs::OpenOptions;
use std::io::Write;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::path::Path;
use std::io::{BufRead, BufReader};
use crossterm::{terminal, Command};

#[derive(Parser)]
#[command(name = "jobu")]
#[command(about = "A job management utility")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}


#[derive(Subcommand)]
enum Commands {
    Activate 
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Activate => {
            run_activate();
        }
    }
}

fn display_center_message(tty_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::OpenOptions;
    use std::io::Write;
    
    // Open the TTY device for writing
    let mut tty = OpenOptions::new().write(true).open(tty_path)?;
    
    // Get terminal size using a simple approach
    // We'll use the TIOCGWINSZ ioctl through crossterm
    let (cols, rows) = terminal::size()?;
    
    // Calculate center position
    let center_col = cols / 2;
    let center_row = rows / 2;
    
    // Calculate position to center the text "hello from jobu" (15 characters)
    let text = "hello from jobu";
    
    crossterm::execute!(tty,
        crossterm::cursor::SavePosition,
        crossterm::cursor::MoveTo(center_col, center_row)
    )?;
    write!(tty, "\x1b[31m{}\x1b[0m", text)?; // Red text and reset
    crossterm::execute!(tty, crossterm::cursor::RestorePosition)?;
    tty.flush()?;
    
    Ok(())
}

fn run_activate() {

    const ACTIVATE_SCRIPT: &str = include_str!("activate.sh");
    for line in ACTIVATE_SCRIPT.lines() {
        println!("FORBASH: {}", line);
    }
}
