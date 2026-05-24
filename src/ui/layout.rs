pub const STATUS_BAR_H: u32 = 22;
pub const TAB_BAR_H: u32 = 22;
pub const PANE_PADDING: u32 = 4;
const SEP: u32 = 1;
pub const NUDGE_STEP: f32 = 0.05;

#[derive(Clone, Copy, Debug)]
pub enum SplitDir {
    /// Side by side: left | right
    H,
    /// Stacked: top / bottom
    V,
}

/// Identifies a specific separator in the layout tree, carrying enough
/// information to update its ratio without a second tree traversal.
#[derive(Clone, Copy, Debug)]
pub struct SeparatorHandle {
    /// DFS preorder index of the owning Split node (same order as `separators()`).
    idx: usize,
    pub dir: SplitDir,
    /// Origin along the split axis: x for H splits, y for V splits.
    region_pos: u32,
    /// Extent along the split axis: w for H splits, h for V splits.
    region_size: u32,
}

#[derive(Clone, Debug)]
enum Node {
    Leaf(usize),
    Split {
        dir: SplitDir,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}

impl Node {
    fn leaves(&self) -> Vec<usize> {
        match self {
            Node::Leaf(id) => vec![*id],
            Node::Split { a, b, .. } => {
                let mut v = a.leaves();
                v.extend(b.leaves());
                v
            }
        }
    }

    fn compute_rects(&self, x: u32, y: u32, w: u32, h: u32, out: &mut Vec<(usize, [u32; 4])>) {
        match self {
            Node::Leaf(id) => out.push((*id, [x, y, w, h])),
            Node::Split { dir, ratio, a, b } => match dir {
                SplitDir::H => {
                    let wa = ((w as f32 * ratio) as u32).clamp(1, w.saturating_sub(SEP + 1));
                    let wb = w.saturating_sub(wa + SEP);
                    a.compute_rects(x, y, wa, h, out);
                    b.compute_rects(x + wa + SEP, y, wb, h, out);
                }
                SplitDir::V => {
                    let ha = ((h as f32 * ratio) as u32).clamp(1, h.saturating_sub(SEP + 1));
                    let hb = h.saturating_sub(ha + SEP);
                    a.compute_rects(x, y, w, ha, out);
                    b.compute_rects(x, y + ha + SEP, w, hb, out);
                }
            },
        }
    }

    fn separators(&self, x: u32, y: u32, w: u32, h: u32, out: &mut Vec<[u32; 4]>) {
        if let Node::Split { dir, ratio, a, b } = self {
            match dir {
                SplitDir::H => {
                    let wa = ((w as f32 * ratio) as u32).clamp(1, w.saturating_sub(SEP + 1));
                    let wb = w.saturating_sub(wa + SEP);
                    out.push([x + wa, y, SEP, h]);
                    a.separators(x, y, wa, h, out);
                    b.separators(x + wa + SEP, y, wb, h, out);
                }
                SplitDir::V => {
                    let ha = ((h as f32 * ratio) as u32).clamp(1, h.saturating_sub(SEP + 1));
                    let hb = h.saturating_sub(ha + SEP);
                    out.push([x, y + ha, w, SEP]);
                    a.separators(x, y, w, ha, out);
                    b.separators(x, y + ha + SEP, w, hb, out);
                }
            }
        }
    }

    fn contains_leaf(&self, target: usize) -> bool {
        match self {
            Node::Leaf(id) => *id == target,
            Node::Split { a, b, .. } => a.contains_leaf(target) || b.contains_leaf(target),
        }
    }

    /// Find the separator within `margin` pixels of `(px, py)`.
    /// DFS preorder: increments `counter` at each Split node.
    #[allow(clippy::too_many_arguments)]
    fn find_sep_at_pixel(
        &self,
        px: u32,
        py: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        margin: u32,
        counter: &mut usize,
    ) -> Option<SeparatorHandle> {
        let Node::Split { dir, ratio, a, b } = self else {
            return None;
        };
        let idx = *counter;
        *counter += 1;
        match dir {
            SplitDir::H => {
                let wa = ((w as f32 * ratio) as u32).clamp(1, w.saturating_sub(SEP + 1));
                let wb = w.saturating_sub(wa + SEP);
                let sep_x = x + wa;
                if py >= y && py < y + h && sep_x.abs_diff(px) <= margin {
                    return Some(SeparatorHandle {
                        idx,
                        dir: SplitDir::H,
                        region_pos: x,
                        region_size: w,
                    });
                }
                a.find_sep_at_pixel(px, py, x, y, wa, h, margin, counter)
                    .or_else(|| {
                        b.find_sep_at_pixel(px, py, x + wa + SEP, y, wb, h, margin, counter)
                    })
            }
            SplitDir::V => {
                let ha = ((h as f32 * ratio) as u32).clamp(1, h.saturating_sub(SEP + 1));
                let hb = h.saturating_sub(ha + SEP);
                let sep_y = y + ha;
                if px >= x && px < x + w && sep_y.abs_diff(py) <= margin {
                    return Some(SeparatorHandle {
                        idx,
                        dir: SplitDir::V,
                        region_pos: y,
                        region_size: h,
                    });
                }
                a.find_sep_at_pixel(px, py, x, y, w, ha, margin, counter)
                    .or_else(|| {
                        b.find_sep_at_pixel(px, py, x, y + ha + SEP, w, hb, margin, counter)
                    })
            }
        }
    }

    /// Set the ratio of the Split node at DFS preorder index `target`.
    fn set_ratio_by_idx(&mut self, target: usize, new_ratio: f32, counter: &mut usize) -> bool {
        let Node::Split { ratio, a, b, .. } = self else {
            return false;
        };
        if *counter == target {
            *ratio = new_ratio;
            return true;
        }
        *counter += 1;
        a.set_ratio_by_idx(target, new_ratio, counter)
            || b.set_ratio_by_idx(target, new_ratio, counter)
    }

    /// Grow (`delta > 0`) or shrink (`delta < 0`) the active pane along
    /// the horizontal (`split_h = true`) or vertical axis.
    /// Finds the innermost matching split on the path to `target` and adjusts
    /// its ratio so the side containing `target` gains/loses space.
    /// Move the separator toward the active pane's edge in the given direction.
    /// `delta > 0` moves it right/down; `delta < 0` moves it left/up.
    /// The ratio always changes by `+delta`, so the separator moves in a
    /// consistent direction regardless of which side the active pane is on.
    fn nudge(&mut self, target: usize, split_h: bool, delta: f32) -> bool {
        let Node::Split { dir, ratio, a, b } = self else {
            return false;
        };
        let dir_matches = matches!(dir, SplitDir::H) == split_h;
        if a.contains_leaf(target) {
            if a.nudge(target, split_h, delta) {
                return true;
            }
            if dir_matches {
                *ratio = (*ratio + delta).clamp(0.1, 0.9);
                return true;
            }
        } else if b.contains_leaf(target) {
            if b.nudge(target, split_h, delta) {
                return true;
            }
            if dir_matches {
                *ratio = (*ratio + delta).clamp(0.1, 0.9);
                return true;
            }
        }
        false
    }

    fn split_leaf(&mut self, target: usize, new_id: usize, dir: SplitDir) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                let old = Node::Leaf(*id);
                *self = Node::Split {
                    dir,
                    ratio: 0.5,
                    a: Box::new(old),
                    b: Box::new(Node::Leaf(new_id)),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => {
                a.split_leaf(target, new_id, dir) || b.split_leaf(target, new_id, dir)
            }
        }
    }

    fn remove_leaf(&mut self, target: usize) -> RemoveResult {
        match self {
            Node::Leaf(id) if *id == target => RemoveResult::RemoveMe,
            Node::Leaf(_) => RemoveResult::NotFound,
            Node::Split { a, b, .. } => match a.remove_leaf(target) {
                RemoveResult::RemoveMe => {
                    RemoveResult::Replace(std::mem::replace(b, Box::new(Node::Leaf(0))))
                }
                RemoveResult::Replace(node) => {
                    *a = node;
                    RemoveResult::Done
                }
                RemoveResult::Done => RemoveResult::Done,
                RemoveResult::NotFound => match b.remove_leaf(target) {
                    RemoveResult::RemoveMe => {
                        RemoveResult::Replace(std::mem::replace(a, Box::new(Node::Leaf(0))))
                    }
                    RemoveResult::Replace(node) => {
                        *b = node;
                        RemoveResult::Done
                    }
                    r => r,
                },
            },
        }
    }
}

enum RemoveResult {
    NotFound,
    RemoveMe,
    Replace(Box<Node>),
    Done,
}

pub struct Layout {
    root: Node,
    pub width: u32,
    pub height: u32,
}

impl Layout {
    pub fn new(initial_pane: usize, width: u32, height: u32) -> Self {
        Self {
            root: Node::Leaf(initial_pane),
            width,
            height,
        }
    }

    fn usable_h(&self) -> u32 {
        self.height.saturating_sub(STATUS_BAR_H + TAB_BAR_H)
    }

    pub fn rects(&self) -> Vec<(usize, [u32; 4])> {
        let mut out = Vec::new();
        self.root
            .compute_rects(0, TAB_BAR_H, self.width, self.usable_h(), &mut out);
        out
    }

    pub fn separators(&self) -> Vec<[u32; 4]> {
        let mut out = Vec::new();
        self.root
            .separators(0, TAB_BAR_H, self.width, self.usable_h(), &mut out);
        out
    }

    pub fn leaves(&self) -> Vec<usize> {
        self.root.leaves()
    }

    pub fn split(&mut self, active: usize, new_id: usize, dir: SplitDir) {
        self.root.split_leaf(active, new_id, dir);
    }

    /// Remove pane. Returns the new focus ID (sibling of the removed pane).
    pub fn remove(&mut self, target: usize) -> Option<usize> {
        let leaves_before = self.root.leaves();
        let sibling = leaves_before.iter().find(|&&id| id != target).copied();

        match self.root.remove_leaf(target) {
            RemoveResult::Replace(node) => self.root = *node,
            RemoveResult::RemoveMe => {} // last pane — caller should quit
            _ => {}
        }
        sibling
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    /// Returns a handle to the separator within `margin` pixels of `(px, py)`,
    /// or `None` if no separator is that close.
    pub fn separator_at_pixel(&self, px: u32, py: u32, margin: u32) -> Option<SeparatorHandle> {
        let mut counter = 0usize;
        self.root.find_sep_at_pixel(
            px,
            py,
            0,
            TAB_BAR_H,
            self.width,
            self.usable_h(),
            margin,
            &mut counter,
        )
    }

    /// Move the separator identified by `handle` to absolute pixel position
    /// `new_pos` (x for H splits, y for V splits). Ratio is clamped to 0.1–0.9.
    pub fn move_separator(&mut self, handle: SeparatorHandle, new_pos: u32) {
        if handle.region_size == 0 {
            return;
        }
        let raw = new_pos.saturating_sub(handle.region_pos) as f32 / handle.region_size as f32;
        let new_ratio = raw.clamp(0.1, 0.9);
        let mut counter = 0usize;
        self.root
            .set_ratio_by_idx(handle.idx, new_ratio, &mut counter);
    }

    /// Grow (`delta > 0`) or shrink (`delta < 0`) the active pane along
    /// the horizontal (`split_h = true`) or vertical axis.
    pub fn nudge_pane(&mut self, pane_id: usize, split_h: bool, delta: f32) {
        self.root.nudge(pane_id, split_h, delta);
    }

    /// Find the pane spatially closest to `from` in direction `dx, dy`.
    pub fn focus_dir(&self, from: usize, dx: i32, dy: i32) -> Option<usize> {
        let rects = self.rects();
        let from_rect = rects.iter().find(|(id, _)| *id == from)?.1;
        let from_cx = (from_rect[0] + from_rect[2] / 2) as i32;
        let from_cy = (from_rect[1] + from_rect[3] / 2) as i32;

        let mut best: Option<(usize, i32)> = None;
        for (id, rect) in &rects {
            if *id == from {
                continue;
            }
            let cx = (rect[0] + rect[2] / 2) as i32;
            let cy = (rect[1] + rect[3] / 2) as i32;
            let ddx = cx - from_cx;
            let ddy = cy - from_cy;
            // Must be in the requested direction
            if dx != 0 && ddx.signum() != dx {
                continue;
            }
            if dy != 0 && ddy.signum() != dy {
                continue;
            }
            // Prefer movement mostly along the requested axis
            let dist = ddx.abs() + ddy.abs();
            if let Some((_, bd)) = best {
                if dist < bd {
                    best = Some((*id, dist));
                }
            } else {
                best = Some((*id, dist));
            }
        }
        best.map(|(id, _)| id)
    }
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod tests;
