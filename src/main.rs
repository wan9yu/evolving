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
    /// List every decision in the ledger (id, status, decision).
    List,
    /// Show the decision lineage from HEAD back to genesis.
    Log,
    /// Boot-read: the user-ruled decisions and the roads they rejected
    Brief {
        /// Cap the number of decisions shown (overrides config brief_limit; 0 = show all).
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Audit the chain + refusals
    Verify {
        /// reproduce the frozen golden vectors and exit
        #[arg(long)]
        self_test: bool,
    },
    /// Record a decision (with grounds + roads-not-taken)
    Decide {
        /// The decision text; omit it and pass --from-git <commit> to seed from a commit envelope.
        /// allow_hyphen_values lets a leading --from-git reach us; cmd::decide re-routes it into args.
        #[arg(allow_hyphen_values = true)]
        decision: Option<String>,
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
        #[arg(long)]
        authority: Option<String>,
    },
    /// Evaluate bound checks against cached receipts and report a flat verdict set.
    Check {
        /// Exit non-zero if any ground is not green.
        #[arg(long)]
        exit_on_red: bool,
        /// Run each bound test (that declares --platform) locally and record a receipt before evaluating.
        #[arg(long)]
        run: bool,
        /// The platform this --run represents (which declared platform the local run satisfies).
        #[arg(long, default_value = "local")]
        platform: String,
        /// Use only the cached staleness reference; never resolve it fresh (non-blocking).
        #[arg(long)]
        offline: bool,
        /// Platforms THIS runner speaks for (comma-separated). Declared platforms not in this
        /// set are exempt here, not not-run. Omit to attest ALL declared platforms (default).
        #[arg(long, value_delimiter = ',')]
        attest: Vec<String>,
    },
    /// Backfill an existing decision history into the ledger (multi-source, idempotent).
    Migrate {
        /// A source to import, as `<kind>:<path>` — kind ∈ {gitlog, to-human, decisions-immutable,
        /// escalation}. Repeatable; sources are imported in deterministic source_key order.
        #[arg(long = "source")]
        sources: Vec<String>,
        /// Parse + report what WOULD import without writing any tick.
        #[arg(long)]
        dry_run: bool,
        /// Reconcile mode: join the --against source against the store and report the buckets
        /// (in-both / source-only gap / store-only / un-keyable) instead of importing.
        #[arg(long)]
        reconcile: bool,
        /// The source to reconcile against, as `<kind>:<path>` (with --reconcile).
        #[arg(long)]
        against: Option<String>,
        /// Fallback author for any source record carrying no author (R5 stays intact — no fabrication).
        #[arg(long)]
        blame: Option<String>,
        /// Harvest an existing test as a bound check on the named selector (counter_test None, full
        /// liveness still required). Prints the harvested check shape; does not write a tick by itself.
        #[arg(long)]
        bind_check: Option<String>,
        /// The platforms the --bind-check harvest declares (with --bind-check).
        #[arg(long = "on-platform")]
        platforms: Vec<String>,
        /// The triggered-by paths the --bind-check harvest declares (with --bind-check).
        #[arg(long = "triggered-by")]
        triggered_by: Vec<String>,
        /// The surfaces the --bind-check harvest declares (with --bind-check).
        #[arg(long = "surface")]
        surfaces: Vec<String>,
        /// The 40-hex sha the --bind-check harvest was verified at (defaults to HEAD).
        #[arg(long)]
        verified_at_sha: Option<String>,
    },
    /// Reverse lookup: name the decision + ground a test selector guards.
    Why {
        /// The bound test selector to look up.
        selector: String,
    },
    /// Pull the full decision object (decision, grounds + current verdicts, roads-not-taken). Present only.
    Reopen {
        /// The tick id to reopen.
        id: String,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let repo = std::env::current_dir().expect("cwd");
    match cli.cmd {
        Cmd::Init => ev::cmd::init(&repo),
        Cmd::Show { id } => ev::cmd::show(&repo, &id),
        Cmd::List => ev::cmd::list(&repo),
        Cmd::Log => ev::cmd::log(&repo),
        Cmd::Brief { limit } => ev::cmd::brief(&repo, limit),
        Cmd::Verify { self_test } => ev::cmd::verify_cmd(&repo, self_test),
        Cmd::Decide { decision, args } => ev::cmd::decide(&repo, decision.as_deref(), &args),
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
            authority,
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
                authority,
            },
        ),
        Cmd::Check {
            exit_on_red,
            run,
            platform,
            offline,
            attest,
        } => ev::cmd::check(&repo, exit_on_red, run, &platform, offline, attest),
        Cmd::Migrate {
            sources,
            dry_run,
            reconcile,
            against,
            blame,
            bind_check,
            platforms,
            triggered_by,
            surfaces,
            verified_at_sha,
        } => ev::cmd::migrate(
            &repo,
            ev::cmd::MigrateArgs {
                sources,
                dry_run,
                reconcile,
                against,
                blame,
                bind_check,
                platforms,
                triggered_by,
                surfaces,
                verified_at_sha,
            },
        ),
        Cmd::Why { selector } => ev::cmd::why(&repo, &selector),
        Cmd::Reopen { id } => ev::cmd::reopen(&repo, &id),
    }
}
