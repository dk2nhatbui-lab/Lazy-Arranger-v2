//! Binary entry point. The real code lives in the library so tests can drive it.

use std::process::ExitCode;

/// The clap command tree is large in debug builds and exceeds the 1 MiB default
/// main-thread stack on Windows; give the CLI a roomy thread of its own.
const STACK_SIZE: usize = 32 * 1024 * 1024;

fn main() -> ExitCode {
    match std::thread::Builder::new()
        .name(String::from("lazy-arranger"))
        .stack_size(STACK_SIZE)
        .spawn(lazy_arranger_cli::cli_main)
    {
        Ok(handle) => handle.join().unwrap_or(ExitCode::FAILURE),
        Err(_) => lazy_arranger_cli::cli_main(),
    }
}
