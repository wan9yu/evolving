use std::path::Path;
use std::process::ExitCode;

pub fn init(repo: &Path) -> ExitCode { let _ = repo; ExitCode::SUCCESS }
pub fn show(repo: &Path, id: &str) -> ExitCode { let _ = (repo, id); ExitCode::SUCCESS }
pub fn verify_cmd(repo: &Path, self_test: bool) -> ExitCode { let _ = (repo, self_test); ExitCode::SUCCESS }
