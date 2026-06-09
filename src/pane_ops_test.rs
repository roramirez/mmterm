use std::collections::HashMap;

use crate::app_state::{AppState, TabState};
use crate::dpi::Logical;
use crate::renderer::FontMetrics;
use crate::ui::layout::{Layout, SplitDir};

use super::App;

fn metrics(font_px: f32, cw: u32, ch: u32) -> FontMetrics {
    FontMetrics {
        font_px,
        cell_width: cw,
        cell_height: ch,
        baseline: ch.saturating_sub(3),
    }
}

fn empty_tab() -> TabState {
    TabState {
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
    }
}

#[test]
fn sync_uses_per_pane_metrics() {
    // Two panes side-by-side; pane 1 has half the cell size of pane 2,
    // so it must end up with more cols/rows after sizing.
    let mut tab = empty_tab();
    tab.layout.split(1, 2, SplitDir::H);
    tab.panes.insert(
        1,
        AppState::test_pane_entry(Logical(16.0), metrics(16.0, 8, 16)),
    );
    tab.panes.insert(
        2,
        AppState::test_pane_entry(Logical(32.0), metrics(32.0, 16, 32)),
    );

    App::sync_pane_sizes_tab(&mut tab, 22, 22, 0);

    let c1 = tab.panes[&1].pane.parser.grid.cols;
    let c2 = tab.panes[&2].pane.parser.grid.cols;
    let r1 = tab.panes[&1].pane.parser.grid.rows;
    let r2 = tab.panes[&2].pane.parser.grid.rows;
    assert!(c1 > c2, "smaller cells must yield more cols: {c1} vs {c2}");
    assert!(r1 > r2, "smaller cells must yield more rows: {r1} vs {r2}");
}
