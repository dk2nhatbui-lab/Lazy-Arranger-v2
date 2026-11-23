//! Isolated VST3 scanner used by the plugin manager.
//!
//! Loads one bundle in this throwaway process and prints its audio classes as
//! JSON. A crash or timeout stays in the child.

use std::path::Path;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: lazy-arranger-plugin-scan <bundle.vst3>");
        std::process::exit(2);
    };
    match plugin_host::scan_vst3(Path::new(&path)) {
        Ok(classes) => match serde_json::to_string(&classes) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("serialize: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
