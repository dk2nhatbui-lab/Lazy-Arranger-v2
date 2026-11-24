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
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}
