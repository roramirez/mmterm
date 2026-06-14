use super::*;
use chrono::TimeZone;

fn fixed_now() -> chrono::DateTime<chrono::Local> {
    chrono::Local
        .with_ymd_and_hms(2026, 1, 15, 12, 30, 0)
        .unwrap()
}

#[test]
fn empty_template_returns_none() {
    assert!(resolve("", None, &fixed_now()).is_none());
}

#[test]
fn literal_text_rendered_verbatim() {
    assert_eq!(
        resolve("hello", None, &fixed_now()),
        Some("hello".to_string())
    );
}

#[test]
fn pwd_token_with_cwd_present() {
    assert_eq!(
        resolve("%pwd", Some("/home/user"), &fixed_now()),
        Some("/home/user".to_string())
    );
}

#[test]
fn pwd_token_without_cwd_returns_none() {
    assert!(resolve("%pwd", None, &fixed_now()).is_none());
}

#[test]
fn date_token_formats_date() {
    assert_eq!(
        resolve("%date{%Y-%m-%d}", None, &fixed_now()),
        Some("2026-01-15".to_string())
    );
}

#[test]
fn date_token_formats_time() {
    assert_eq!(
        resolve("%date{%H:%M}", None, &fixed_now()),
        Some("12:30".to_string())
    );
}

#[test]
fn template_preserves_literal_spaces_between_tokens() {
    assert_eq!(
        resolve("%pwd  |  %date{%H:%M}", Some("/src"), &fixed_now()),
        Some("/src  |  12:30".to_string())
    );
}

#[test]
fn pwd_token_removed_strips_trailing_space() {
    assert_eq!(
        resolve("%pwd %date{%H:%M}", None, &fixed_now()),
        Some("12:30".to_string())
    );
}

#[test]
fn pwd_and_literal_combined() {
    assert_eq!(
        resolve("%pwd  branch", Some("/src"), &fixed_now()),
        Some("/src  branch".to_string())
    );
}

#[test]
fn only_pwd_no_cwd_returns_none() {
    assert!(resolve("%pwd", None, &fixed_now()).is_none());
}

// ── shorten_home ─────────────────────────────────────────────────────────────

#[test]
fn shorten_home_replaces_prefix() {
    assert_eq!(shorten_home("/home/user/src", "/home/user"), "~/src");
}

#[test]
fn shorten_home_exact_match() {
    assert_eq!(shorten_home("/home/user", "/home/user"), "~");
}

#[test]
fn shorten_home_no_match_returns_unchanged() {
    assert_eq!(shorten_home("/tmp/foo", "/home/user"), "/tmp/foo");
}

#[test]
fn shorten_home_empty_home_returns_unchanged() {
    assert_eq!(shorten_home("/home/user/src", ""), "/home/user/src");
}

// ── pane_title_for_display ───────────────────────────────────────────────────

#[test]
fn pane_title_shown_when_pwd_not_in_right() {
    assert_eq!(
        pane_title_for_display(Some("vim"), false, Some("/src")),
        Some("vim")
    );
}

#[test]
fn pane_title_shown_when_title_differs_from_cwd() {
    assert_eq!(
        pane_title_for_display(Some("vim"), true, Some("/src")),
        Some("vim")
    );
}

#[test]
fn pane_title_suppressed_when_matches_cwd_and_pwd_in_right() {
    assert_eq!(
        pane_title_for_display(Some("/src"), true, Some("/src")),
        None
    );
}

#[test]
fn pane_title_shown_when_no_cwd() {
    assert_eq!(pane_title_for_display(Some("vim"), true, None), Some("vim"));
}

#[test]
fn pane_title_none_input_returns_none() {
    assert_eq!(pane_title_for_display(None, true, Some("/src")), None);
}

// ── apply_pwd_token ───────────────────────────────────────────────────────────

#[test]
fn apply_pwd_token_some_appends_cwd() {
    let mut s = String::from("prefix ");
    apply_pwd_token(&mut s, Some("/home/user"));
    assert_eq!(s, "prefix /home/user");
}

#[test]
fn apply_pwd_token_none_removes_trailing_space() {
    let mut s = String::from("prefix ");
    apply_pwd_token(&mut s, None);
    assert_eq!(s, "prefix");
}

#[test]
fn apply_pwd_token_none_no_trailing_space_unchanged() {
    let mut s = String::from("prefix");
    apply_pwd_token(&mut s, None);
    assert_eq!(s, "prefix");
}

// ── apply_date_token ─────────────────────────────────────────────────────────

#[test]
fn apply_date_token_known_format_appends_to_result() {
    use chrono::TimeZone;
    let now = chrono::Local
        .with_ymd_and_hms(2024, 1, 15, 12, 0, 0)
        .unwrap();
    let mut s = String::new();
    let rest = apply_date_token(&mut s, "%Y}", &now);
    assert_eq!(s, "2024");
    assert_eq!(rest, "");
}

#[test]
fn apply_date_token_no_close_brace_emits_literal() {
    use chrono::Local;
    let now = Local::now();
    let mut s = String::new();
    let rest = apply_date_token(&mut s, "unclosed", &now);
    assert_eq!(s, "%date{");
    assert_eq!(rest, "unclosed");
}
