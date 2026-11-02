use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, DeleteRequest, Mode};
use crate::clean::CleanScope;

const MAX_INPUT_LEN: usize = 32;
const MAX_COUNT: usize = 99_999;
const PAGE: usize = 10;

impl App {
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        match self.mode {
            Mode::Browse => self.on_browse_key(key),
            Mode::Search => self.on_search_key(key),
            Mode::Help => self.mode = Mode::Browse,
            Mode::ConfirmClean(_) => self.on_confirm_clean_key(key),
            Mode::ConfirmDelete { .. } => self.on_confirm_delete_key(key),
        }
    }

    fn on_browse_key(&mut self, key: KeyEvent) {
        if let KeyCode::Char(digit @ '0'..='9') = key.code
            && (digit != '0' || self.count.is_some())
        {
            let digit = digit.to_digit(10).map_or(0, |d| d as usize);
            self.count = Some((self.count.unwrap_or(0) * 10 + digit).min(MAX_COUNT));
            return;
        }

        let count = self.count.take();
        let pending_g = std::mem::take(&mut self.pending_g);
        let steps = count.unwrap_or(1);
        let line = count.map(|n| n - 1);

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.rebuild_view();
            }
            KeyCode::Esc => self.marked.clear(),
            KeyCode::Char('/') => self.mode = Mode::Search,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('f') => {
                self.filter = self.filter.next();
                self.rebuild_view();
            }
            KeyCode::Char('o') => self.open_selected(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(steps),
            KeyCode::Up | KeyCode::Char('k') => self.move_up(steps),
            KeyCode::PageDown => self.move_down(steps * PAGE),
            KeyCode::PageUp => self.move_up(steps * PAGE),
            KeyCode::Char('g') if pending_g => self.move_to(line.unwrap_or(0)),
            KeyCode::Char('g') => {
                self.pending_g = true;
                self.count = count;
            }
            KeyCode::Char('G') => self.move_to(line.unwrap_or(usize::MAX)),
            KeyCode::Home => self.move_to(0),
            KeyCode::End => self.move_to(usize::MAX),
            KeyCode::Char(' ') => self.toggle_mark(),
            KeyCode::Char('a') => self.toggle_mark_all(),
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.sort_reversed = false;
                self.rebuild_view();
            }
            KeyCode::Char('S') => {
                self.sort_reversed = !self.sort_reversed;
                self.rebuild_view();
            }
            KeyCode::Char('r') => self.rescan(),
            KeyCode::Char('n') => self.request_clean(CleanScope::Deps),
            KeyCode::Char('c') => self.request_clean(CleanScope::Cache),
            KeyCode::Char('h') => self.request_clean(CleanScope::Claude),
            KeyCode::Char('x') => self.request_clean(CleanScope::All),
            KeyCode::Char('d') => self.request_delete(),
            _ => {}
        }
    }

    fn on_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.mode = Mode::Browse,
            KeyCode::Esc => {
                self.query.clear();
                self.mode = Mode::Browse;
            }
            KeyCode::Down => self.move_down(1),
            KeyCode::Up => self.move_up(1),
            KeyCode::Backspace => {
                self.query.pop();
            }
            KeyCode::Char(c) if self.query.len() < MAX_INPUT_LEN => self.query.push(c),
            _ => return,
        }
        self.rebuild_view();
    }

    fn on_confirm_clean_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Mode::ConfirmClean(request) = std::mem::replace(&mut self.mode, Mode::Browse)
                {
                    self.start_clean(request);
                }
            }
            KeyCode::Char('n' | 'q') | KeyCode::Esc => self.mode = Mode::Browse,
            _ => {}
        }
    }

    fn on_confirm_delete_key(&mut self, key: KeyEvent) {
        let Mode::ConfirmDelete { request, input } = &mut self.mode else {
            return;
        };
        let confirmed = if request.requires_typing() {
            match key.code {
                KeyCode::Enter => input == DeleteRequest::CONFIRM_WORD,
                KeyCode::Backspace => {
                    input.pop();
                    false
                }
                KeyCode::Char(c) if input.len() < MAX_INPUT_LEN => {
                    input.push(c);
                    false
                }
                KeyCode::Esc => {
                    self.mode = Mode::Browse;
                    return;
                }
                _ => false,
            }
        } else {
            match key.code {
                KeyCode::Char('y') | KeyCode::Enter => true,
                KeyCode::Char('n' | 'q') | KeyCode::Esc => {
                    self.mode = Mode::Browse;
                    return;
                }
                _ => false,
            }
        };

        if confirmed
            && let Mode::ConfirmDelete { request, .. } =
                std::mem::replace(&mut self.mode, Mode::Browse)
        {
            let confirmed_unsafe = request.requires_typing();
            self.start_delete(request, confirmed_unsafe);
        }
    }
}
