pub const SCHEMA_VERSION: u8 = 2;

/// Short display form of an event id: `<prefix>_<first-6-of-ulid>`.
pub fn short_id(id: &str) -> String {
    match id.split_once('_') {
        Some((p, rest)) => format!("{p}_{}", rest.chars().take(6).collect::<String>()),
        None => id.to_string(),
    }
}

pub type Result<T> = std::result::Result<T, EvError>;

#[derive(Debug)]
pub enum EvError {
    Refusal(String),
    Failure(String),
}

impl EvError {
    pub fn exit_code(&self) -> i32 {
        match self {
            EvError::Refusal(_) => 1,
            EvError::Failure(_) => 2,
        }
    }
}

impl std::fmt::Display for EvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvError::Refusal(m) | EvError::Failure(m) => write!(f, "{m}"),
        }
    }
}

impl From<std::io::Error> for EvError {
    fn from(e: std::io::Error) -> Self {
        EvError::Failure(e.to_string())
    }
}

pub mod cmd;
pub mod exhaust;
pub mod hooks;
pub mod ledger;
pub mod pause;
pub mod render;
pub mod state;
pub mod verify;

/// Run a git subcommand under `root` and return its trimmed stdout, or None on failure.
pub(crate) fn git_output(root: &std::path::Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
