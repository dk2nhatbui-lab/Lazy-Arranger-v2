//! Verifies the MCP output-root guard in a separate test process.

use project::song::{ProjectFile, Song};

#[test]
fn exports_outside_the_allowed_root_are_refused() {
    let root = std::env::temp_dir().join("lazy-arranger-mcp-output-root");
    let outside = std::env::temp_dir().join("lazy-arranger-mcp-outside");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&root).expect("root");
    std::fs::create_dir_all(&outside).expect("outside");
    mcp::set_output_root(&root).expect("set root");
    let mut session = mcp::session(ProjectFile::new(Song::new("Guard", 120.0))).expect("session");
    let refused = mcp::call(
        &mut session,
        "save_project",
        &serde_json::json!({"path": outside.join("song.json").display().to_string()}),
    );
    assert!(refused.is_err());
    let allowed = mcp::call(
        &mut session,
        "save_project",
        &serde_json::json!({"path": root.join("song.json").display().to_string()}),
    );
    assert!(allowed.is_ok());
    assert!(
        mcp::call(
            &mut session,
            "style_from_clips",
            &serde_json::json!({"out": outside.join("style.sty").display().to_string()}),
        )
        .is_err()
    );
    assert!(
        mcp::call(
            &mut session,
            "style_from_audio",
            &serde_json::json!({
                "audio": outside.join("missing.wav").display().to_string(),
                "out": outside.join("audio.sty").display().to_string()
            }),
        )
        .is_err()
    );
    // Folder tools may write into the root itself and new folders below it,
    // but not into hidden folders or outside it.
    let mut stems = |directory: std::path::PathBuf| {
        mcp::call(
            &mut session,
            "export_stems",
            &serde_json::json!({"directory": directory.display().to_string(), "sample_rate": 8000}),
        )
    };
    assert!(stems(root.clone()).is_ok());
    assert!(stems(root.join("new/stems")).is_ok());
    assert!(stems(root.join(".hidden")).is_err());
    assert!(stems(outside.join("stems")).is_err());
    #[cfg(unix)]
    {
        let planted = root.join("linked");
        std::fs::create_dir_all(&planted).expect("folder");
        std::os::unix::fs::symlink(outside.join("stems.txt"), planted.join("stems.txt"))
            .expect("link");
        assert!(stems(planted).is_err());
        assert!(!outside.join("stems.txt").exists());
    }
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}
