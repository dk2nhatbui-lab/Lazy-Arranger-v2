//! Main-thread smoke check for native macOS VST3 and Audio Unit editor windows.
//!
//! Usage:
//!   ./scripts/rust.sh run -p plugin-host --example native_editor_smoke -- vst3 /Library/Audio/Plug-Ins/VST3/R0Verb.vst3
//!   ./scripts/rust.sh run -p plugin-host --example native_editor_smoke -- au appl:dcmp

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let kind = args
        .next()
        .ok_or_else(|| String::from("usage: native_editor_smoke <vst3|au> <path|mfr:sub>"))?;
    let target = args
        .next()
        .ok_or_else(|| String::from("missing plug-in path or AU identifier"))?;
    match kind.as_str() {
        "vst3" => {
            let bundle = std::path::Path::new(&target);
            let classes = plugin_host::scan_vst3(bundle)?;
            let mut editor = plugin_host::Vst3Editor::open(
                &classes[0].name,
                bundle,
                &classes[0].cid,
                "",
                48_000.0,
            )?;
            let (w, h) = editor.size();
            let open = editor.is_open();
            let state = editor.save_state_base64().map(|s| s.len()).unwrap_or(0);
            println!(
                "VST3 native editor OK: {} ({w}x{h}), visible={open}, state_b64={state} chars",
                classes[0].name
            );
            editor.close();
            Ok(())
        }
        "au" => {
            let mut editor = plugin_host::AuEditor::open(&target, &target, "", 48_000.0)?;
            let open = editor.is_open();
            let custom = editor.used_custom_view();
            let state = editor.save_parameter_state();
            println!(
                "AU native editor OK: {target}, visible={open}, custom_cocoa={custom}, params_len={}",
                state.len()
            );
            editor.close();
            Ok(())
        }
        other => Err(format!("unknown kind {other:?}; use vst3 or au")),
    }
}
