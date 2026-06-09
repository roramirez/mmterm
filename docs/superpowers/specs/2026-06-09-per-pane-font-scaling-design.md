# Per-Pane Font Scaling — Design

**Date:** 2026-06-09
**Status:** Approved (design); pending implementation plan

## Problem

`Ctrl +`, `Ctrl -`, and `Ctrl 0` change font size for the **entire active tab**:
every pane in the tab is resized together. The desired behavior is to scope these
actions to the **active pane only**, leaving sibling panes untouched.

## Root Cause

Font state lives on `TabState`:

- `TabState.logical_font_size: Logical` — density-independent font size
- `TabState.metrics: FontMetrics` — cell layout derived from `scale.px(logical_font_size)`

`input_ops.rs::change_font_size` mutates the active **tab's** font state and then calls
`sync_pane_sizes_tab`, which re-grids every pane in the tab using the single shared
`tab.metrics`.

## Goal

Move font ownership from the tab to the pane so that:

1. `Ctrl ±` / `Ctrl 0` affect only the active pane.
2. Sibling panes keep their own font size and grid dimensions.
3. Each pane re-derives its own metrics on a HiDPI scale-factor change.

## Non-Goals

- Persisting per-pane font size (it remains session-only — never written to config or
  the session file, matching today's behavior).
- Any change to zoom, layout, or split geometry.
- A separate "resize the whole tab" affordance — per-pane is the only mode.

## Design

### State move

Move the two font fields from `TabState` to `PaneEntry`:

```rust
// app_state.rs
pub struct PaneEntry {
    pub pane: Pane,
    pub pty: crate::pty::PtySession,
    pub rx: Receiver<Vec<u8>>,
    pub log_file: Option<std::fs::File>,
    pub logical_font_size: Logical,   // NEW
    pub metrics: FontMetrics,         // NEW
}

pub struct TabState {
    pub panes: HashMap<usize, PaneEntry>,
    pub layout: Layout,
    pub active: usize,
    // metrics: REMOVED
    // logical_font_size: REMOVED
    pub name: Option<String>,
    // ... rest unchanged
}
```

### Mutation path

**`input_ops.rs::change_font_size(delta)`** — read the active pane's logical size,
apply the delta, write new logical + metrics back to that pane, then re-grid **only
that pane**. The pane's pixel rect is unchanged; only its cols/rows change, so no other
pane needs touching and the layout tree is untouched.

```rust
pub(crate) fn change_font_size(&mut self, delta: f32) {
    let idx = self.state.active_tab;
    let active = self.state.tabs[idx].active;
    let Some(entry) = self.state.tabs[idx].panes.get(&active) else { return };
    let logical = entry.logical_font_size;
    let Some((new_logical, new_metrics)) =
        crate::scaling::apply_font_delta(logical, delta, self.scale, &mut self.renderer)
    else {
        return;
    };
    let pane_padding = self.pane_padding();
    let entry = self.state.tabs[idx].panes.get_mut(&active).unwrap();
    entry.logical_font_size = new_logical;
    entry.metrics = new_metrics;
    // re-grid this pane only, using its fixed rect and new metrics
    let [_, _, w, h] = entry.pane.rect;
    let pad2 = pane_padding * 2;
    let (cols, rows) = entry
        .metrics
        .grid_size_for(w.saturating_sub(pad2), h.saturating_sub(pad2));
    if entry.pane.parser.grid.cols != cols || entry.pane.parser.grid.rows != rows {
        entry.pane.resize(cols, rows, entry.pane.rect);
        let _ = entry.pty.resize(cols as u16, rows as u16);
    }
}
```

(Exact borrow structure to be finalized in implementation; the constraint is: never
borrow `self.renderer` and a `tabs[idx].panes` entry mutably in the same expression.)

**`app_state.rs::do_reset_font_size`** — read `current` from the active pane instead of
the tab:

```rust
let current = self
    .tabs
    .get(self.active_tab)
    .and_then(|t| t.panes.get(&t.active))
    .map(|e| e.logical_font_size.0)
    .unwrap_or(default_logical);
```

The reset still returns `AppEffect::ChangeFontSize(default_logical - current)`, which
flows through the per-pane `change_font_size` above.

### Pane sizing (`pane_ops.rs`)

- **`sync_pane_sizes_tab`** — use each `entry.metrics` rather than the shared
  `tab.metrics`. Because the borrow of `tab.metrics` and `tab.panes` no longer overlap,
  the per-entry metric read is local to each loop iteration.
- **`spawn_pane_into`** — every newly spawned pane starts at the **config default**
  (`config.font.size`). Compute `logical = Logical(config.font.size)` and
  `metrics = renderer.make_metrics(scale.px(logical))`, use those metrics to size the
  initial grid, and store both on the new `PaneEntry`.
- **`new_tab`** — remove the `metrics` / `logical_font_size` fields from the `TabState`
  literal (the initial pane gets them via `spawn_pane_into`).

### Scale changes (`scaling.rs::recompute_metrics_for_scale`)

On a winit `ScaleFactorChanged`, iterate **every pane in every tab** and re-derive each
pane's metrics from its own `logical_font_size`:

```rust
pub fn recompute_metrics_for_scale(tabs: &mut [TabState], scale: Scale, r: &mut Renderer) {
    for tab in tabs.iter_mut() {
        for entry in tab.panes.values_mut() {
            entry.metrics = r.make_metrics(scale.px(entry.logical_font_size));
        }
    }
}
```

`recompute_metrics_for_scale` is followed by `sync_all_pane_sizes` (unchanged call site),
which now re-grids each pane with its own refreshed metrics.

### Rendering (`views.rs`, `render_ops.rs`, `renderer/text.rs`)

- `PaneView` gains `pub metrics: FontMetrics`.
- `collect_pane_views` fills `metrics: entry.metrics.clone()` for both the zoomed and
  tiled branches.
- `Renderer::draw_pane` uses `pane.metrics` instead of the shared `m: &FontMetrics`
  parameter.
- The top-level `metrics: &FontMetrics` parameter is removed from `Renderer::draw` and
  from the call site in `render_ops.rs` (line ~132–173). Chrome (tab bar, status bar)
  already computes its own font px internally and does not read this parameter, so its
  rendering is unaffected.

### Session restore (`restore.rs`)

Remove the `metrics` / `logical_font_size` fields from the `TabState` literal. Restored
panes are spawned through `spawn_pane_into`, so they come up at config default — which is
correct, since per-pane font size is not persisted.

### Mouse (`mouse_ops.rs::pixel_to_cell`)

Use `&entry.metrics` (the pane being clicked) instead of `tab.metrics`. This is strictly
more correct now that sibling panes can have different cell dimensions.

## Files Touched

| File | Change |
|---|---|
| `src/app_state.rs` | Move fields PaneEntry←TabState; fix `do_reset_font_size`; update test builders |
| `src/input_ops.rs` | `change_font_size` re-grids active pane only |
| `src/pane_ops.rs` | `spawn_pane_into` seeds per-pane font; `sync_pane_sizes_tab` per-entry metrics; `new_tab` literal |
| `src/scaling.rs` | `recompute_metrics_for_scale` iterates panes |
| `src/views.rs` | `PaneView.metrics` filled from `entry.metrics` |
| `src/render_ops.rs` | drop shared-metrics arg to `Renderer::draw` |
| `src/renderer/text.rs` | `PaneView.metrics` field; `draw_pane` uses it; `draw` signature |
| `src/restore.rs` | drop per-tab font fields from `TabState` literal |
| `src/mouse_ops.rs` | `pixel_to_cell` uses `entry.metrics` |

## Testing

- **Updated** — existing font-size tests in `app_state_test.rs` that assert tab-level
  mutation: re-target to assert the active pane changed and that a sibling pane's
  `logical_font_size` / grid dims are unchanged.
- **New** — split inheritance: bump the active pane's font, split, assert the new pane is
  at `config.font.size` (not the source pane's size).
- **New** — scale change: two panes at different `logical_font_size`, apply
  `recompute_metrics_for_scale`, assert each pane's metrics are independently re-derived
  (proportional to its own logical size, not a shared value).
- **Existing** — keybinding tests for Ctrl ±/0 (`keybindings_test.rs`) remain valid (they
  assert action dispatch, not scope) and must still pass.
- Run `cargo test`, `cargo fmt --check`, `cargo clippy --locked -- -D warnings`.

## Changelog

Under `## [Unreleased]` → `### Changed`:

```markdown
- scope font size adjustment (ctrl +/-/0) to the active pane instead of the whole tab
```

## Invariants Respected

- `tab.metrics` no longer exists; all layout uses the relevant `entry.metrics`
  (per the "never use `renderer.font_px` for layout" rule, now per-pane).
- Never borrow `self.renderer` and a pane entry mutably in the same expression.
- Font clamp `6.0..=72.0` preserved (unchanged `font::apply_delta`).
- Session-only state (per-pane font size) is not persisted.

## Git Note

Per the user's hard rule (NEVER `git commit`), this design doc is written but **not**
committed. Staging/commit is a manual developer action.
