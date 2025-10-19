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

const DEFAULT_PID_FILE: &str = "/tmp/jobu.pid";

#[derive(Subcommand)]
enum Commands {
    /// Run the application as a background daemon
    Daemonize ,
    Activate,
    KillAll,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemonize => {
            run_daemon(DEFAULT_PID_FILE, "/tmp/jobu.log");
        }
        Commands::Activate => {
            run_activate();
        }
        Commands::KillAll => {
            kill_daemon().unwrap();
        }
    }
}

fn kill_daemon() -> Result<(), Box<dyn std::error::Error>> {
    // Implementation for killing all jobu processes would go here
    // This could involve reading the PID file and sending termination signals
    // to the processes, or using system commands to find and kill them.
    println!("Killing all jobu processes..."); 
    let jobu_pid = std::fs::read_to_string(DEFAULT_PID_FILE)?;
    let pid: i32 = jobu_pid.trim().parse()?;
    if pid <= 0 {
        return Err(format!("Invalid PID {} found in {}", pid, DEFAULT_PID_FILE).into());
    }

    let target = Pid::from_raw(pid);
    match kill(target, Signal::SIGTERM) {
        Ok(_) => {
            println!("Sent SIGTERM to {}", pid);
            std::thread::sleep(std::time::Duration::from_secs(1));

            let proc_path = format!("/proc/{}", pid);
            let proc_path = Path::new(&proc_path);
            if proc_path.exists() {
                println!("Process {} still alive; sending SIGKILL...", pid);
                if let Err(e) = kill(target, Signal::SIGKILL) {
                    eprintln!("Failed to send SIGKILL to {}: {}", pid, e);
                    process::exit(1);
                } else {
                    println!("Sent SIGKILL to {}", pid);
                }
            } else {
                println!("Process {} terminated.", pid);
            }

            std::fs::remove_file(DEFAULT_PID_FILE)?;
            Ok(())
        }
        Err(nix::errno::Errno::ESRCH) => {
            
            std::fs::remove_file(DEFAULT_PID_FILE)?;
            Err(format!("No such process: {}", pid).into())
        }
        Err(e) => {
            Err(e.into())
        }
    }
    
    
}

fn run_daemon(pid_file: &str, log_file: &str)  {
    println!("Starting jobu daemon...");
    println!("PID file: {}", pid_file);
    println!("Log file: {}", log_file);

    let stdout = File::create(&log_file).unwrap_or_else(|e| {
        eprintln!("Failed to create log file {}: {}", log_file, e);
        process::exit(1);
    });

    let stderr = File::create(&format!("{}.err", log_file)).unwrap_or_else(|e| {
        eprintln!("Failed to create error log file: {}", e);
        process::exit(1);
    });

    // Create a more permissive daemon configuration for development
    let daemonize = Daemonize::new()
        .pid_file(pid_file)
        .chown_pid_file(true)
        .working_directory("/tmp")
        .umask(0o022)  // More permissive umask
        .stdout(stdout)
        .stderr(stderr);

    match daemonize.execute() {
            daemonize::Outcome::Parent(Ok(_)) => {
                // println!("Daemon started successfully.");
            },
            daemonize::Outcome::Parent(Err(err)) => {
                eprintln!("Error daemonizing: {}", err);
                // process::exit(1);
            },
            daemonize::Outcome::Child(Ok(_)) => daemon_main_loop(),
            daemonize::Outcome::Child(Err(err)) => {
                eprintln!("Error in daemon child process: {}", err);
                process::exit(1);
            },
    }
}

const JOBU_FIFO_PATH: &str = "/tmp/jobu.fifo";

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

fn daemon_main_loop() {
    // this doesnt terminate

    // This is where the main daemon functionality would be implemented
    // For demonstration, we'll log periodic messages
    let mut log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/jobu_daemon.log")
            .unwrap();

    // Remove existing FIFO if it exists
    if Path::new(JOBU_FIFO_PATH).exists() {
        std::fs::remove_file(JOBU_FIFO_PATH).unwrap();
    }

    // Create FIFO using system command
    std::process::Command::new("mkfifo")
        .arg(JOBU_FIFO_PATH)
        .output()
        .expect("Failed to create FIFO");

    
    loop {

        // Open FIFO for reading
        match File::open(JOBU_FIFO_PATH) {
            Ok(file) => {
                let reader = BufReader::new(file);
                for line in reader.lines() {
                    match line {
                        Ok(content) => {
                            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
                            writeln!(log_file, "[{}] {}", timestamp, content).ok();
                            if content.contains("tty=") {
                                // do something special
                                let tty = content.split("tty=").nth(1).unwrap_or("");
                                writeln!(log_file, "[{}] Found TTY: {}", timestamp, tty).ok();
                                
                                // Display red message in center of terminal
                                if let Err(e) = display_center_message(tty) {
                                    writeln!(log_file, "[{}] Error displaying message: {}", timestamp, e).ok();
                                }
                            }
                        }
                        Err(e) => {
                            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
                            writeln!(log_file, "[{}] Error reading line: {}", timestamp, e).ok();
                        }
                    }
                }
            }
            Err(e) => {
                let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
                writeln!(log_file, "[{}] Error opening FIFO: {}", timestamp, e).ok();
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
}

fn run_activate() {
    match kill_daemon() {
        Ok(_) => (),
        Err(e) => eprintln!("Warning: {}", e),
    }
    run_daemon(DEFAULT_PID_FILE, "/tmp/jobu.log");
    
    // Wait a moment for daemon to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    // find the /dev/tty of the current terminal
    let tty = std::fs::read_link("/proc/self/fd/0").unwrap();
    println!("Current terminal TTY: {}", tty.display());

    // Write to FIFO
    match OpenOptions::new().write(true).open(JOBU_FIFO_PATH) {
        Ok(mut file) => {
            writeln!(file, "starting up with tty={}", tty.display()).unwrap();
        }
        Err(e) => {
            eprintln!("Failed to write to FIFO: {}", e);
        }
    }

    // Read and print each line of activate.sh with FORBASH: prefix (compile-time inclusion)
    println!("FORBASH: export JOBU_FIFO_PATH={}", JOBU_FIFO_PATH);

    const ACTIVATE_SCRIPT: &str = include_str!("activate.sh");
    for line in ACTIVATE_SCRIPT.lines() {
        println!("FORBASH: {}", line);
    }
}
