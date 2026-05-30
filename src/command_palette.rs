use crate::input::keybindings::Action;

type Entry = (&'static str, &'static str, &'static str, fn() -> Action);

/// (human label, internal code, keyboard shortcut, action factory)
static ENTRIES: &[Entry] = &[
    ("Split Horizontal", "SplitH", "Ctrl+W v", || Action::SplitH),
    ("Split Vertical", "SplitV", "Ctrl+W s", || Action::SplitV),
    ("Auto Split", "AutoSplit", "Ctrl+W a", || Action::AutoSplit),
    ("Close Pane", "ClosePane", "Ctrl+W q", || Action::ClosePane),
    ("Zoom Pane", "ZoomPane", "Ctrl+W z", || Action::ZoomPane),
    ("Focus Left", "FocusLeft", "Ctrl+W h", || Action::FocusLeft),
    ("Focus Right", "FocusRight", "Ctrl+W l", || {
        Action::FocusRight
    }),
    ("Focus Up", "FocusUp", "Ctrl+W k", || Action::FocusUp),
    ("Focus Down", "FocusDown", "Ctrl+W j", || Action::FocusDown),
    ("Focus Next Pane", "FocusNext", "Ctrl+W w", || {
        Action::FocusNext
    }),
    (
        "Resize Pane Right",
        "ResizePaneRight",
        "Ctrl+Shift+→",
        || Action::ResizePaneRight,
    ),
    (
        "Resize Pane Left",
        "ResizePaneLeft",
        "Ctrl+Shift+←",
        || Action::ResizePaneLeft,
    ),
    (
        "Resize Pane Down",
        "ResizePaneDown",
        "Ctrl+Shift+↓",
        || Action::ResizePaneDown,
    ),
    ("Resize Pane Up", "ResizePaneUp", "Ctrl+Shift+↑", || {
        Action::ResizePaneUp
    }),
    ("New Tab", "NewTab", "Ctrl+T", || Action::NewTab),
    ("Next Tab", "NextTab", "Ctrl+PgDn", || Action::NextTab),
    ("Previous Tab", "PrevTab", "Ctrl+PgUp", || Action::PrevTab),
    ("Close Tab", "CloseTab", "Ctrl+Shift+W", || Action::CloseTab),
    ("Rename Tab", "RenameTab", "Ctrl+Shift+R", || {
        Action::RenameTab
    }),
    ("Move Tab Left", "MoveTabLeft", "Ctrl+Shift+PgUp", || {
        Action::MoveTabLeft
    }),
    ("Move Tab Right", "MoveTabRight", "Ctrl+Shift+PgDn", || {
        Action::MoveTabRight
    }),
    ("Increase Font Size", "IncreaseFontSize", "Ctrl++", || {
        Action::IncreaseFontSize
    }),
    ("Decrease Font Size", "DecreaseFontSize", "Ctrl+-", || {
        Action::DecreaseFontSize
    }),
    ("Reset Font Size", "ResetFontSize", "Ctrl+0", || {
        Action::ResetFontSize
    }),
    ("Search", "SearchOpen", "/  (Normal mode)", || {
        Action::SearchOpen
    }),
    ("Scroll to Top", "ScrollToTop", "Ctrl+Shift+Home", || {
        Action::ScrollToTop
    }),
    (
        "Scroll to Bottom",
        "ScrollToBottom",
        "Ctrl+Shift+End",
        || Action::ScrollToBottom,
    ),
    (
        "Clear Scrollback",
        "ClearScrollback",
        "Ctrl+Shift+K",
        || Action::ClearScrollback,
    ),
    ("Toggle Log", "ToggleLog", "Ctrl+Shift+L", || {
        Action::ToggleLog
    }),
    (
        "Toggle Fullscreen",
        "ToggleFullscreen",
        "Ctrl+Enter",
        || Action::ToggleFullscreen,
    ),
    ("Open Config", "OpenConfig", "Ctrl+,", || Action::OpenConfig),
    ("Quit", "Quit", "Ctrl+Q", || Action::Quit),
];

/// Returns indices into ENTRIES where the query matches label or code (case-insensitive substring).
/// Empty query returns all entries.
pub fn filter(query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    ENTRIES
        .iter()
        .enumerate()
        .filter(|(_, (label, code, _, _))| {
            q.is_empty() || label.to_lowercase().contains(&q) || code.to_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

pub fn entry_label(idx: usize) -> &'static str {
    ENTRIES[idx].0
}

#[cfg(test)]
fn entry_code(idx: usize) -> &'static str {
    ENTRIES[idx].1
}

pub fn entry_shortcut(idx: usize) -> &'static str {
    ENTRIES[idx].2
}

pub fn entry_action(idx: usize) -> Action {
    (ENTRIES[idx].3)()
}

pub fn total() -> usize {
    ENTRIES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all() {
        assert_eq!(filter("").len(), total());
    }

    #[test]
    fn filter_matches_human_label() {
        let res = filter("split");
        assert!(res.len() >= 3);
        for i in &res {
            let label = entry_label(*i).to_lowercase();
            let code = entry_code(*i).to_lowercase();
            assert!(label.contains("split") || code.contains("split"));
        }
    }

    #[test]
    fn filter_matches_internal_code() {
        let res = filter("splitv");
        assert!(!res.is_empty());
        assert!(res.iter().any(|&i| entry_code(i) == "SplitV"));
    }

    #[test]
    fn filter_no_match_returns_empty() {
        assert!(filter("xyznotexist").is_empty());
    }

    #[test]
    fn entry_action_returns_correct_variant() {
        let idx = filter("New Tab")[0];
        assert!(matches!(entry_action(idx), Action::NewTab));
    }

    #[test]
    fn filter_tab_matches_tab_actions() {
        let codes: Vec<&str> = filter("tab").iter().map(|&i| entry_code(i)).collect();
        assert!(codes.contains(&"NewTab"));
        assert!(codes.contains(&"NextTab"));
        assert!(codes.contains(&"CloseTab"));
    }

    #[test]
    fn all_entries_have_shortcut() {
        for i in 0..total() {
            assert!(!entry_shortcut(i).is_empty(), "entry {} has no shortcut", i);
        }
    }

    #[test]
    fn labels_and_codes_are_non_empty() {
        for i in 0..total() {
            assert!(!entry_label(i).is_empty());
            assert!(!entry_code(i).is_empty());
        }
    }

    #[test]
    fn all_entry_actions_can_be_constructed() {
        for i in 0..total() {
            let _ = entry_action(i);
        }
    }
}
