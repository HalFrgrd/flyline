use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "flycomp")]
#[command(about = "Generate shell completions from COMMAND --help output")]
struct CliArgs {
    /// Command name or path to synthesize completions for.
    command: String,
    /// Output shell type (defaults to bash).
    #[arg(long, value_enum, default_value_t = clap_complete::Shell::Bash)]
    shell: clap_complete::Shell,
}

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    let script = flycomp::generate_completion_script(&args.command, args.shell)?;
    print!("{}", script);
    Ok(())
}
