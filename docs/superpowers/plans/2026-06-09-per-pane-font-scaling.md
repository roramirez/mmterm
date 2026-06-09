# Per-Pane Font Scaling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scope `Ctrl +` / `Ctrl -` / `Ctrl 0` font-size changes to the active pane instead of the whole tab.

**Architecture:** Move the font-state pair (`logical_font_size: Logical` + derived `metrics: FontMetrics`) from `TabState` to `PaneEntry`. Every pane owns its font size; the mutation path re-grids only the active pane (its pixel rect is unchanged, so reusing `sync_pane_sizes_tab` leaves siblings untouched). New panes always spawn at config default. On HiDPI scale change, each pane re-derives its own metrics. Rendering and mouse hit-testing read per-pane metrics via `PaneView.metrics` / `entry.metrics`.

**Tech Stack:** Rust, winit, fontdue, crossbeam-channel. Tests are window-free unit tests (`*_test.rs` siblings) that spawn `/bin/true` PTYs for real `PaneEntry` values.

**Design doc:** `docs/superpowers/specs/2026-06-09-per-pane-font-scaling-design.md`

**Git note:** Per the user's hard rule, do NOT run `git commit`. Where steps below say "Commit", instead `git add` the listed files and STOP for the developer to commit manually. Do not add `Co-Authored-By` trailers.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src/app_state.rs` | `PaneEntry` / `TabState` structs; `do_reset_font_size`; test builders | Move 2 fields; fix reset; add `test_pane_entry`; trim `add_empty_tab` / remove `test_tab` |
| `src/input_ops.rs` | `change_font_size` | Mutate active pane, reuse `sync_pane_sizes_tab` |
| `src/pane_ops.rs` | spawn / new_tab / sync sizing | Per-pane metrics seed + per-entry sizing; new test mod |
| `src/scaling.rs` | `recompute_metrics_for_scale` | Iterate panes |
| `src/restore.rs` | session restore `TabState` literal | Drop 2 fields + unused local |
| `src/views.rs` | `collect_pane_views` | Fill `PaneView.metrics` |
| `src/render_ops.rs` | `draw` call site | Drop shared `metrics` arg |
| `src/renderer/text.rs` | `PaneView`, `draw`, `draw_pane` | Add `metrics` field; per-pane forward |
| `src/mouse_ops.rs` | `pixel_to_cell` | Use `entry.metrics` |
| `src/drain_test.rs` | existing test builders | Add 2 fields / drop 2 fields |
| `src/scaling_test.rs` | recompute test | Rewrite to per-pane |
| `src/pane_ops_test.rs` | NEW | Per-pane sizing test |
| `CHANGELOG.md` | changelog | One `Changed` line |

---

## Task 1: Red — new per-pane sizing test + shared test builder

This task adds a failing test that drives the field move. It will NOT compile until Task 2 (the new fields don't exist yet) — that compile error is the "red".

**Files:**
- Modify: `src/app_state.rs` (add `#[cfg(test)] test_pane_entry`)
- Create: `src/pane_ops_test.rs`
- Modify: `src/pane_ops.rs` (link the test module)

- [ ] **Step 1: Add the shared test builder to `src/app_state.rs`**

Insert this `#[cfg(test)]` helper into the `impl AppState` test-helpers section (right after the existing `add_empty_tab`, around line 925). It builds a real `PaneEntry` with the two NEW fields, spawning a throwaway `/bin/true` PTY (same pattern as `drain_test.rs::make_pane_entry`):

```rust
    /// Build a real `PaneEntry` (throwaway `/bin/true` PTY) carrying the given
    /// per-pane font size + metrics. For window-free tests of per-pane sizing.
    #[cfg(test)]
    pub(crate) fn test_pane_entry(
        logical: Logical,
        metrics: crate::renderer::FontMetrics,
    ) -> PaneEntry {
        use crate::terminal::grid::{Color, GridColors};
        use crate::ui::Pane;
        let (_unused_tx, rx) = crossbeam_channel::unbounded::<Vec<u8>>();
        let (pty_tx, _pty_rx) = crossbeam_channel::unbounded::<Vec<u8>>();
        let pty = crate::pty::PtySession::spawn_with_shell(
            80,
            24,
            pty_tx,
            "/bin/true",
            None,
            Box::new(|| {}),
        )
        .expect("PTY spawn failed");
        let colors = GridColors {
            fg: Color::WHITE,
            bg: Color::BLACK,
            cursor: Color::WHITE,
            selection: Color::WHITE,
            palette: [Color::BLACK; 16],
        };
        let pane = Pane::new_with_colors(80, 24, [0, 22, 800, 556], colors, 1000);
        PaneEntry {
            pane,
            pty,
            rx,
            log_file: None,
            logical_font_size: logical,
            metrics,
        }
    }
```

> Note: this references `PaneEntry { logical_font_size, metrics }` fields that do not exist yet — intended. It compiles in Task 2.

- [ ] **Step 2: Link a test module in `src/pane_ops.rs`**

At the very bottom of `src/pane_ops.rs`, after the `open_log_file` function, add:

```rust
#[cfg(test)]
#[path = "pane_ops_test.rs"]
mod tests;
```

- [ ] **Step 3: Create `src/pane_ops_test.rs` with the failing test**

```rust
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
    tab.panes
        .insert(1, AppState::test_pane_entry(Logical(16.0), metrics(16.0, 8, 16)));
    tab.panes
        .insert(2, AppState::test_pane_entry(Logical(32.0), metrics(32.0, 16, 32)));

    App::sync_pane_sizes_tab(&mut tab, 22, 22, 0);

    let c1 = tab.panes[&1].pane.parser.grid.cols;
    let c2 = tab.panes[&2].pane.parser.grid.cols;
    let r1 = tab.panes[&1].pane.parser.grid.rows;
    let r2 = tab.panes[&2].pane.parser.grid.rows;
    assert!(c1 > c2, "smaller cells must yield more cols: {c1} vs {c2}");
    assert!(r1 > r2, "smaller cells must yield more rows: {r1} vs {r2}");
}
```

- [ ] **Step 4: Confirm the red (compile failure)**

Run: `cargo test sync_uses_per_pane_metrics 2>&1 | head -30`
Expected: compile error — `no field 'logical_font_size' on type PaneEntry` (and `metrics`). This is the intended red state; proceed to Task 2.

- [ ] **Step 5: Stage (no commit)**

```bash
git add src/app_state.rs src/pane_ops.rs src/pane_ops_test.rs
```
Then STOP — developer commits manually.

---

## Task 2: Green — move font state to `PaneEntry` across the codebase

Atomic refactor. The build is red until every edit in this task is done; verify with one `cargo test` at the end.

**Files (all edits below):** `src/app_state.rs`, `src/input_ops.rs`, `src/pane_ops.rs`, `src/scaling.rs`, `src/restore.rs`, `src/views.rs`, `src/render_ops.rs`, `src/renderer/text.rs`, `src/mouse_ops.rs`, `src/drain_test.rs`, `src/scaling_test.rs`.

- [ ] **Step 1: `src/app_state.rs` — move struct fields**

Add the two fields to `PaneEntry` (currently lines 19–24):

```rust
pub struct PaneEntry {
    pub pane: Pane,
    pub pty: crate::pty::PtySession,
    pub rx: Receiver<Vec<u8>>,
    pub log_file: Option<std::fs::File>,
    /// Per-pane density-independent font size. Physical px = scale.px(logical_font_size).
    /// Mutated by Ctrl±/reset; re-derived (not persisted) on ScaleFactorChanged.
    pub logical_font_size: Logical,
    /// Cell layout derived from `scale.px(logical_font_size)`.
    pub metrics: FontMetrics,
}
```

Remove the two fields from `TabState` (currently lines 30–34) — delete the `pub metrics: FontMetrics,` line, the doc comment block, and the `pub logical_font_size: Logical,` line so the struct becomes:

```rust
pub struct TabState {
    pub panes: HashMap<usize, PaneEntry>,
    pub layout: Layout,
    pub active: usize,
    pub name: Option<String>,
    pub zoomed: bool,
    pub has_activity: bool,
    pub bell_flash_start: Option<Instant>,
    pub bell_flash_until: Option<Instant>,
    pub bell_cooldown_until: Option<Instant>,
    pub passthrough: bool,
}
```

- [ ] **Step 2: `src/app_state.rs` — fix `do_reset_font_size` (lines 541–549)**

```rust
    fn do_reset_font_size(&self) -> Vec<AppEffect> {
        let default_logical = self.config.font.size;
        let current = self
            .tabs
            .get(self.active_tab)
            .and_then(|t| t.panes.get(&t.active))
            .map(|e| e.logical_font_size.0)
            .unwrap_or(default_logical);
        vec![AppEffect::ChangeFontSize(default_logical - current)]
    }
```

- [ ] **Step 3: `src/app_state.rs` — fix `add_empty_tab` (lines 899–925)**

Delete the `let metrics = ... };` block (lines 904–909) and remove the `metrics,` and `logical_font_size: Logical(16.0),` lines from the `TabState` literal. The panes-less tab needs neither.

- [ ] **Step 4: `src/app_state.rs` — remove `test_tab` (lines 927–949)**

Delete the entire `test_tab` helper (its only caller, `scaling_test.rs::recompute_two_tabs_2x`, is rewritten in Step 11). Leaving it would reference the removed `metrics`/`logical_font_size` tab fields and fail to compile.

- [ ] **Step 5: `src/input_ops.rs` — rewrite `change_font_size` (lines 208–222)**

```rust
    pub(crate) fn change_font_size(&mut self, delta: f32) {
        let idx = self.state.active_tab;
        let active = self.state.tabs[idx].active;
        let Some(logical) = self.state.tabs[idx]
            .panes
            .get(&active)
            .map(|e| e.logical_font_size)
        else {
            return;
        };
        let Some((new_logical, new_metrics)) =
            crate::scaling::apply_font_delta(logical, delta, self.scale, &mut self.renderer)
        else {
            return;
        };
        if let Some(entry) = self.state.tabs[idx].panes.get_mut(&active) {
            entry.logical_font_size = new_logical;
            entry.metrics = new_metrics;
        }
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let pane_padding = self.pane_padding();
        // Re-grids only the active pane: sibling metrics + rects are unchanged,
        // so their cols/rows don't change and they are left alone.
        Self::sync_pane_sizes_tab(&mut self.state.tabs[idx], tab_h, status_h, pane_padding);
    }
```

- [ ] **Step 6: `src/pane_ops.rs` — seed per-pane metrics in `spawn_pane_into` (lines 21–41)**

Replace the head of the function (id allocation through the `grid_size_for` call) so the new pane computes its own config-default metrics and stores them:

```rust
        let id = self.state.next_pane_id;
        self.state.next_pane_id += 1;
        let [_, _, w, h] = rect;
        let logical = crate::dpi::Logical(self.state.config.font.size);
        let metrics = self.renderer.make_metrics(self.scale.px(logical));
        let pad2 = self.scale.chrome(crate::ui::layout::PANE_PADDING) * 2;
        let (cols, rows) =
            metrics.grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
```

Then add the two fields to the `PaneEntry` literal (currently lines 72–80):

```rust
                self.state.tabs[tab_idx].panes.insert(
                    id,
                    PaneEntry {
                        pane,
                        pty,
                        rx,
                        log_file,
                        logical_font_size: logical,
                        metrics,
                    },
                );
```

- [ ] **Step 7: `src/pane_ops.rs` — fix `new_tab` (lines 104–128)**

Delete the now-unused `let logical = ...;` and `let metrics = ...;` locals (lines 104–105) and remove the `metrics,` and `logical_font_size: crate::dpi::Logical(self.state.config.font.size),` lines from the `TabState` literal (lines 119–120). The initial pane gets its font via `spawn_pane_into` (Step 6). Keep `tab_h`/`status_h` and the rest.

- [ ] **Step 8: `src/pane_ops.rs` — per-entry sizing in `sync_pane_sizes_tab` (lines 185–198)**

```rust
        let rects = tab.layout.rects_scaled(tab_h, status_h);
        for (id, rect) in rects {
            if let Some(entry) = tab.panes.get_mut(&id) {
                let [_, _, w, h] = rect;
                let pad2 = pane_padding * 2;
                let (cols, rows) = entry
                    .metrics
                    .grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
                if entry.pane.parser.grid.cols != cols || entry.pane.parser.grid.rows != rows {
                    entry.pane.resize(cols, rows, rect);
                    let _ = entry.pty.resize(cols as u16, rows as u16);
                }
            }
        }
```

- [ ] **Step 9: `src/scaling.rs` — per-pane recompute (lines 29–33)**

```rust
pub fn recompute_metrics_for_scale(tabs: &mut [TabState], scale: Scale, r: &mut Renderer) {
    for tab in tabs.iter_mut() {
        for entry in tab.panes.values_mut() {
            entry.metrics = r.make_metrics(scale.px(entry.logical_font_size));
        }
    }
}
```

(`FontMetrics` import stays — still used by `apply_font_delta`'s return type.)

- [ ] **Step 10: `src/restore.rs` — fix restore `TabState` literal (lines 55–75)**

Delete the `let metrics = self.renderer.make_metrics(...);` local (lines 55–58) and remove `metrics: metrics.clone(),` and `logical_font_size: crate::dpi::Logical(self.state.config.font.size),` from the `TabState` literal (lines 66–67). Restored panes spawn through `spawn_pane_into`, so they come up at config default.

- [ ] **Step 11: `src/views.rs` — fill `PaneView.metrics` (both branches)**

In `collect_pane_views`, add `metrics: entry.metrics.clone(),` to BOTH `PaneView { ... }` literals — the zoomed branch (around line 48) and the tiled branch (around line 75). Place it alongside the other per-pane fields, e.g. after `cursor_shape: entry.pane.parser.grid.cursor_shape,`.

- [ ] **Step 12: `src/renderer/text.rs` — add field, forward per-pane, drop shared param**

Add to `PaneView` (after `cursor_shape`, line 41):

```rust
    /// Per-pane cell metrics (font size is per-pane, not per-tab).
    pub metrics: FontMetrics,
```

In `draw` (lines 144–165), delete the `metrics: &FontMetrics,` parameter (line 154).

In `draw`'s pane loop (line 173), forward each pane's own metrics:

```rust
        for pane in panes {
            self.draw_pane(buf, buf_width, pane, mode, &pane.metrics, inactive_dim, theme);
        }
```

`draw_pane`'s `m: &FontMetrics` parameter and body are unchanged.

- [ ] **Step 13: `src/render_ops.rs` — drop the shared metrics arg**

Delete `let metrics = self.state.tabs[self.state.active_tab].metrics.clone();` (line 132). In the `self.renderer.draw(...)` call (lines 165–185), remove the `&metrics,` argument (line 174).

- [ ] **Step 14: `src/mouse_ops.rs` — per-pane metrics in `pixel_to_cell` (line 59)**

Change `let m = &tab.metrics;` to:

```rust
        let m = &entry.metrics;
```

(`entry` is already bound on the line above; `tab` stays bound for `tab.panes.get`.)

- [ ] **Step 15: `src/drain_test.rs` — update test builders**

In `make_tab` (lines 24–37) remove `metrics: dummy_metrics(),` and `logical_font_size: crate::dpi::Logical(16.0),`.

In `make_pane_entry`'s `PaneEntry` literal (lines 62–67) add the two fields:

```rust
    let entry = PaneEntry {
        pane,
        pty,
        rx: test_rx,
        log_file: None,
        logical_font_size: crate::dpi::Logical(16.0),
        metrics: dummy_metrics(),
    };
```

(`dummy_metrics` and `crate::dpi` are already in scope.)

- [ ] **Step 16: `src/scaling_test.rs` — rewrite `recompute_two_tabs_2x` to per-pane (lines 44–59)**

Replace the test (and add imports at top of file) so it builds tabs that each contain one pane at a distinct logical size and asserts each pane's metrics re-derive independently:

```rust
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
```

- [ ] **Step 17: Build + run the full suite (the green)**

Run: `cargo test 2>&1 | tail -30`
Expected: compiles; all tests pass, including `sync_uses_per_pane_metrics` (Task 1) and `recompute_two_panes_2x`.

If you hit a borrow-checker error in `change_font_size` (Step 5) about borrowing `self.renderer` and `self.state.tabs` together: the structure above already reads `logical` first (immutable, dropped), then borrows `self.renderer` for `apply_font_delta`, then re-borrows the pane mutably — keep those three in separate statements; do not collapse them.

- [ ] **Step 18: Format + lint**

Run: `cargo fmt && cargo clippy --locked -- -D warnings 2>&1 | tail -20`
Expected: no diff from fmt that breaks anything; clippy reports zero warnings. (Watch for an "unused import" on `FontMetrics` in `scaling.rs` — if clippy flags it, it means a usage was missed; re-check Step 9.)

- [ ] **Step 19: Stage (no commit)**

```bash
git add src/app_state.rs src/input_ops.rs src/pane_ops.rs src/scaling.rs \
  src/restore.rs src/views.rs src/render_ops.rs src/renderer/text.rs \
  src/mouse_ops.rs src/drain_test.rs src/scaling_test.rs
```
Then STOP — developer commits manually.

---

## Task 3: Reset reads the active pane (behavior assertion)

**Files:**
- Modify: `src/app_state_test.rs` (rewrite `dispatch_reset_font_size_emits_logical_delta_to_default`, lines 849–868)

- [ ] **Step 1: Rewrite the reset-delta test to use an active pane**

The old test set `s.tabs[0].logical_font_size` (a removed field). Replace it with a test that inserts a pane at logical 18.0 and asserts the reset delta is `-2.0` (default 16 − current 18), proving the reset reads the active pane:

```rust
#[test]
fn dispatch_reset_font_size_emits_logical_delta_to_default() {
    // Config default is 16.0; active pane is at logical 18.0 → delta should be -2.0.
    use crate::renderer::FontMetrics;
    let mut s = make_state_with_tabs(1);
    let m = FontMetrics {
        font_px: 18.0,
        cell_width: 9,
        cell_height: 18,
        baseline: 15,
    };
    s.tabs[0]
        .panes
        .insert(1, AppState::test_pane_entry(crate::dpi::Logical(18.0), m));
    s.tabs[0].active = 1;

    let effects = s.dispatch_action(Action::ResetFontSize);
    let delta = effects.iter().find_map(|e| {
        if let AppEffect::ChangeFontSize(d) = e {
            Some(*d)
        } else {
            None
        }
    });
    assert!(delta.is_some(), "expected ChangeFontSize effect");
    let d = delta.unwrap();
    assert!(
        (d - (-2.0_f32)).abs() < 1e-5,
        "expected delta -2.0 (default 16 - current 18), got {d}"
    );
}
```

(`AppState` is in scope via `use super::*;` at the top of `app_state_test.rs`. `dispatch_reset_font_size_returns_change_effect` at line 838 needs no change — with no pane it falls back to default and still yields `ChangeFontSize(0.0)`, matching `ChangeFontSize(_)`.)

- [ ] **Step 2: Run the reset tests**

Run: `cargo test reset_font 2>&1 | tail -20`
Expected: both `dispatch_reset_font_size_returns_change_effect` and `dispatch_reset_font_size_emits_logical_delta_to_default` PASS.

- [ ] **Step 3: Stage (no commit)**

```bash
git add src/app_state_test.rs
```
Then STOP — developer commits manually.

---

## Task 4: Changelog + final gates

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Add the changelog entry**

Under `## [Unreleased]`, in a `### Changed` section (create it if absent, in the standard order Added/Changed/Fixed), add:

```markdown
- scope font size adjustment (ctrl +/-/0) to the active pane instead of the whole tab
```

- [ ] **Step 2: Full verification**

Run each and confirm the expected result:

```bash
cargo fmt --check          # expected: no output (clean)
cargo clippy --locked -- -D warnings   # expected: zero warnings
cargo test                 # expected: all pass
```

- [ ] **Step 3: Quality gate (kimun)**

Run: `km score --trend origin/main --fail-if-worse`
Expected: score not worse than `origin/main` (`.kimun.toml` `fail_below` respected). If it regresses, address the flagged file before finishing.

- [ ] **Step 4: Manual smoke (developer)**

Build and run, then: split a pane (`Ctrl-W` then split), press `Ctrl +` several times in one pane, confirm ONLY that pane's font grows and the sibling is unchanged; `Ctrl 0` resets only the active pane; split again and confirm the new pane is at the default size.

```bash
cargo run --release
```

- [ ] **Step 5: Stage (no commit)**

```bash
git add CHANGELOG.md
```
Then STOP — developer commits manually.

---

## Self-Review Notes

- **Spec coverage:** state move (T2 S1), per-pane mutation + reuse of `sync_pane_sizes_tab` (T2 S5), reset reads active pane (T2 S2 + T3), spawn config-default (T2 S6), per-pane scale recompute (T2 S9 + T2 S16), rendering via `PaneView.metrics` (T2 S11–S13), mouse per-pane (T2 S14), restore drops fields (T2 S10), session-only/no-persist (unchanged — no save-path edits). All spec sections mapped.
- **Type consistency:** field names `logical_font_size: Logical` / `metrics: FontMetrics` are identical everywhere; `test_pane_entry(Logical, FontMetrics)` signature matches all call sites; `sync_pane_sizes_tab(&mut TabState, u32, u32, u32)` unchanged.
- **No placeholders:** every code step shows full code; commands have expected output.
