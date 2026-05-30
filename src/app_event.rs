use arboard::Clipboard;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::CursorIcon;

use crate::config::Config;
use crate::input::{InputMode, handle_ctrl_w, handle_key, handle_key_passthrough};
use crate::theme::{load_theme, themes_dir};
use crate::tui_config::ConfigAction;
use crate::ui::{SplitDir, layout::SeparatorHandle};
use crate::{command_palette, search};

use super::App;

fn palette_move_selection(selected: usize, filtered_len: usize, up: bool) -> usize {
    if filtered_len == 0 {
        0
    } else if up {
        selected.saturating_sub(1)
    } else {
        (selected + 1).min(filtered_len - 1)
    }
}

fn cursor_icon_for_hover(hover_sep: Option<&SeparatorHandle>, has_url: bool) -> CursorIcon {
    if let Some(h) = hover_sep {
        match h.dir {
            SplitDir::H => CursorIcon::ColResize,
            SplitDir::V => CursorIcon::RowResize,
        }
    } else if has_url {
        CursorIcon::Pointer
    } else {
        CursorIcon::Text
    }
}

impl App {
    pub(super) fn handle_search_key(&mut self, event: &winit::event::KeyEvent) {
        let query = if let InputMode::Search { query } = &self.state.mode {
            query.clone()
        } else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.state.mode = InputMode::Normal;
                self.state.search_matches.clear();
            }
            Key::Named(NamedKey::Enter) if !self.state.search_matches.is_empty() => {
                let next = (self.state.search_current + 1) % self.state.search_matches.len();
                self.scroll_to_match(next);
            }
            Key::Named(NamedKey::Backspace) => {
                let mut q = query;
                q.pop();
                self.state.mode = InputMode::Search { query: q };
                self.update_search_matches();
            }
            Key::Character(s) if s == "\x03" || s == "c" => {
                self.copy_current_match();
            }
            Key::Character(s) => {
                let mut q = query;
                q.push_str(s);
                self.state.mode = InputMode::Search { query: q };
                self.update_search_matches();
            }
            _ => {}
        }
    }

    pub(super) fn handle_command_palette_key(
        &mut self,
        event: &winit::event::KeyEvent,
        event_loop: &ActiveEventLoop,
    ) {
        let (query, selected) =
            if let InputMode::CommandPalette { query, selected } = &self.state.mode {
                (query.clone(), *selected)
            } else {
                return;
            };

        let filtered = command_palette::filter(&query);
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.state.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.state.mode = InputMode::CommandPalette {
                    query,
                    selected: palette_move_selection(selected, filtered.len(), true),
                };
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.state.mode = InputMode::CommandPalette {
                    query,
                    selected: palette_move_selection(selected, filtered.len(), false),
                };
            }
            Key::Named(NamedKey::Enter) => {
                self.state.mode = InputMode::Insert;
                if let Some(&entry_idx) = filtered.get(selected) {
                    let action = command_palette::entry_action(entry_idx);
                    self.execute_action(action, event_loop);
                }
            }
            Key::Named(NamedKey::Backspace) => {
                let mut q = query;
                q.pop();
                self.state.mode = InputMode::CommandPalette {
                    selected: 0,
                    query: q,
                };
            }
            Key::Character(s) => {
                let mut q = query;
                q.push_str(s);
                self.state.mode = InputMode::CommandPalette {
                    selected: 0,
                    query: q,
                };
            }
            _ => {}
        }
    }

    pub(super) fn copy_current_match(&mut self) {
        let Some(&(abs_row, col, len)) = self.state.search_matches.get(self.state.search_current)
        else {
            return;
        };
        let tab_idx = self.state.active_tab;
        let active = self.state.tabs[tab_idx].active;
        let Some(entry) = self.state.tabs[tab_idx].panes.get(&active) else {
            return;
        };
        let grid = &entry.pane.parser.grid;
        let text = search::extract_match_text(grid, abs_row, col, len);
        if !text.is_empty() {
            let cb = self
                .state
                .clipboard
                .get_or_insert_with(|| Clipboard::new().expect("clipboard unavailable"));
            match cb.set_text(text) {
                Ok(()) => log::info!("Copied search match to clipboard"),
                Err(e) => log::warn!("Clipboard write failed: {e}"),
            }
        }
    }

    pub(super) fn update_search_matches(&mut self) {
        let query = match &self.state.mode {
            InputMode::Search { query } => query.clone(),
            _ => return,
        };

        self.state.search_matches.clear();
        self.state.search_current = 0;

        let tab_idx = self.state.active_tab;
        let active = self.state.tabs[tab_idx].active;

        if let Some(entry) = self.state.tabs[tab_idx].panes.get(&active) {
            self.state.search_matches =
                search::compute_search_matches(&entry.pane.parser.grid, &query);
        }

        if !self.state.search_matches.is_empty() {
            self.scroll_to_match(0);
        }
    }

    pub(super) fn scroll_to_match(&mut self, idx: usize) {
        if idx >= self.state.search_matches.len() {
            return;
        }
        self.state.search_current = idx;
        let (abs_row, _, _) = self.state.search_matches[idx];

        let tab_idx = self.state.active_tab;
        let active = self.state.tabs[tab_idx].active;

        let (sb_len, grid_rows) = self.state.tabs[tab_idx]
            .panes
            .get(&active)
            .map(|e| (e.pane.parser.grid.scrollback.len(), e.pane.parser.grid.rows))
            .unwrap_or((0, 24));

        let new_offset = search::compute_scroll_offset(abs_row, sb_len, grid_rows);

        if let Some(entry) = self.state.tabs[tab_idx].panes.get_mut(&active) {
            entry.pane.scroll_offset = new_offset;
        }
    }

    pub(super) fn apply_config(&mut self, new_cfg: Config, window: &winit::window::Window) {
        let td = themes_dir();
        let new_theme = load_theme(&new_cfg.theme.name, &td).unwrap_or_else(|e| {
            log::warn!("{e} — keeping current theme");
            self.state.theme.clone()
        });
        self.state.theme = new_theme;
        self.reseed_pane_palettes();
        new_cfg.save();
        window.set_title(&new_cfg.window.title);
        self.state.config = new_cfg;
        self.state.config_panel = None;
    }

    pub(super) fn reseed_pane_palettes(&mut self) {
        let t = &self.state.theme;
        for tab in &mut self.state.tabs {
            for entry in tab.panes.values_mut() {
                let g = &mut entry.pane.parser.grid;
                g.palette = t.palette;
                g.default_fg = t.foreground;
                g.default_bg = t.background;
                g.cursor_color = t.cursor;
                g.selection_color = t.selection;
            }
        }
    }

    pub(super) fn handle_rename_key(&mut self, event: &winit::event::KeyEvent) {
        let buf = if let InputMode::RenameTab { buf } = &self.state.mode {
            buf.clone()
        } else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.state.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::Enter) => {
                let name = buf.trim().to_string();
                self.state.tabs[self.state.active_tab].name =
                    if name.is_empty() { None } else { Some(name) };
                self.state.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::Backspace) => {
                let mut b = buf;
                b.pop();
                self.state.mode = InputMode::RenameTab { buf: b };
            }
            Key::Character(s) => {
                let mut b = buf;
                b.push_str(s);
                self.state.mode = InputMode::RenameTab { buf: b };
            }
            _ => {}
        }
    }

    pub(super) fn handle_screenshot_name_key(&mut self, event: &winit::event::KeyEvent) {
        let (cx, cy, half_w, half_h, name) = if let InputMode::ScreenshotName {
            cx,
            cy,
            half_w,
            half_h,
            name,
        } = &self.state.mode
        {
            (*cx, *cy, *half_w, *half_h, name.clone())
        } else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.state.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::Enter) => {
                let (w, h) = self.surface_size;
                let x = cx.saturating_sub(half_w);
                let y = cy.saturating_sub(half_h);
                let sw = (half_w * 2).min(w.saturating_sub(x));
                let sh = (half_h * 2).min(h.saturating_sub(y));
                self.pending_screenshot = Some(([x, y, sw, sh], name.trim().to_string()));
                self.state.mode = InputMode::Insert;
            }
            Key::Named(NamedKey::Backspace) => {
                let mut n = name;
                n.pop();
                self.state.mode = InputMode::ScreenshotName {
                    cx,
                    cy,
                    half_w,
                    half_h,
                    name: n,
                };
            }
            Key::Character(s) => {
                let mut n = name;
                n.push_str(s);
                self.state.mode = InputMode::ScreenshotName {
                    cx,
                    cy,
                    half_w,
                    half_h,
                    name: n,
                };
            }
            _ => {}
        }
    }

    pub(super) fn handle_config_key(&mut self, event: &winit::event::KeyEvent) {
        let ctrl = self.modifiers.state().control_key();
        let panel = match &mut self.state.config_panel {
            Some(p) => p,
            None => return,
        };

        let shift = self.modifiers.state().shift_key();
        let action = match &event.logical_key {
            Key::Named(NamedKey::Escape) => panel.handle_escape(),
            Key::Named(NamedKey::Enter) => panel.handle_char('\r'),
            Key::Named(NamedKey::Backspace) => panel.handle_backspace(),
            Key::Named(NamedKey::ArrowUp) => panel.handle_up(),
            Key::Named(NamedKey::ArrowDown) => panel.handle_down(),
            Key::Named(NamedKey::ArrowLeft) => panel.handle_left(),
            Key::Named(NamedKey::ArrowRight) => panel.handle_right(),
            Key::Named(NamedKey::Space) => panel.handle_char(' '),
            Key::Named(NamedKey::Tab) => {
                if shift {
                    panel.jump_section_backward()
                } else {
                    panel.jump_section_forward()
                };
                ConfigAction::None
            }
            Key::Character(s) => {
                if ctrl && s.eq_ignore_ascii_case("s") {
                    panel.save()
                } else {
                    let c = s.chars().next().unwrap_or(' ');
                    panel.handle_char(c)
                }
            }
            _ => ConfigAction::None,
        };

        self.apply_config_action(action);
    }

    fn apply_config_action(&mut self, action: ConfigAction) {
        match action {
            ConfigAction::Save(cfg) => {
                if let Some(w) = self.window.clone() {
                    self.apply_config(*cfg, &w);
                }
            }
            ConfigAction::Cancel => {
                self.state.config_panel = None;
            }
            ConfigAction::PreviewTheme(name) => {
                let td = themes_dir();
                match load_theme(&name, &td) {
                    Ok(t) => {
                        self.state.theme = t;
                        self.reseed_pane_palettes();
                    }
                    Err(e) => log::warn!("{e}"),
                }
            }
            ConfigAction::None => {}
        }
    }

    fn active_grid_params(&self) -> (usize, usize, bool) {
        let tab = self.tab();
        tab.panes
            .get(&tab.active)
            .map(|e| {
                (
                    e.pane.parser.grid.cols,
                    e.pane.parser.grid.rows,
                    e.pane.parser.grid.application_cursor_keys,
                )
            })
            .unwrap_or((80, 24, false))
    }

    /// Handle keyboard input while in passthrough mode.
    /// Returns `true` if the event was consumed and the caller should return.
    fn handle_passthrough_key(&mut self, event: &winit::event::KeyEvent, app_cursor: bool) -> bool {
        if event.state == ElementState::Pressed
            && self.modifiers.state().control_key()
            && event.logical_key == Key::Character("b".into())
        {
            self.tab_mut().passthrough = false;
            self.request_redraw();
        } else {
            let action = handle_key_passthrough(event, &self.modifiers, app_cursor);
            if let crate::input::keybindings::Action::SendToPty(bytes) = action {
                self.do_send_to_pty(bytes);
            }
        }
        true
    }

    fn separator_at_pixel(&self, px: u32, py: u32) -> Option<crate::ui::layout::SeparatorHandle> {
        let tab = &self.state.tabs[self.state.active_tab];
        if !tab.zoomed {
            tab.layout.separator_at_pixel(px, py, 4)
        } else {
            None
        }
    }

    pub(super) fn handle_keyboard_input(
        &mut self,
        event: winit::event::KeyEvent,
        event_loop: &ActiveEventLoop,
    ) {
        use std::time::Instant;
        self.state.cursor_blink = true;
        self.state.blink_last = Instant::now();

        let (grid_cols, grid_rows, app_cursor) = self.active_grid_params();

        // Passthrough mode: Ctrl+B exits, everything else goes straight to the PTY.
        if self.tab().passthrough {
            self.handle_passthrough_key(&event, app_cursor);
            return;
        }

        if self.try_dispatch_overlay_key(&event, event_loop) {
            return;
        }

        let action = handle_key(
            &event,
            &self.modifiers,
            &self.state.mode,
            grid_cols,
            grid_rows,
            app_cursor,
        );
        self.execute_action(action, event_loop);
    }

    fn try_dispatch_mode_key(
        &mut self,
        event: &winit::event::KeyEvent,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if matches!(self.state.mode, InputMode::RenameTab { .. }) {
            self.handle_rename_key(event);
            return true;
        }
        if matches!(self.state.mode, InputMode::Search { .. }) {
            self.handle_search_key(event);
            self.request_redraw();
            return true;
        }
        if matches!(self.state.mode, InputMode::CommandPalette { .. }) {
            self.handle_command_palette_key(event, event_loop);
            self.request_redraw();
            return true;
        }
        if matches!(self.state.mode, InputMode::ScreenshotName { .. }) {
            self.handle_screenshot_name_key(event);
            self.request_redraw();
            return true;
        }
        false
    }

    fn try_dispatch_overlay_key(
        &mut self,
        event: &winit::event::KeyEvent,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if self.state.quit_pending {
            let confirmed = matches!(
                event.logical_key,
                Key::Character(ref s) if s.eq_ignore_ascii_case("y")
            );
            self.state.quit_pending = false;
            if confirmed {
                event_loop.exit();
            } else {
                self.request_redraw();
            }
            return true;
        }
        if self.state.config_panel.is_some() {
            self.handle_config_key(event);
            self.request_redraw();
            return true;
        }
        if self.try_dispatch_mode_key(event, event_loop) {
            return true;
        }
        if self.state.ctrl_w_pending {
            self.state.ctrl_w_pending = false;
            let action = handle_ctrl_w(event);
            self.execute_action(action, event_loop);
            return true;
        }
        false
    }

    pub(super) fn handle_cursor_moved(&mut self, px: f64, py: f64) {
        self.state.mouse_pos = Some((px, py));

        if self.state.drag_separator.is_some() {
            self.move_separator_drag(px, py);
            return;
        }

        let hover_sep = self.separator_at_pixel(px as u32, py as u32);

        let url = self.url_at_pixel(px, py);
        let icon = cursor_icon_for_hover(hover_sep.as_ref(), url.is_some());
        if let Some(w) = &self.window {
            w.set_cursor(icon);
        }
        let url_changed = self.state.hovered_url != url;
        self.state.hovered_url = url;
        if url_changed {
            self.request_redraw();
        }
        let (mouse_mode, mouse_sgr) = self.active_mouse_mode();
        if mouse_mode >= 1002 {
            self.report_pty_mouse_move(px, py, mouse_mode, mouse_sgr);
        } else if self.state.mouse_selecting {
            self.update_mouse_selection(px, py);
            self.request_redraw();
        }
    }

    fn move_separator_drag(&mut self, px: f64, py: f64) {
        let handle = match self.state.drag_separator {
            Some(h) => h,
            None => return,
        };
        let new_pos = match handle.dir {
            SplitDir::H => px as u32,
            SplitDir::V => py as u32,
        };
        let ai = self.state.active_tab;
        self.state.tabs[ai].layout.move_separator(handle, new_pos);
        Self::sync_pane_sizes_tab(&mut self.state.tabs[ai]);
        let icon = match handle.dir {
            SplitDir::H => CursorIcon::ColResize,
            SplitDir::V => CursorIcon::RowResize,
        };
        if let Some(w) = &self.window {
            w.set_cursor(icon);
            w.request_redraw();
        }
    }

    fn report_pty_mouse_move(&mut self, px: f64, py: f64, mouse_mode: u16, mouse_sgr: bool) {
        let report = mouse_mode >= 1003 || self.state.mouse_selecting;
        if report {
            let active = self.tab().active;
            if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                let btn = if self.state.mouse_selecting { 32 } else { 35 };
                self.send_mouse_event(btn, col, row, false, mouse_sgr);
            }
        }
    }

    fn handle_left_button(&mut self, state: ElementState) -> bool {
        if state == ElementState::Released && self.state.drag_separator.take().is_some() {
            return true;
        }
        if state == ElementState::Pressed
            && let Some((mx, my)) = self.state.mouse_pos
            && self.try_start_separator_drag(mx, my)
        {
            return true;
        }
        false
    }

    pub(super) fn handle_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        if button == MouseButton::Left && self.handle_left_button(state) {
            return;
        }

        let btn_code = match button {
            MouseButton::Left => 0u8,
            MouseButton::Middle => 1u8,
            MouseButton::Right => 2u8,
            _ => 3u8,
        };
        let (mouse_mode, mouse_sgr) = self.active_mouse_mode();
        if mouse_mode >= 1000 && btn_code < 3 {
            self.send_pty_mouse_click(btn_code, state, button, mouse_sgr);
            return;
        }

        if button == MouseButton::Left {
            self.handle_selection_click(state);
        } else if button == MouseButton::Middle && state == ElementState::Pressed {
            self.do_middle_click_paste();
        }
    }

    fn try_start_separator_drag(&mut self, mx: f64, my: f64) -> bool {
        if let Some(handle) = self.separator_at_pixel(mx as u32, my as u32) {
            self.state.drag_separator = Some(handle);
            true
        } else {
            false
        }
    }

    fn send_pty_mouse_click(
        &mut self,
        btn_code: u8,
        state: ElementState,
        button: MouseButton,
        mouse_sgr: bool,
    ) {
        if let Some((px, py)) = self.state.mouse_pos {
            let active = self.tab().active;
            if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                let release = state == ElementState::Released;
                self.send_mouse_event(btn_code, col, row, release, mouse_sgr);
            }
        }
        if button == MouseButton::Left {
            self.state.mouse_selecting = state == ElementState::Pressed;
        }
    }

    fn handle_selection_click(&mut self, state: ElementState) {
        match state {
            ElementState::Pressed => {
                if let Some((mx, my)) = self.state.mouse_pos {
                    self.start_mouse_selection(mx, my);
                }
            }
            ElementState::Released => {
                if self.state.mouse_selecting {
                    self.state.mouse_selecting = false;
                    self.finish_mouse_selection();
                }
            }
        }
    }

    fn do_middle_click_paste(&mut self) {
        let text = self
            .state
            .clipboard
            .as_mut()
            .and_then(|cb| cb.get_text().ok())
            .or_else(|| Clipboard::new().ok()?.get_text().ok());
        if let Some(text) = text {
            let active = self.tab().active;
            if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
                let mut data = b"\x1b[200~".to_vec();
                data.extend_from_slice(text.as_bytes());
                data.extend_from_slice(b"\x1b[201~");
                let _ = entry.pty.write_input(&data);
            }
        }
    }

    pub(super) fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y,
            MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as f32,
        };
        let (mouse_mode, mouse_sgr) = self.active_mouse_mode();
        if mouse_mode >= 1000 {
            self.send_pty_scroll(lines, mouse_sgr);
        } else {
            self.viewport_scroll(lines);
        }
    }

    fn send_pty_scroll(&mut self, lines: f32, mouse_sgr: bool) {
        let steps = lines.abs().ceil() as usize;
        let btn = if lines > 0.0 { 64u8 } else { 65u8 };
        if let Some((px, py)) = self.state.mouse_pos {
            let active = self.tab().active;
            if let Some((col, row)) = self.pixel_to_cell(active, px, py) {
                for _ in 0..steps.max(1) {
                    self.send_mouse_event(btn, col, row, false, mouse_sgr);
                }
            }
        }
    }

    fn viewport_scroll(&mut self, lines: f32) {
        let n = lines.abs().ceil() as usize;
        let active = self.tab().active;
        if let Some(entry) = self.tab_mut().panes.get_mut(&active) {
            if lines > 0.0 {
                entry.pane.scroll_up(n);
            } else {
                entry.pane.scroll_down(n);
            }
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
