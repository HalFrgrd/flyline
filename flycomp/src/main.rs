use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "flycomp")]
#[command(about = "Generate shell completions from COMMAND --help output")]
struct CliArgs {
    /// Command name or path to synthesize completions for.
    #[arg(required_unless_present = "version")]
    command: Option<String>,
    /// Output format (defaults to bash).
    #[arg(long, value_enum, default_value_t = flycomp::OutputFormat::Bash)]
    output: flycomp::OutputFormat,
    /// Parsing strategy.
    #[arg(long, value_enum, default_value_t = flycomp::SynthesisStrategy::default())]
    strategy: flycomp::SynthesisStrategy,
    /// Run execution unsandboxed (bypass bubblewrap/bwrap sandboxing).
    #[arg(long)]
    no_sandbox: bool,
    /// Timeout in milliseconds for running commands.
    #[arg(long, default_value_t = 15000)]
    timeout_ms: u64,
    /// Log level to output to stderr (off, error, warn, info, debug, trace).
    #[arg(long, default_value = "error")]
    log_level: String,
    /// Show version information
    #[arg(long)]
    version: bool,
    /// Maximum depth for recursive subcommand synthesis/exploration.
    #[arg(long, default_value_t = 3)]
    recurse_limit: usize,
}

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        eprintln!("[{}] {}", record.level(), record.args());
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    if args.version {
        println!(
            "flycomp version {} ({}) git:{} built:{}",
            env!("CARGO_PKG_VERSION"),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            env!("GIT_HASH"),
            env!("BUILD_TIME"),
        );
        return Ok(());
    }

    let log_level = match args.log_level.to_lowercase().as_str() {
        "off" => log::LevelFilter::Off,
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => anyhow::bail!("invalid log level: {}", args.log_level),
    };

    if log_level != log::LevelFilter::Off {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log_level);
    }

    let command_str = args.command.as_deref().unwrap_or("");
    match flycomp::generate_completion_output(
        command_str,
        args.output,
        args.strategy,
        !args.no_sandbox,
        args.timeout_ms,
        args.recurse_limit,
    ) {
        Ok(output) => {
            print!("{}", output);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {:#}", e);
            std::process::exit(1);
        }
    }
}
