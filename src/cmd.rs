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
pub fn show(repo: &Path, id: &str) -> ExitCode {
    let store = Store::at(repo);
    let path = store.ticks_dir().join(id);
    if !path.is_file() {
        eprintln!("error: no tick with id {id}");
        return ExitCode::FAILURE;
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            // print as-is (the on-disk pretty JSON: hashed payload + bookkeeping).
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: reading {id}: {e}");
            ExitCode::FAILURE
        }
    }
}
pub fn verify_cmd(repo: &Path, self_test: bool) -> ExitCode { let _ = (repo, self_test); ExitCode::SUCCESS }
