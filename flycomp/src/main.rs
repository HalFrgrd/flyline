use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "flycomp")]
#[command(about = "Generate shell completions from COMMAND --help output")]
struct CliArgs {
    /// Command name or path to synthesize completions for.
    command: String,
    /// Output format (defaults to bash).
    #[arg(long, value_enum, default_value_t = flycomp::OutputFormat::Bash)]
    output: flycomp::OutputFormat,
    /// Parsing strategy.
    #[arg(long, value_enum, default_value_t = flycomp::SynthesisStrategy::default())]
    strategy: flycomp::SynthesisStrategy,
    /// Run execution unsandboxed (bypass bubblewrap/bwrap sandboxing).
    #[arg(long)]
    no_sandbox: bool,
}

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    let output = flycomp::generate_completion_output(
        &args.command,
        args.output,
        args.strategy,
        !args.no_sandbox,
    )?;
    print!("{}", output);

    Ok(())
}
