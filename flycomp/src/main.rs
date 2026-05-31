use clap::Parser;

#[derive(Clone, Debug, clap::ValueEnum, Default)]
#[value(rename_all = "kebab-case")]
enum Strategy {
    #[default]
    ManPageThenRunHelp,
    ManPage,
    RunHelp,
}

impl From<Strategy> for flycomp::SynthesisStrategy {
    fn from(s: Strategy) -> Self {
        match s {
            Strategy::ManPageThenRunHelp => flycomp::SynthesisStrategy::ManPageThenRunHelp,
            Strategy::ManPage => flycomp::SynthesisStrategy::ManPage,
            Strategy::RunHelp => flycomp::SynthesisStrategy::RunHelp,
        }
    }
}

#[derive(Clone, Debug, clap::ValueEnum)]
#[value(rename_all = "lower")]
enum OutputFormat {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
    Json,
}

#[derive(Parser, Debug)]
#[command(name = "flycomp")]
#[command(about = "Generate shell completions from COMMAND --help output")]
struct CliArgs {
    /// Command name or path to synthesize completions for.
    command: String,
    /// Output format (defaults to bash).
    #[arg(long, value_enum, default_value_t = OutputFormat::Bash)]
    output: OutputFormat,
    /// Parsing strategy.
    #[arg(long, value_enum, default_value_t = Strategy::default())]
    strategy: Strategy,
    /// Run execution unsandboxed (bypass bubblewrap/bwrap sandboxing).
    #[arg(long)]
    no_sandbox: bool,
}

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    if matches!(args.output, OutputFormat::Json) {
        let parsed_cmd = flycomp::synthesize_completion(
            &args.command,
            |extra_args| flycomp::run_help(&args.command, extra_args, !args.no_sandbox),
            args.strategy.clone().into(),
        )?;
        let json = serde_json::to_string_pretty(&parsed_cmd)?;
        println!("{}", json);
    } else {
        let shell = match args.output {
            OutputFormat::Bash => clap_complete::Shell::Bash,
            OutputFormat::Elvish => clap_complete::Shell::Elvish,
            OutputFormat::Fish => clap_complete::Shell::Fish,
            OutputFormat::Powershell => clap_complete::Shell::PowerShell,
            OutputFormat::Zsh => clap_complete::Shell::Zsh,
            OutputFormat::Json => unreachable!(),
        };
        let script = flycomp::generate_completion_script(
            &args.command,
            shell,
            args.strategy.into(),
            !args.no_sandbox,
        )?;
        print!("{}", script);
    }

    Ok(())
}
