use std::path::PathBuf;

use clap::{Parser, Subcommand};
use simple_logging;

mod app;
mod bash_coms;
mod cursor_animation;
mod events;

#[derive(Parser)]
#[command(name = "jobu")]
#[command(about = "A replacement for bash readline")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Activate,
    GetCommand {
        request_pipe: PathBuf,
        response_pipe: PathBuf,
    },
}

fn main() {
    // Initialize logging first
    if let Err(e) = setup_logging() {
        eprintln!("Failed to setup logging: {}", e);
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Activate => {
            run_activate();
        }
        Commands::GetCommand {
            request_pipe,
            response_pipe,
        } => {
            log::info!("Starting GetCommand operation");
            let runtime = build_runtime();

            let command = runtime.block_on(app::get_command(request_pipe, response_pipe));
            log::debug!("Retrieved command: {}", command);
            // print on stderr because we will be drawing on stdout
            eprintln!("{}", command);
            println!("\n");
            log::info!("GetCommand operation completed");
        }
    }
}

fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
    use std::env;
    use std::path::PathBuf;

    // Get home directory
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let log_file_path = PathBuf::from(home_dir).join("jobu.logs");

    // Initialize simple-logging to write to file
    simple_logging::log_to_file(&log_file_path, log::LevelFilter::Debug)?;

    log::info!(
        "Jobu logging initialized, output will be logged to: {}",
        log_file_path.display()
    );

    Ok(())
}

fn build_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

// fn display_center_message(tty_path: &str) -> Result<(), Box<dyn std::error::Error>> {
//     use std::fs::OpenOptions;
//     use std::io::Write;

//     // Open the TTY device for writing
//     let mut tty = OpenOptions::new().write(true).open(tty_path)?;

//     // Get terminal size using a simple approach
//     // We'll use the TIOCGWINSZ ioctl through crossterm
//     let (cols, rows) = terminal::size()?;

//     // Calculate center position
//     let center_col = cols / 2;
//     let center_row = rows / 2;

//     let text = "JOBU ACTIVE";

//     crossterm::execute!(tty,
//         crossterm::cursor::SavePosition,
//         crossterm::cursor::MoveTo(center_col, center_row)
//     )?;
//     tty.write_all(format!("\x1b[31m{}\x1b[0m", text).as_bytes())?; // Red text and reset
//     crossterm::execute!(tty, crossterm::cursor::RestorePosition)?;
//     tty.flush()?;

//     Ok(())
// }

fn run_activate() {
    log::info!("Starting jobu activation");

    // display_center_message("/dev/tty").unwrap_or_else(|err| {
    //     error!("Error displaying message: {}", err);
    // });

    let path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), file!());
    let path = path.trim_end_matches("src/main.rs");
    const EXEC_PATH: &str = "/target/debug/jobu";

    let exec_path = path.to_owned() + EXEC_PATH;
    log::debug!("Setting JOBU_EXEC_PATH to: {}", exec_path);
    println!("export JOBU_EXEC_PATH={}", exec_path);

    const ACTIVATE_SCRIPT: &str = include_str!("activate.sh");
    log::info!(
        "Executing activate script with {} lines",
        ACTIVATE_SCRIPT.lines().count()
    );

    for line in ACTIVATE_SCRIPT.lines() {
        println!("{}", line);
    }

    log::info!("Jobu activation completed");
}
