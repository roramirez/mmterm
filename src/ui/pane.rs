use std::sync::{Arc, RwLock};

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

    pub fn resize(&mut self, cols: usize, rows: usize, rect: [u32; 4]) {
        // Compute delta and new_sb atomically within the write lock so the parser
        // thread cannot add scrollback between the two reads.
        let (delta, new_sb) = {
            let mut g = self.grid.write().unwrap();
            let delta = g.resize(cols, rows);
            let new_sb = g.scrollback_len();
            (delta, new_sb)
        };
        self.rect = rect;
        if self.scroll_offset > 0 {
            self.scroll_offset = (self.scroll_offset as isize + delta).max(0) as usize;
            self.scroll_offset = self.scroll_offset.min(new_sb);
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        let max = self.grid.read().unwrap().scrollback_len();
        self.scroll_offset = (self.scroll_offset + n).min(max);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    pub fn scroll_top(&mut self) {
        self.scroll_offset = self.grid.read().unwrap().scrollback_len();
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

#[cfg(test)]
#[path = "pane_test.rs"]
mod tests;
