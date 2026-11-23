//! Integration test for the isolated scanner binary.

use std::process::Command;

#[test]
#[ignore = "loads an installed VST3 plug-in"]
fn scanner_binary_prints_classes_as_json() {
    let exe = env!("CARGO_BIN_EXE_lazy-arranger-plugin-scan");
    let output = Command::new(exe)
        .arg("/Library/Audio/Plug-Ins/VST3/R0Verb.vst3")
        .output()
        .expect("run scanner");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\"name\":\"R0Verb\""), "{text}");
    assert!(text.contains("\"cid\":\"56535472767262723076657262000000\""));
}
