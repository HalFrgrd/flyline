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

fn daemon_main_loop() {
    // this doesnt terminate

    // This is where the main daemon functionality would be implemented
    // For demonstration, we'll log periodic messages
    
    let mut counter = 0;
    loop {
        counter += 1;
        
        // Write to log file to show the daemon is working
        if let Ok(mut log_file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/jobu_daemon.log")
        {
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
            writeln!(log_file, "[{}] Jobu daemon is running (iteration {})", timestamp, counter).ok();
        }
        
        // Sleep for 30 seconds
        std::thread::sleep(std::time::Duration::from_secs(30));
        
        // In a real implementation, this would be doing actual work like:
        // - Processing job queues
        // - Monitoring system resources
        // - Handling incoming requests
        // - Managing background tasks
    }
}

fn run_activate() {
    match kill_daemon() {
        Ok(_) => (),
        Err(e) => eprintln!("Warning: {}", e),
    }
    run_daemon(DEFAULT_PID_FILE, "/tmp/jobu.log");
    println!("FORBASH: echo 'hello from jobu'");
}
