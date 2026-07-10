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
    /// Attach evidence to a claim (typed ref). Agents may do this.
    Evidence { claim: String, evidence_ref: String },
    /// Re-verify a claim's evidence (or all open claims).
    Verify { claim: Option<String> },
    /// The daily glance: returned demands, open claims, grey.
    Brief {
        #[arg(long)]
        json: bool,
    },
    /// Close a claim (needs evidence, or --dead --reason).
    Close {
        claim: String,
        #[arg(long)]
        dead: bool,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long = "i-am-the-human")]
        i_am_the_human: bool,
    },
    /// Move a claim to grey with a reason.
    Hold {
        claim: String,
        #[arg(long)]
        reason: String,
        #[arg(long = "i-am-the-human")]
        i_am_the_human: bool,
    },
    /// Bounce a claim back for evidence (leads the next brief).
    Demand {
        claim: String,
        #[arg(long = "i-am-the-human")]
        i_am_the_human: bool,
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
        Some(Command::Evidence {
            claim,
            evidence_ref,
        }) => evolving::cmd::evidence(claim, evidence_ref),
        Some(Command::Verify { claim }) => evolving::cmd::verify_cmd(claim),
        Some(Command::Brief { json }) => evolving::cmd::brief(json),
        Some(Command::Close {
            claim,
            dead,
            reason,
            i_am_the_human,
        }) => evolving::cmd::close(evolving::cmd::CloseArgs {
            claim,
            dead,
            reason,
            i_am_the_human,
        }),
        Some(Command::Hold {
            claim,
            reason,
            i_am_the_human,
        }) => evolving::cmd::hold(claim, reason, i_am_the_human),
        Some(Command::Demand {
            claim,
            i_am_the_human,
        }) => evolving::cmd::demand(claim, i_am_the_human),
    };
    if let Err(e) = result {
        eprintln!("ev: {e}");
        std::process::exit(e.exit_code());
    }
}
