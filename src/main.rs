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
    };
    if let Err(e) = result {
        eprintln!("ev: {e}");
        std::process::exit(e.exit_code());
    }
}
