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
enum Command {}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        None => {
            println!(
                "ev — run `ev --help`. Nothing runs in the background; ev refreshes when invoked."
            );
        }
    }
}
