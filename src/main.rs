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
    /// Record a decision (with grounds + roads-not-taken)
    Decide {
        decision: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Attach an existing test to a decision's ground (writes a new child)
    Guard {
        selector: String,
        id: String,
        target: Option<String>,
        #[arg(long)]
        counter_test: String,
        #[arg(long = "on-platform")]
        platforms: Vec<String>,
        #[arg(long = "triggered-by")]
        triggered_by: Vec<String>,
        #[arg(long = "surface")]
        surfaces: Vec<String>,
        #[arg(long)]
        verified_at_sha: Option<String>,
        #[arg(long)]
        blame: Option<String>,
    },
    /// Evaluate bound checks against cached receipts (read-only) and report a flat verdict set.
    Check {
        /// Exit non-zero if any ground is not green.
        #[arg(long)]
        exit_on_red: bool,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let repo = std::env::current_dir().expect("cwd");
    match cli.cmd {
        Cmd::Init => ev::cmd::init(&repo),
        Cmd::Show { id } => ev::cmd::show(&repo, &id),
        Cmd::Verify { self_test } => ev::cmd::verify_cmd(&repo, self_test),
        Cmd::Decide { decision, args } => ev::cmd::decide(&repo, &decision, &args),
        Cmd::Guard {
            selector,
            id,
            target,
            counter_test,
            platforms,
            triggered_by,
            surfaces,
            verified_at_sha,
            blame,
        } => ev::cmd::guard(
            &repo,
            ev::guard::GuardArgs {
                selector,
                id,
                target,
                counter_test,
                platforms,
                triggered_by,
                surfaces,
                verified_at_sha,
                blame,
            },
        ),
        Cmd::Check { exit_on_red } => ev::cmd::check(&repo, exit_on_red),
    }
}
