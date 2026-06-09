//! Logical-vs-physical pixel boundary for HiDPI scaling.
//!
//! Makes the logical-vs-physical pixel distinction type-safe at the **boundary**
//! (config size, font-metrics input, chrome scaling). This is a **deliberately
//! moderate change**: a fuller best-practice design would thread `Logical`/`Physical`
//! (or winit `dpi::LogicalUnit`/`PhysicalUnit`) through `FontMetrics` and *every*
//! pixel computation in the renderer/layout, eliminating all bare-`f32` pixel math.
//! That is a much larger refactor and is intentionally deferred (spec §9, "full
//! type-safe dpi refactor" TBD). Until then: cross the boundary via `Scale::px` /
//! `Scale::chrome`, and never multiply by scale with a bare `as u32` cast (it
//! truncates the fraction — round instead, via `Scale::chrome`).

/// A density-independent (logical) pixel value: what the user configures / sees as
/// a visual "size". `config.font.size` is logical; the clamp 6..=72 is on the logical value.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Logical(pub f32);

/// A device (physical) pixel value: what is rasterized into the softbuffer surface.
/// Never construct from a raw constant — go through `Scale::px`/`Scale::chrome`.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Physical(pub f32);

/// The current monitor's scale factor, floored at 1.0. The **single** Logical→Physical
/// conversion point. Monitor-local + dynamic: always from `window.scale_factor()`.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Scale(f32);

impl Scale {
    /// From winit's f64 scale_factor, floored at 1.0 (never shrink below logical size).
    pub fn new(raw: f64) -> Scale {
        Scale((raw as f32).max(1.0))
    }

    /// The raw scale value.
    // used in tests only; `px`/`chrome` are the production conversion points
    #[allow(dead_code)]
    pub fn get(self) -> f32 {
        self.0
    }

    /// Logical font/pixel size -> physical pixels (single conversion point).
    pub fn px(self, l: Logical) -> Physical {
        Physical(l.0 * self.0)
    }

    /// Logical chrome constant -> whole physical pixels. ROUND, never truncate:
    /// `22 * 1.25 = 27.5 -> 28` (a bare `as u32` would give 27 — a 1px error at
    /// fractional scales). All logical->integer chrome conversions must use this.
    pub fn chrome(self, logical: u32) -> u32 {
        ((logical as f32) * self.0).round() as u32
    }
}

#[cfg(test)]
#[path = "dpi_test.rs"]
mod tests;
