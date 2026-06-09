use super::{apply_font_delta, reset_font_size};
use crate::dpi::{Logical, Physical, Scale};
use crate::renderer::Renderer;

fn r() -> Renderer {
    Renderer::new("JetBrainsMono", 16.0)
}

#[test]
fn delta_2x() {
    let (l, m) = apply_font_delta(Logical(16.0), 2.0, Scale::new(2.0), &mut r()).unwrap();
    assert_eq!(l, Logical(18.0));
    assert_eq!(m.font_px, 36.0);
}
#[test]
fn delta_1x() {
    let (l, m) = apply_font_delta(Logical(16.0), 1.0, Scale::new(1.0), &mut r()).unwrap();
    assert_eq!(l, Logical(17.0));
    assert_eq!(m.font_px, 17.0);
}
#[test]
fn delta_max_none() {
    assert!(apply_font_delta(Logical(72.0), 1.0, Scale::new(1.0), &mut r()).is_none());
}
#[test]
fn delta_min_none() {
    assert!(apply_font_delta(Logical(6.0), -1.0, Scale::new(1.0), &mut r()).is_none());
}
#[test]
fn reset_2x() {
    let (l, m) = reset_font_size(Logical(16.0), Scale::new(2.0), &mut r());
    assert_eq!(l, Logical(16.0));
    assert_eq!(m.font_px, 32.0);
}
#[test]
fn compose() {
    let mut rr = r();
    let a = rr.make_metrics(Scale::new(2.0).px(Logical(16.0)));
    let b = rr.make_metrics(Physical(32.0));
    assert_eq!(a.cell_width, b.cell_width);
    assert_eq!(a.cell_height, b.cell_height);
}

#[test]
fn recompute_two_panes_2x() {
    use super::recompute_metrics_for_scale;
    use crate::app_state::{AppState, TabState};
    use crate::dpi::Logical;
    use crate::ui::layout::Layout;
    use std::collections::HashMap;

    fn one_pane_tab(logical: Logical, m: crate::renderer::FontMetrics) -> TabState {
        let mut tab = TabState {
            panes: HashMap::new(),
            layout: Layout::new(1, 800, 600),
            active: 1,
            name: None,
            zoomed: false,
            has_activity: false,
            bell_flash_start: None,
            bell_flash_until: None,
            bell_cooldown_until: None,
            passthrough: false,
        };
        tab.panes.insert(1, AppState::test_pane_entry(logical, m));
        tab
    }

    let mut rr = r();
    let m16 = rr.make_metrics(Scale::new(1.0).px(Logical(16.0)));
    let m12 = rr.make_metrics(Scale::new(1.0).px(Logical(12.0)));
    let mut tabs = vec![
        one_pane_tab(Logical(16.0), m16),
        one_pane_tab(Logical(12.0), m12),
    ];
    recompute_metrics_for_scale(&mut tabs, Scale::new(2.0), &mut rr);
    assert_eq!(tabs[0].panes[&1].metrics.font_px, 32.0);
    assert_eq!(tabs[1].panes[&1].metrics.font_px, 24.0);
    recompute_metrics_for_scale(&mut tabs, Scale::new(1.0), &mut rr);
    assert_eq!(tabs[0].panes[&1].metrics.font_px, 16.0);
    assert_eq!(tabs[1].panes[&1].metrics.font_px, 12.0);
}
