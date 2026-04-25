use std::{
    io,
    sync::mpsc::{Receiver, Sender, channel},
    thread,
};

use ratatui::DefaultTerminal;

use crate::{
    gmail::GmailClient,
    models::{ThreadDetail, ThreadSummary},
};

mod events;
mod input;
mod render;
mod state;
mod ui;

use events::{AppEvent, PostSendAction, Request};
use state::{ComposeField, ComposeOrigin, ComposeState, View};

use crate::{logging, models::ComposeDraft};

const THREAD_LIMIT: usize = 30;

pub struct App {
    pub(crate) exit: bool,
    pub(crate) gmail: GmailClient,
    pub(crate) inbox: Vec<ThreadSummary>,
    pub(crate) selected: usize,
    pub(crate) selected_thread: Option<ThreadDetail>,
    pub(crate) view: View,
    pub(crate) status: String,
    pub(crate) compose: Option<ComposeState>,
    pub(crate) pending: Option<Request>,
    pub(crate) tx: Sender<AppEvent>,
    pub(crate) rx: Receiver<AppEvent>,
}

impl App {
    pub fn new(gmail: GmailClient) -> Self {
        let (tx, rx) = channel();

        Self {
            exit: false,
            gmail,
            inbox: Vec::new(),
            selected: 0,
            selected_thread: None,
            view: View::Loading("Loading inbox...".into()),
            status: "Press r to load mail, q to quit.".into(),
            compose: None,
            pending: None,
            tx,
            rx,
        }
    }

    pub fn new_for_testonly() -> Self {
        let (tx, rx) = channel();

        Self {
            exit: false,
            gmail: GmailClient::new_stub(),
            inbox: vec![],
            selected: 0,
            selected_thread: None,
            view: View::Loading("Loading inbox...".into()),
            status: "Press r to load mail, q to quit.".into(),
            compose: None,
            pending: None,
            tx,
            rx,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        self.request_inbox();

        while !self.exit {
            self.drain_events();
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_input()?;
        }

        Ok(())
    }

    pub(crate) fn request_inbox(&mut self) {
        if self.pending.is_some() {
            return;
        }

        self.view = View::Loading("Loading inbox...".into());
        self.status = "Loading inbox...".into();
        self.pending = Some(Request::Inbox);

        let gmail = self.gmail.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = gmail
                .list_inbox(THREAD_LIMIT)
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::InboxLoaded(result));
        });
    }

    pub(crate) fn request_thread(&mut self, thread_id: String) {
        if self.pending.is_some() {
            return;
        }

        self.view = View::Loading("Loading thread...".into());
        self.status = "Loading thread...".into();
        self.pending = Some(Request::Thread);

        let gmail = self.gmail.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = gmail.get_thread(&thread_id).map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::ThreadLoaded(result));
        });
    }

    pub(crate) fn open_new_compose(&mut self) {
        self.compose = Some(ComposeState {
            draft: ComposeDraft::new(),
            field: ComposeField::To,
            origin: match self.view {
                View::Thread => self
                    .selected_thread
                    .as_ref()
                    .map(|thread| ComposeOrigin::Thread(thread.id.clone()))
                    .unwrap_or(ComposeOrigin::Inbox),
                _ => ComposeOrigin::Inbox,
            },
            error: None,
        });
        self.view = View::Compose;
        self.status = "Compose a new email. Ctrl+S sends, Esc cancels.".into();
    }

    pub(crate) fn open_reply_compose(&mut self) {
        let thread = match self.selected_thread.as_ref() {
            Some(thread) => thread,
            None => {
                self.status = "Open a thread before replying.".into();
                return;
            }
        };

        self.compose = Some(ComposeState {
            draft: ComposeDraft::for_reply(thread),
            field: ComposeField::Body,
            origin: ComposeOrigin::Thread(thread.id.clone()),
            error: None,
        });
        self.view = View::Compose;
        self.status = "Replying to thread. Ctrl+S sends, Esc cancels.".into();
    }

    pub(crate) fn cancel_compose(&mut self) {
        if let Some(compose) = self.compose.take() {
            self.view = match compose.origin {
                ComposeOrigin::Inbox => View::Inbox,
                ComposeOrigin::Thread(_) => {
                    if self.selected_thread.is_some() {
                        View::Thread
                    } else {
                        View::Inbox
                    }
                }
            };
        }
    }

    pub(crate) fn send_compose(&mut self) {
        logging::info("ACTION send_compose invoked");

        if self.pending.is_some() {
            logging::warn("ACTION send_compose skipped because pending request exists");
            return;
        }

        let Some(compose) = self.compose.clone() else {
            logging::warn("ACTION send_compose skipped because compose state missing");
            return;
        };

        if compose.draft.to.trim().is_empty() {
            logging::warn("ACTION send_compose blocked: To field is empty");
            self.status = "The To field is required.".into();
            if let Some(current) = self.compose.as_mut() {
                current.error = Some("The To field is required.".into());
            }
            return;
        }

        let post_send = match compose.origin {
            ComposeOrigin::Inbox => PostSendAction::RefreshInbox,
            ComposeOrigin::Thread(thread_id) => PostSendAction::OpenThread(thread_id),
        };

        self.view = View::Loading("Sending message...".into());
        self.status = "Sending message...".into();
        self.pending = Some(Request::Send);
        logging::info("ACTION send_compose queued async Gmail send request");

        let gmail = self.gmail.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            logging::info("ACTION async send thread started");
            let result = gmail
                .send_message(&compose.draft)
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::MessageSent(result, post_send));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ComposeDraft, ThreadDetail};
    use state::{ComposeField, ComposeOrigin};

    fn thread_summary(id: &str, subject: &str) -> ThreadSummary {
        ThreadSummary {
            id: id.into(),
            subject: subject.into(),
            from: "Sender <sender@example.com>".into(),
            date: "Mon, 01 Jan 2024 00:00:00 +0000".into(),
            snippet: "snippet".into(),
            message_count: 1,
            unread: false,
        }
    }

    #[test]
    fn selection_clamps_to_inbox_bounds() {
        let mut app = App::new_for_testonly();
        app.inbox = vec![thread_summary("1", "One"), thread_summary("2", "Two")];

        app.move_selection(1);
        app.move_selection(1);
        assert_eq!(app.selected, 1);

        app.move_selection(-10);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn cancel_compose_returns_to_origin_view() {
        let mut app = App::new_for_testonly();
        app.selected_thread = Some(ThreadDetail {
            id: "thread".into(),
            subject: "Subject".into(),
            snippet: "snippet".into(),
            messages: vec![],
        });
        app.compose = Some(ComposeState {
            draft: ComposeDraft::new(),
            field: ComposeField::To,
            origin: ComposeOrigin::Thread("thread".into()),
            error: None,
        });
        app.view = View::Compose;

        app.cancel_compose();

        assert!(matches!(app.view, View::Thread));
        assert!(app.compose.is_none());
    }
}
