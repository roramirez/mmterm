//! Window-free font-scaling helpers. Every function takes `scale: Scale` as an
//! explicit parameter (never reads a window) — the testability seam (spec §5.2.1).
use crate::app_state::TabState;
use crate::dpi::{Logical, Scale};
use crate::font;
use crate::renderer::{FontMetrics, Renderer};

/// Apply a font-size delta on the LOGICAL axis (clamp 6..=72), derive physical metrics.
/// `None` when the delta is a no-op at a clamp boundary.
pub fn apply_font_delta(
    logical: Logical,
    delta: f32,
    scale: Scale,
    r: &mut Renderer,
) -> Option<(Logical, FontMetrics)> {
    let new = Logical(font::apply_delta(logical.0, delta)?);
    let metrics = r.make_metrics(scale.px(new));
    Some((new, metrics))
}

/// Reset to the config-default logical size; derive physical metrics.
// public seam kept for symmetry; production reset routes through apply_font_delta
#[allow(dead_code)]
pub fn reset_font_size(config: Logical, scale: Scale, r: &mut Renderer) -> (Logical, FontMetrics) {
    (config, r.make_metrics(scale.px(config)))
}

/// Re-derive every tab's metrics for a new scale (winit ScaleFactorChanged). Window-free.
pub fn recompute_metrics_for_scale(tabs: &mut [TabState], scale: Scale, r: &mut Renderer) {
    for tab in tabs.iter_mut() {
        tab.metrics = r.make_metrics(scale.px(tab.logical_font_size));
    }
}

#[cfg(test)]
#[path = "scaling_test.rs"]
mod tests;
