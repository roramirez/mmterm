pub const STATUS_BAR_H: u32 = 22;
pub const TAB_BAR_H: u32 = 22;
pub const PANE_PADDING: u32 = 4;
// intentionally 1 physical px at all scales; scale-aware strokes deferred (spec §9)
const SEP: u32 = 1;
pub const NUDGE_STEP: f32 = 0.05;
const RATIO_MIN: f32 = 0.1;
const RATIO_MAX: f32 = 0.9;

/// Split `full` pixels at `ratio`, reserving 1 px for the separator.
/// Returns `(a_size, b_size)` both clamped to at least 1 px.
fn split_dimension(full: u32, ratio: f32) -> (u32, u32) {
    let a = ((full as f32 * ratio) as u32).clamp(1, full.saturating_sub(SEP + 1));
    (a, full.saturating_sub(a + SEP))
}

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

fn sep_hit(
    perp_start: u32,
    perp_size: u32,
    sep_along: u32,
    point_along: u32,
    point_perp: u32,
    margin: u32,
) -> bool {
    point_perp >= perp_start
        && point_perp < perp_start + perp_size
        && sep_along.abs_diff(point_along) <= margin
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

    fn apply_leaf_ids(&mut self, ids: &[usize], cursor: &mut usize) {
        match self {
            Node::Leaf(id) => {
                *id = ids[*cursor];
                *cursor += 1;
            }
            Node::Split { a, b, .. } => {
                a.apply_leaf_ids(ids, cursor);
                b.apply_leaf_ids(ids, cursor);
            }
        }
    }

    fn compute_rects(&self, x: u32, y: u32, w: u32, h: u32, out: &mut Vec<(usize, [u32; 4])>) {
        match self {
            Node::Leaf(id) => out.push((*id, [x, y, w, h])),
            Node::Split { dir, ratio, a, b } => match dir {
                SplitDir::H => {
                    let (wa, wb) = split_dimension(w, *ratio);
                    a.compute_rects(x, y, wa, h, out);
                    b.compute_rects(x + wa + SEP, y, wb, h, out);
                }
                SplitDir::V => {
                    let (ha, hb) = split_dimension(h, *ratio);
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
                    let (wa, wb) = split_dimension(w, *ratio);
                    out.push([x + wa, y, SEP, h]);
                    a.separators(x, y, wa, h, out);
                    b.separators(x + wa + SEP, y, wb, h, out);
                }
                SplitDir::V => {
                    let (ha, hb) = split_dimension(h, *ratio);
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
                let (wa, wb) = split_dimension(w, *ratio);
                if sep_hit(y, h, x + wa, px, py, margin) {
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
                let (ha, hb) = split_dimension(h, *ratio);
                if sep_hit(x, w, y + ha, py, px, margin) {
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
        if a.nudge(target, split_h, delta) || b.nudge(target, split_h, delta) {
            return true;
        }
        if (a.contains_leaf(target) || b.contains_leaf(target))
            && matches!(dir, SplitDir::H) == split_h
        {
            *ratio = (*ratio + delta).clamp(RATIO_MIN, RATIO_MAX);
            return true;
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
            Node::Split { a, b, .. } => {
                match a.remove_leaf(target) {
                    RemoveResult::NotFound => {}
                    r => return apply_child_remove(a, b, r),
                }
                let r = b.remove_leaf(target);
                apply_child_remove(b, a, r)
            }
        }
    }
}

fn apply_child_remove(
    child: &mut Box<Node>,
    sibling: &mut Box<Node>,
    r: RemoveResult,
) -> RemoveResult {
    match r {
        RemoveResult::RemoveMe => {
            RemoveResult::Replace(std::mem::replace(sibling, Box::new(Node::Leaf(0))))
        }
        RemoveResult::Replace(node) => {
            *child = node;
            RemoveResult::Done
        }
        other => other,
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

    /// Usable height between the bars, given PHYSICAL chrome heights.
    pub fn usable_h_for(&self, tab_h: u32, status_h: u32) -> u32 {
        self.height.saturating_sub(tab_h + status_h)
    }

    pub fn rects(&self) -> Vec<(usize, [u32; 4])> {
        self.rects_scaled(TAB_BAR_H, STATUS_BAR_H)
    }

    /// Pane rects given PHYSICAL chrome heights (panes start at y = tab_h).
    pub fn rects_scaled(&self, tab_h: u32, status_h: u32) -> Vec<(usize, [u32; 4])> {
        let mut out = Vec::new();
        self.root.compute_rects(
            0,
            tab_h,
            self.width,
            self.usable_h_for(tab_h, status_h),
            &mut out,
        );
        out
    }

    pub fn separators(&self) -> Vec<[u32; 4]> {
        self.separators_scaled(TAB_BAR_H, STATUS_BAR_H)
    }

    /// Separators given PHYSICAL chrome heights.
    pub fn separators_scaled(&self, tab_h: u32, status_h: u32) -> Vec<[u32; 4]> {
        let mut out = Vec::new();
        self.root.separators(
            0,
            tab_h,
            self.width,
            self.usable_h_for(tab_h, status_h),
            &mut out,
        );
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
        self.separator_at_pixel_scaled(px, py, margin, TAB_BAR_H, STATUS_BAR_H)
    }

    /// Hit-test a separator given PHYSICAL chrome heights.
    pub fn separator_at_pixel_scaled(
        &self,
        px: u32,
        py: u32,
        margin: u32,
        tab_h: u32,
        status_h: u32,
    ) -> Option<SeparatorHandle> {
        let mut counter = 0usize;
        self.root.find_sep_at_pixel(
            px,
            py,
            0,
            tab_h,
            self.width,
            self.usable_h_for(tab_h, status_h),
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
        let new_ratio = raw.clamp(RATIO_MIN, RATIO_MAX);
        let mut counter = 0usize;
        self.root
            .set_ratio_by_idx(handle.idx, new_ratio, &mut counter);
    }

    /// Grow (`delta > 0`) or shrink (`delta < 0`) the active pane along
    /// the horizontal (`split_h = true`) or vertical axis.
    pub fn nudge_pane(&mut self, pane_id: usize, split_h: bool, delta: f32) {
        self.root.nudge(pane_id, split_h, delta);
    }

    /// Rotate pane IDs within the layout tree in DFS order.
    /// `forward = true`: last pane moves to first slot (Ctrl-W r).
    /// `forward = false`: first pane moves to last slot (Ctrl-W R).
    /// No-op when there is only one pane.
    pub fn rotate_leaves(&mut self, forward: bool) {
        let mut ids = self.root.leaves();
        if ids.len() < 2 {
            return;
        }
        if forward {
            ids.rotate_right(1);
        } else {
            ids.rotate_left(1);
        }
        self.root.apply_leaf_ids(&ids, &mut 0);
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
            best = best_dir_candidate(*id, rect, from_cx, from_cy, dx, dy, best);
        }
        best.map(|(id, _)| id)
    }

    /// Serialize the layout tree for session persistence.
    ///
    /// Returns `(node, id_order)` where `id_order` lists pane IDs in DFS
    /// leaf order; `node` uses slot indices (position in `id_order`) as leaf
    /// values so the caller can substitute fresh IDs on restore.
    pub fn to_saved_node(&self) -> (crate::session::SavedNode, Vec<usize>) {
        let mut id_order = Vec::new();
        let node = node_to_saved(&self.root, &mut id_order);
        (node, id_order)
    }

    /// Reconstruct a `Layout` from a saved node tree.
    ///
    /// `slot_to_id[slot]` must contain the new pane ID for each leaf slot in
    /// the saved tree. `w` and `h` are the current window pixel dimensions.
    pub fn from_saved_node(
        node: &crate::session::SavedNode,
        slot_to_id: &[usize],
        w: u32,
        h: u32,
    ) -> Self {
        Self {
            root: saved_to_node(node, slot_to_id),
            width: w,
            height: h,
        }
    }
}

fn in_target_direction(ddx: i32, ddy: i32, dx: i32, dy: i32) -> bool {
    (dx == 0 || ddx.signum() == dx) && (dy == 0 || ddy.signum() == dy)
}

fn best_dir_candidate(
    id: usize,
    rect: &[u32; 4],
    from_cx: i32,
    from_cy: i32,
    dx: i32,
    dy: i32,
    current: Option<(usize, i32)>,
) -> Option<(usize, i32)> {
    let cx = (rect[0] + rect[2] / 2) as i32;
    let cy = (rect[1] + rect[3] / 2) as i32;
    let ddx = cx - from_cx;
    let ddy = cy - from_cy;
    if !in_target_direction(ddx, ddy, dx, dy) {
        return current;
    }
    let dist = ddx.abs() + ddy.abs();
    if current.is_none_or(|(_, bd)| dist < bd) {
        Some((id, dist))
    } else {
        current
    }
}

fn node_to_saved(node: &Node, id_order: &mut Vec<usize>) -> crate::session::SavedNode {
    match node {
        Node::Leaf(id) => {
            let slot = id_order.len();
            id_order.push(*id);
            crate::session::SavedNode::Leaf { slot }
        }
        Node::Split { dir, ratio, a, b } => crate::session::SavedNode::Split {
            dir: match dir {
                SplitDir::H => crate::session::SavedSplitDir::H,
                SplitDir::V => crate::session::SavedSplitDir::V,
            },
            ratio: *ratio,
            a: Box::new(node_to_saved(a, id_order)),
            b: Box::new(node_to_saved(b, id_order)),
        },
    }
}

fn saved_to_node(node: &crate::session::SavedNode, slot_to_id: &[usize]) -> Node {
    match node {
        crate::session::SavedNode::Leaf { slot } => {
            Node::Leaf(slot_to_id.get(*slot).copied().unwrap_or(0))
        }
        crate::session::SavedNode::Split { dir, ratio, a, b } => Node::Split {
            dir: match dir {
                crate::session::SavedSplitDir::H => SplitDir::H,
                crate::session::SavedSplitDir::V => SplitDir::V,
            },
            ratio: *ratio,
            a: Box::new(saved_to_node(a, slot_to_id)),
            b: Box::new(saved_to_node(b, slot_to_id)),
        },
    }
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod tests;
