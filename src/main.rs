use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ev", version, about = "git for decisions")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create the .evolving/ store
    Init,
    /// Print one decision in full
    Show { id: String },
    /// Audit the chain + refusals
    Verify {
        /// reproduce the frozen golden vectors and exit
        #[arg(long)]
        self_test: bool,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let repo = std::env::current_dir().expect("cwd");
    match cli.cmd {
        Cmd::Init => ev::cmd::init(&repo),
        Cmd::Show { id } => ev::cmd::show(&repo, &id),
        Cmd::Verify { self_test } => ev::cmd::verify_cmd(&repo, self_test),
    }
}
