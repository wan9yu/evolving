use std::path::Path;
use std::process::ExitCode;
use crate::store::Store;

pub fn init(repo: &Path) -> ExitCode {
    let store = Store::at(repo);
    match store.init() {
        Ok(true) => {
            println!("created .evolving/  (content-addressed chain + results cache)");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!(".evolving/ already exists (no-op)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not create .evolving/: {e}");
            ExitCode::FAILURE
        }
    }
}
pub fn show(repo: &Path, id: &str) -> ExitCode { let _ = (repo, id); ExitCode::SUCCESS }
pub fn verify_cmd(repo: &Path, self_test: bool) -> ExitCode { let _ = (repo, self_test); ExitCode::SUCCESS }
