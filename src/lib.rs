pub const SCHEMA_VERSION: u8 = 2;

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
