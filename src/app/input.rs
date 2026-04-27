use std::{io, time::Duration};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::{App, events::PostSendAction, state::*};
use crate::logging;

impl App {
    pub(super) fn drain_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            self.handle_event(event);
        }
    }

    pub(super) fn handle_event(&mut self, event: super::events::AppEvent) {
        match event {
            super::events::AppEvent::InboxLoaded(result) => {
                self.pending = None;
                self.view = View::Inbox;
                match result {
                    Ok(items) => {
                        self.inbox = items;
                        self.selected = self.selected.min(self.inbox.len().saturating_sub(1));
                        self.status = format!("Loaded {} thread(s).", self.inbox.len());
                    }
                    Err(err) => {
                        self.inbox.clear();
                        self.selected = 0;
                        self.status = format!("Failed to load inbox: {err}");
                    }
                }
            }
            super::events::AppEvent::ThreadLoaded(result) => {
                self.pending = None;
                match result {
                    Ok(thread) => {
                        self.status = format!("Opened thread {}", thread.subject);
                        self.selected_thread = Some(thread);
                        self.view = View::Thread;
                    }
                    Err(err) => {
                        self.selected_thread = None;
                        self.view = View::Inbox;
                        self.status = format!("Failed to load thread: {err}");
                    }
                }
            }
            super::events::AppEvent::MessageSent(result, post_send) => {
                self.pending = None;
                match result {
                    Ok(()) => {
                        self.status = "Message sent.".into();
                        self.compose = None;
                        match post_send {
                            PostSendAction::RefreshInbox => self.request_inbox(),
                            PostSendAction::OpenThread(thread_id) => self.request_thread(thread_id),
                        }
                    }
                    Err(err) => {
                        self.status = format!("Failed to send message: {err}");
                        self.view = View::Compose;
                    }
                }
            }
        }
    }

    pub(super) fn handle_input(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press {
                    self.handle_key_event(key_event);
                }
            }
        }

        Ok(())
    }

    pub(super) fn handle_key_event(&mut self, key_event: KeyEvent) {
        logging::info(&format!(
            "KEY_PRESS view={:?} code={:?} modifiers={:?}",
            self.view, key_event.code, key_event.modifiers
        ));

        if self.compose.is_some() {
            logging::info("ACTION handle_compose_key");
            self.handle_compose_key(key_event);
            return;
        }

        match self.view {
            View::Loading(_) => {
                if matches!(key_event.code, KeyCode::Char('q')) {
                    self.exit = true;
                }
            }
            View::Inbox => self.handle_inbox_key(key_event),
            View::Thread => self.handle_thread_key(key_event),
            View::Compose => {}
        }
    }

    pub(super) fn handle_inbox_key(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit = true,
            KeyCode::Char('r') => self.request_inbox(),
            KeyCode::Char('n') => self.open_new_compose(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Enter => {
                if let Some(thread) = self.inbox.get(self.selected) {
                    self.request_thread(thread.id.clone());
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_thread_key(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit = true,
            KeyCode::Esc | KeyCode::Char('b') => {
                self.view = View::Inbox;
                self.selected_thread = None;
            }
            KeyCode::Char('r') => {
                if let Some(thread) = &self.selected_thread {
                    self.request_thread(thread.id.clone());
                }
            }
            KeyCode::Char('n') => self.open_new_compose(),
            KeyCode::Char('y') => self.open_reply_compose(),
            _ => {}
        }
    }

    pub(super) fn handle_compose_key(&mut self, key_event: KeyEvent) {
        let is_ctrl_s = matches!(key_event.code, KeyCode::Char('s') | KeyCode::Char('S'))
            && key_event.modifiers.contains(KeyModifiers::CONTROL);

        if is_ctrl_s {
            logging::info("ACTION send_compose via Ctrl+S variant");
            self.send_compose();
            return;
        }

        if matches!(key_event.code, KeyCode::Char('|')) {
            logging::info("ACTION send_compose via '|'");
            self.send_compose();
            return;
        }

        match (key_event.code, key_event.modifiers) {
            (KeyCode::Char('q'), _) => self.exit = true,
            (KeyCode::Esc, _) => self.cancel_compose(),
            (KeyCode::Tab, _) => {
                logging::info("ACTION compose field next");
                if let Some(compose) = self.compose.as_mut() {
                    compose.move_to_next_field();
                }
            }
            (KeyCode::BackTab, _) => {
                logging::info("ACTION compose field previous");
                if let Some(compose) = self.compose.as_mut() {
                    compose.move_to_previous_field();
                }
            }
            _ => {
                let Some(compose) = self.compose.as_mut() else {
                    return;
                };

                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Left, _) => compose.move_cursor_left(),
                    (KeyCode::Right, _) => compose.move_cursor_right(),
                    (KeyCode::Up, _) => compose.move_cursor_up(),
                    (KeyCode::Down, _) => compose.move_cursor_down(),
                    (KeyCode::Home, _) => compose.move_cursor_home(),
                    (KeyCode::End, _) => compose.move_cursor_end(),
                    (KeyCode::Enter, _) => {
                        logging::info("ACTION compose body newline");
                        compose.insert_newline();
                    }
                    (KeyCode::Backspace, _) => {
                        logging::info("ACTION compose backspace");
                        compose.backspace();
                    }
                    (KeyCode::Char(ch), KeyModifiers::NONE)
                    | (KeyCode::Char(ch), KeyModifiers::SHIFT) => {
                        logging::info("ACTION compose type active field");
                        compose.insert_char(ch);
                    }
                    _ => {}
                }

                compose.sync_cursors_to_text();
            }
        }
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        if self.inbox.is_empty() {
            return;
        }

        let max_index = self.inbox.len().saturating_sub(1) as isize;
        let next = (self.selected as isize + delta).clamp(0, max_index);
        self.selected = next as usize;
    }
}
