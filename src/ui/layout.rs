/// Manages how panes are arranged. Designed to support splits in the future.
pub struct Layout {
    pub width: u32,
    pub height: u32,
}

impl Layout {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Full-window rect for a single pane.
    pub fn full_rect(&self) -> [u32; 4] {
        [0, 0, self.width, self.height]
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}
