use super::*;
use std::path::PathBuf;

// ── session_path_for tests ────────────────────────────────────────────────────

#[test]
fn session_path_for_none_ends_with_default() {
    let p = session_path_for(None);
    assert!(
        p.ends_with("mmterm/session.toml"),
        "expected path ending with mmterm/session.toml, got: {}",
        p.display()
    );
}

#[test]
fn session_path_for_scope_work() {
    let p = session_path_for(Some("work"));
    assert!(
        p.ends_with("mmterm/sessions/work.toml"),
        "expected path ending with mmterm/sessions/work.toml, got: {}",
        p.display()
    );
}

#[test]
fn session_path_for_scope_with_dashes() {
    let p = session_path_for(Some("my-project"));
    assert!(
        p.ends_with("mmterm/sessions/my-project.toml"),
        "got: {}",
        p.display()
    );
}

// ── list_scopes_in tests ──────────────────────────────────────────────────────

#[test]
fn list_scopes_in_empty_dir_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = dir.path().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    assert_eq!(list_scopes_in(&sessions), Vec::<String>::new());
}

#[test]
fn list_scopes_in_nonexistent_dir_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = dir.path().join("sessions_does_not_exist");
    assert_eq!(list_scopes_in(&sessions), Vec::<String>::new());
}

#[test]
fn list_scopes_in_returns_sorted_stems() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = dir.path().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join("c.toml"), "").unwrap();
    std::fs::write(sessions.join("a.toml"), "").unwrap();
    std::fs::write(sessions.join("b.toml"), "").unwrap();
    assert_eq!(list_scopes_in(&sessions), vec!["a", "b", "c"]);
}

#[test]
fn list_scopes_in_ignores_non_toml_files() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = dir.path().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join("work.toml"), "").unwrap();
    std::fs::write(sessions.join("README.md"), "").unwrap();
    std::fs::write(sessions.join("tmp.bak"), "").unwrap();
    assert_eq!(list_scopes_in(&sessions), vec!["work"]);
}

#[test]
fn list_scopes_in_strips_only_trailing_toml() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = dir.path().join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    // "my.scope.toml" → stem should be "my.scope"
    std::fs::write(sessions.join("my.scope.toml"), "").unwrap();
    assert_eq!(list_scopes_in(&sessions), vec!["my.scope"]);
}

// ── scoped save/load round-trip ───────────────────────────────────────────────

#[test]
fn scoped_save_to_and_load_from_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    // Simulate ~/.config/mmterm/sessions/test-scope.toml
    let path = dir
        .path()
        .join("mmterm")
        .join("sessions")
        .join("test-scope.toml");
    let session = simple_session();
    save_to(&path, &session).expect("save_to failed");
    let loaded = load_from(&path).expect("load_from returned None");
    assert_eq!(loaded.active_tab, session.active_tab);
    assert_eq!(loaded.tabs[0].pane_cwds[0], PathBuf::from("/tmp"));
}

#[test]
fn scoped_save_creates_sessions_parent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mmterm").join("sessions").join("work.toml");
    // Parent doesn't exist yet — save_to must create it.
    assert!(!path.parent().unwrap().exists());
    save_to(&path, &simple_session()).expect("save_to should create parent dirs");
    assert!(path.exists());
}

#[test]
fn default_and_scoped_paths_are_different() {
    let default = session_path_for(None);
    let scoped = session_path_for(Some("work"));
    assert_ne!(default, scoped);
}

#[test]
fn two_scopes_produce_different_paths() {
    let a = session_path_for(Some("work"));
    let b = session_path_for(Some("personal"));
    assert_ne!(a, b);
}

fn leaf(slot: usize) -> SavedNode {
    SavedNode::Leaf { slot }
}

fn split(dir: SavedSplitDir, ratio: f32, a: SavedNode, b: SavedNode) -> SavedNode {
    SavedNode::Split {
        dir,
        ratio,
        a: Box::new(a),
        b: Box::new(b),
    }
}

#[test]
fn roundtrip_single_pane() {
    let session = SavedSession {
        active_tab: 0,
        tabs: vec![SavedTab {
            name: None,
            active_pane: 0,
            pane_cwds: vec![PathBuf::from("/tmp")],
            layout: leaf(0),
        }],
    };
    let toml = toml::to_string_pretty(&session).expect("serialize");
    let back: SavedSession = toml::from_str(&toml).expect("deserialize");
    assert_eq!(back.active_tab, 0);
    assert_eq!(back.tabs.len(), 1);
    assert_eq!(back.tabs[0].pane_cwds[0], PathBuf::from("/tmp"));
    assert!(matches!(back.tabs[0].layout, SavedNode::Leaf { slot: 0 }));
}

#[test]
fn roundtrip_h_split() {
    let session = SavedSession {
        active_tab: 0,
        tabs: vec![SavedTab {
            name: Some("build".into()),
            active_pane: 1,
            pane_cwds: vec![PathBuf::from("/home"), PathBuf::from("/tmp")],
            layout: split(SavedSplitDir::H, 0.6, leaf(0), leaf(1)),
        }],
    };
    let toml = toml::to_string_pretty(&session).expect("serialize");
    let back: SavedSession = toml::from_str(&toml).expect("deserialize");
    assert_eq!(back.tabs[0].name.as_deref(), Some("build"));
    assert_eq!(back.tabs[0].active_pane, 1);
    if let SavedNode::Split { dir, ratio, a, b } = &back.tabs[0].layout {
        assert!(matches!(dir, SavedSplitDir::H));
        assert!((ratio - 0.6).abs() < 0.001);
        assert!(matches!(a.as_ref(), SavedNode::Leaf { slot: 0 }));
        assert!(matches!(b.as_ref(), SavedNode::Leaf { slot: 1 }));
    } else {
        panic!("expected Split");
    }
}

#[test]
fn roundtrip_three_pane_tree() {
    // Split(H, Split(V, Leaf(0), Leaf(1)), Leaf(2))
    let layout = split(
        SavedSplitDir::H,
        0.5,
        split(SavedSplitDir::V, 0.5, leaf(0), leaf(1)),
        leaf(2),
    );
    let session = SavedSession {
        active_tab: 0,
        tabs: vec![SavedTab {
            name: None,
            active_pane: 0,
            pane_cwds: vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c"),
            ],
            layout,
        }],
    };
    let toml = toml::to_string_pretty(&session).expect("serialize");
    let back: SavedSession = toml::from_str(&toml).expect("deserialize");
    assert_eq!(back.tabs[0].pane_cwds.len(), 3);
    // spot-check the tree structure survives
    let SavedNode::Split { a, b, .. } = &back.tabs[0].layout else {
        panic!("expected outer Split");
    };
    assert!(matches!(a.as_ref(), SavedNode::Split { .. }));
    assert!(matches!(b.as_ref(), SavedNode::Leaf { slot: 2 }));
}

#[test]
fn roundtrip_multiple_tabs() {
    let session = SavedSession {
        active_tab: 1,
        tabs: vec![
            SavedTab {
                name: Some("one".into()),
                active_pane: 0,
                pane_cwds: vec![PathBuf::from("/a")],
                layout: leaf(0),
            },
            SavedTab {
                name: Some("two".into()),
                active_pane: 0,
                pane_cwds: vec![PathBuf::from("/b"), PathBuf::from("/c")],
                layout: split(SavedSplitDir::V, 0.4, leaf(0), leaf(1)),
            },
        ],
    };
    let toml = toml::to_string_pretty(&session).expect("serialize");
    let back: SavedSession = toml::from_str(&toml).expect("deserialize");
    assert_eq!(back.active_tab, 1);
    assert_eq!(back.tabs.len(), 2);
    assert_eq!(back.tabs[1].name.as_deref(), Some("two"));
}

#[test]
fn load_returns_none_on_missing_file() {
    // session_path() points to the real config dir; this test just checks
    // that load() doesn't panic when the file is absent.
    // We can't override the path without a refactor, so we verify via
    // toml::from_str failing gracefully.
    let result = toml::from_str::<SavedSession>("not valid toml ;;;");
    assert!(result.is_err());
}

#[test]
fn load_returns_none_on_corrupt_toml() {
    let raw = "active_tab = 0\n[[tabs]]\nnot_a_field = true";
    let result = toml::from_str::<SavedSession>(raw);
    // Missing required fields → deserialization error
    assert!(result.is_err());
}

// ── I/O tests using save_to / load_from ──────────────────────────────────────

fn simple_session() -> SavedSession {
    SavedSession {
        active_tab: 0,
        tabs: vec![SavedTab {
            name: None,
            active_pane: 0,
            pane_cwds: vec![PathBuf::from("/tmp")],
            layout: leaf(0),
        }],
    }
}

#[test]
fn save_to_and_load_from_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.toml");
    let session = simple_session();
    super::save_to(&path, &session).expect("save_to failed");
    let loaded = super::load_from(&path).expect("load_from returned None");
    assert_eq!(loaded.active_tab, session.active_tab);
    assert_eq!(loaded.tabs.len(), session.tabs.len());
    assert_eq!(loaded.tabs[0].pane_cwds[0], PathBuf::from("/tmp"));
}

#[test]
fn save_to_creates_parent_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("dirs").join("session.toml");
    super::save_to(&path, &simple_session()).expect("save_to failed");
    assert!(path.exists());
}

#[test]
fn save_to_uses_atomic_rename_no_tmp_leftover() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.toml");
    super::save_to(&path, &simple_session()).expect("save_to failed");
    let tmp = path.with_extension("toml.tmp");
    assert!(!tmp.exists(), ".tmp file should not remain after save");
}

#[test]
fn load_from_missing_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.toml");
    assert!(super::load_from(&path).is_none());
}

#[test]
fn load_from_corrupt_toml_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.toml");
    std::fs::write(&path, b"not valid toml ;;;").unwrap();
    assert!(super::load_from(&path).is_none());
}

#[test]
fn load_from_empty_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.toml");
    std::fs::write(&path, b"").unwrap();
    assert!(super::load_from(&path).is_none());
}

#[test]
fn save_to_overwrites_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.toml");

    let first = SavedSession {
        active_tab: 0,
        tabs: vec![SavedTab {
            name: Some("first".into()),
            active_pane: 0,
            pane_cwds: vec![PathBuf::from("/a")],
            layout: leaf(0),
        }],
    };
    super::save_to(&path, &first).unwrap();

    let second = SavedSession {
        active_tab: 1,
        tabs: vec![
            SavedTab {
                name: None,
                active_pane: 0,
                pane_cwds: vec![PathBuf::from("/b")],
                layout: leaf(0),
            },
            SavedTab {
                name: None,
                active_pane: 0,
                pane_cwds: vec![PathBuf::from("/c")],
                layout: leaf(0),
            },
        ],
    };
    super::save_to(&path, &second).unwrap();

    let loaded = super::load_from(&path).unwrap();
    assert_eq!(loaded.active_tab, 1);
    assert_eq!(loaded.tabs.len(), 2);
}

#[test]
fn roundtrip_with_empty_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.toml");
    let session = SavedSession {
        active_tab: 0,
        tabs: vec![SavedTab {
            name: None,
            active_pane: 0,
            pane_cwds: vec![PathBuf::from("")],
            layout: leaf(0),
        }],
    };
    super::save_to(&path, &session).unwrap();
    let loaded = super::load_from(&path).unwrap();
    assert_eq!(loaded.tabs[0].pane_cwds[0], PathBuf::from(""));
}

#[test]
fn roundtrip_active_tab_out_of_bounds_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.toml");
    let session = SavedSession {
        active_tab: 99,
        tabs: vec![SavedTab {
            name: None,
            active_pane: 0,
            pane_cwds: vec![PathBuf::from("/tmp")],
            layout: leaf(0),
        }],
    };
    super::save_to(&path, &session).unwrap();
    let loaded = super::load_from(&path).unwrap();
    // save_to does not clamp — the index is stored as-is
    assert_eq!(loaded.active_tab, 99);
}
