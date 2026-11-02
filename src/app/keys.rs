use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, DeleteRequest, Mode};
use crate::clean::CleanScope;

const MAX_INPUT_LEN: usize = 32;

impl App {
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        match self.mode {
            Mode::Browse => self.on_browse_key(key),
            Mode::ConfirmClean(_) => self.on_confirm_clean_key(key),
            Mode::ConfirmDelete { .. } => self.on_confirm_delete_key(key),
        }
    }

    fn on_browse_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => self.marked.clear(),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::PageDown => self.move_by(10),
            KeyCode::PageUp => self.move_by(-10),
            KeyCode::Home | KeyCode::Char('g') => self.move_to(0),
            KeyCode::End | KeyCode::Char('G') => self.move_to(usize::MAX),
            KeyCode::Char(' ') => self.toggle_mark(),
            KeyCode::Char('a') => self.toggle_mark_all(),
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.rebuild_view();
            }
            KeyCode::Char('r') => self.rescan(),
            KeyCode::Char('n') => self.request_clean(CleanScope::Deps),
            KeyCode::Char('c') => self.request_clean(CleanScope::Cache),
            KeyCode::Char('x') => self.request_clean(CleanScope::All),
            KeyCode::Char('d') => self.request_delete(),
            _ => {}
        }
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
