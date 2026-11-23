//! Exercises the host against the workspace's own test gain plug-in.

use std::path::PathBuf;

fn dylib_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let debug = exe.parent()?.parent()?;
    let name = if cfg!(target_os = "macos") {
        "liblazy_arranger_test_plugin.dylib"
    } else if cfg!(target_os = "windows") {
        "lazy_arranger_test_plugin.dll"
    } else {
        "liblazy_arranger_test_plugin.so"
    };
    let path = debug.join(name);
    path.is_file().then_some(path)
}

#[cfg(target_os = "macos")]
#[test]
fn the_test_gain_plugin_reports_params_processes_and_has_latency() {
    let Some(library) = dylib_path() else {
        return;
    };
    let root =
        std::env::temp_dir().join(format!("lazy-arranger-test-plugin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let macos = root.join("Test.vst3").join("Contents").join("MacOS");
    std::fs::create_dir_all(&macos).expect("bundle");
    std::fs::copy(&library, macos.join("Test")).expect("copy");
    let bundle = root.join("Test.vst3");
    let classes = plugin_host::scan_vst3(&bundle).expect("scan");
    assert!(!classes.is_empty());
    let mut effect =
        plugin_host::Vst3Effect::new(&bundle, &classes[0].cid, 48_000.0, 512).expect("effect");
    assert!(effect.parameter_count() > 0, "gain parameter missing");
    assert_eq!(effect.latency_samples(), 32);
    effect.set_parameter(0, 0.5);
    let mut samples = vec![1.0_f32; 512 * 2];
    effect.process_stereo(&mut samples).expect("process");
    let late = samples[200 * 2];
    assert!(
        (late - 0.5).abs() < 0.2,
        "parameter should scale the delayed output, got {late}"
    );
    let state = effect.save_state_base64().expect("state");
    assert!(!state.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
