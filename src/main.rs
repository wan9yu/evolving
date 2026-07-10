use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ev",
    version,
    about = "A closure engine for one human and their agent fleet."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Create .evolving/ here and register the repo.
    Init,
    /// Note a thought (optionally pinned).
    Think {
        label: String,
        #[arg(long)]
        pin: bool,
    },
    /// File a claim. Bare unless --evidence is given.
    Claim {
        label: String,
        #[arg(long)]
        evidence: Option<String>,
        #[arg(long = "by", value_parser = ["agent", "human"], default_value = "human")]
        by: String,
        #[arg(long = "source-ref")]
        source_ref: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        None => {
            println!(
                "ev — run `ev --help`. Nothing runs in the background; ev refreshes when invoked."
            );
            Ok(())
        }
        Some(Command::Init) => evolving::cmd::init(),
        Some(Command::Think { label, pin }) => evolving::cmd::think(label, pin),
        Some(Command::Claim {
            label,
            evidence,
            by,
            source_ref,
        }) => evolving::cmd::claim(evolving::cmd::ClaimArgs {
            label,
            evidence,
            by_agent: by == "agent",
            source_ref,
        }),
    };
    if let Err(e) = result {
        eprintln!("ev: {e}");
        std::process::exit(e.exit_code());
    }
}
