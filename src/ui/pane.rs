use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::terminal::grid::Grid;

pub struct Pane {
    pub grid: Arc<RwLock<Grid>>,
    pub rect: [u32; 4],
    pub scroll_offset: usize,
}

impl Pane {
    pub fn new(grid: Arc<RwLock<Grid>>, rect: [u32; 4]) -> Self {
        Self {
            grid,
            rect,
            scroll_offset: 0,
        }
    }

    /// Acquire the grid read lock, degrading instead of panicking if it was
    /// poisoned (a parser thread panicked while holding the write lock). Main-
    /// thread callers use this to drop a frame / skip an action rather than
    /// crash the whole app in cascade. Returns `None` on poison.
    pub fn grid_read(&self) -> Option<RwLockReadGuard<'_, Grid>> {
        match self.grid.read() {
            Ok(g) => Some(g),
            Err(e) => {
                log::warn!("grid read lock poisoned, skipping: {e}");
                None
            }
        }
    }

    /// Acquire the grid write lock, degrading instead of panicking on poison.
    /// See [`Pane::grid_read`].
    pub fn grid_write(&self) -> Option<RwLockWriteGuard<'_, Grid>> {
        match self.grid.write() {
            Ok(g) => Some(g),
            Err(e) => {
                log::warn!("grid write lock poisoned, skipping: {e}");
                None
            }
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize, rect: [u32; 4]) {
        // Compute delta and new_sb atomically within the write lock so the parser
        // thread cannot add scrollback between the two reads.
        let delta_sb = self.grid_write().map(|mut g| {
            let delta = g.resize(cols, rows);
            (delta, g.scrollback_len())
        });
        self.rect = rect;
        if let Some((delta, new_sb)) = delta_sb
            && self.scroll_offset > 0
        {
            self.scroll_offset = (self.scroll_offset as isize + delta).max(0) as usize;
            self.scroll_offset = self.scroll_offset.min(new_sb);
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        let Some(max) = self.grid_read().map(|g| g.scrollback_len()) else {
            return;
        };
        self.scroll_offset = (self.scroll_offset + n).min(max);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    pub fn scroll_top(&mut self) {
        if let Some(len) = self.grid_read().map(|g| g.scrollback_len()) {
            self.scroll_offset = len;
        }
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

#[cfg(test)]
#[path = "pane_test.rs"]
mod tests;
