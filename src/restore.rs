use std::collections::HashMap;

use crate::ui::Layout;
use crate::{TabState, session};

use super::App;

impl App {
    pub(super) fn build_saved_session(&self) -> session::SavedSession {
        let home = dirs_next::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
        let tabs = self
            .state
            .tabs
            .iter()
            .map(|tab| {
                let (node, id_order) = tab.layout.to_saved_node();
                let active_slot = id_order
                    .iter()
                    .position(|&id| id == tab.active)
                    .unwrap_or(0);
                let pane_cwds = id_order
                    .iter()
                    .map(|id| {
                        tab.panes
                            .get(id)
                            .and_then(|e| e.pty.cwd())
                            .unwrap_or_else(|| home.clone())
                    })
                    .collect();
                session::SavedTab {
                    name: tab.name.clone(),
                    active_pane: active_slot,
                    pane_cwds,
                    layout: node,
                }
            })
            .collect();
        session::SavedSession {
            active_tab: self.state.active_tab,
            tabs,
        }
    }

    pub(super) fn restore_session(
        &mut self,
        saved: session::SavedSession,
        win_w: u32,
        win_h: u32,
    ) -> bool {
        if saved.tabs.is_empty() {
            return false;
        }
        let tab_h = self.tab_h();
        let status_h = self.status_h();
        let metrics = self.renderer.make_metrics(
            self.scale
                .px(crate::dpi::Logical(self.state.config.font.size)),
        );
        let home = dirs_next::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
        for tab_sess in &saved.tabs {
            let tab_idx = self.state.tabs.len();
            self.state.tabs.push(TabState {
                panes: HashMap::new(),
                layout: Layout::new(0, win_w, win_h),
                active: 0,
                metrics: metrics.clone(),
                logical_font_size: crate::dpi::Logical(self.state.config.font.size),
                name: tab_sess.name.clone(),
                zoomed: false,
                has_activity: false,
                bell_flash_start: None,
                bell_flash_until: None,
                bell_cooldown_until: None,
                passthrough: false,
            });
            let rect = [0, tab_h, win_w, win_h.saturating_sub(tab_h + status_h)];
            let slot_to_id: Vec<usize> = tab_sess
                .pane_cwds
                .iter()
                .map(|cwd| {
                    let cwd_opt = if cwd.as_os_str().is_empty() {
                        Some(home.clone())
                    } else if cwd.exists() {
                        Some(cwd.clone())
                    } else {
                        Some(home.clone())
                    };
                    self.spawn_pane_into(tab_idx, rect, cwd_opt)
                })
                .collect();
            if slot_to_id.is_empty() {
                let id = self.spawn_pane_into(tab_idx, rect, None);
                self.state.tabs[tab_idx].layout = Layout::new(id, win_w, win_h);
                self.state.tabs[tab_idx].active = id;
            } else {
                let layout = Layout::from_saved_node(&tab_sess.layout, &slot_to_id, win_w, win_h);
                let active_id = slot_to_id
                    .get(tab_sess.active_pane)
                    .copied()
                    .unwrap_or(slot_to_id[0]);
                self.state.tabs[tab_idx].layout = layout;
                self.state.tabs[tab_idx].active = active_id;
            }
            let pane_padding = self.pane_padding();
            Self::sync_pane_sizes_tab(&mut self.state.tabs[tab_idx], tab_h, status_h, pane_padding);
        }
        self.state.active_tab = saved
            .active_tab
            .min(self.state.tabs.len().saturating_sub(1));
        true
    }
}
