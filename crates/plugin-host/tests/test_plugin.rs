//! Exercises the host against the workspace's own test gain plug-in.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::PathBuf;

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dylib_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(target_os = "macos") {
        "liblazy_arranger_test_plugin.dylib"
    } else if cfg!(target_os = "windows") {
        "lazy_arranger_test_plugin.dll"
    } else {
        "liblazy_arranger_test_plugin.so"
    };
    let profile_dir = exe.ancestors().skip(1).find(|dir| {
        dir.join("deps").is_dir() || dir.join("build").join("lazy-arranger-test-plugin").is_dir()
    })?;
    let mut candidates = Vec::new();
    for direct in [profile_dir.join("deps").join(name), profile_dir.join(name)] {
        if direct.is_file() {
            candidates.push(direct);
        }
    }
    if let Ok(entries) =
        std::fs::read_dir(profile_dir.join("build").join("lazy-arranger-test-plugin"))
    {
        for entry in entries.flatten() {
            let candidate = entry.path().join("out").join(name);
            if candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }
    candidates.into_iter().max_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn the_test_gain_plugin_reports_params_processes_and_has_latency() {
    let Some(library) = dylib_path() else {
        return;
    };
    let root =
        std::env::temp_dir().join(format!("lazy-arranger-test-plugin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (folder, name) = if cfg!(target_os = "macos") {
        ("MacOS", "Test")
    } else if cfg!(target_arch = "aarch64") {
        ("aarch64-linux", "Test.so")
    } else {
        ("x86_64-linux", "Test.so")
    };
    let binary = root.join("Test.vst3").join("Contents").join(folder);
    std::fs::create_dir_all(&binary).expect("bundle");
    std::fs::copy(&library, binary.join(name)).expect("copy");
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
    if cfg!(target_os = "macos") {
        let view = effect.probe_view().expect("editor view");
        assert!(view.supports_nsview);
        assert_eq!((view.width, view.height), (360, 220));
        assert!(view.can_resize);
    }
    // Dropping the effect calls `ModuleExit` before the library unloads.
    drop(effect);
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "probes installed R0Verb.vst3 native IPlugView"]
fn installed_r0verb_vst3_exposes_nsview_editor() {
    let bundle = std::path::Path::new("/Library/Audio/Plug-Ins/VST3/R0Verb.vst3");
    let classes = plugin_host::scan_vst3(bundle).expect("scan");
    let effect =
        plugin_host::Vst3Effect::new(bundle, &classes[0].cid, 48_000.0, 512).expect("effect");
    let view = effect.probe_view().expect("R0Verb editor view");
    assert!(view.supports_nsview, "R0Verb should support NSView");
    assert!(view.width >= 200 && view.height >= 100, "got {view:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn the_test_synth_renders_a_note() {
    let Some(library) = dylib_path() else {
        return;
    };
    let root =
        std::env::temp_dir().join(format!("lazy-arranger-test-synth-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let macos = root.join("Synth.vst3").join("Contents").join("MacOS");
    std::fs::create_dir_all(&macos).expect("bundle");
    std::fs::copy(&library, macos.join("Synth")).expect("copy");
    let bundle = root.join("Synth.vst3");
    let classes = plugin_host::scan_vst3(&bundle).expect("scan");
    let class = classes
        .iter()
        .find(|class| class.name.contains("Synth"))
        .expect("synth class");
    let mut instrument =
        plugin_host::Vst3Instrument::new(&bundle, &class.cid, 48_000.0, 512).expect("instrument");
    instrument.note_on_at(60, 100, 0);
    let mut left = vec![0.0_f32; 512];
    let mut right = vec![0.0_f32; 512];
    instrument
        .process_block(&mut left, &mut right)
        .expect("process");
    instrument.note_off_at(60, 0);
    let energy: f32 = left.iter().chain(&right).map(|sample| sample.abs()).sum();
    assert!(energy > 0.1, "synth rendered no audio");
    let _ = std::fs::remove_dir_all(&root);
}
