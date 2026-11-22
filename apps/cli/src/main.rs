//! Binary entry point. The real code lives in the library so tests can drive it.

use std::process::ExitCode;

fn main() -> ExitCode {
    lazy_arranger_cli::cli_main()
}
