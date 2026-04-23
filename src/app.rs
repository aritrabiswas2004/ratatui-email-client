use std::{
    io,
    sync::mpsc::{Receiver, Sender, channel},
    thread,
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    gmail::GmailClient,
    logging,
    models::{ComposeDraft, ThreadDetail, ThreadSummary},
};

const THREAD_LIMIT: usize = 30;

pub struct App {
    exit: bool,
    gmail: GmailClient,
    inbox: Vec<ThreadSummary>,
    selected: usize,
    selected_thread: Option<ThreadDetail>,
    view: View,
    status: String,
    compose: Option<ComposeState>,
    pending: Option<Request>,
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
}

#[derive(Debug, Clone)]
enum View {
    Loading(String),
    Inbox,
    Thread,
    Compose,
}

#[derive(Debug, Clone)]
enum Request {
    Inbox,
    Thread,
    Send,
}

#[derive(Debug, Clone)]
enum PostSendAction {
    RefreshInbox,
    OpenThread(String),
}

#[derive(Debug)]
enum AppEvent {
    InboxLoaded(Result<Vec<ThreadSummary>, String>),
    ThreadLoaded(Result<ThreadDetail, String>),
    MessageSent(Result<(), String>, PostSendAction),
}

#[derive(Debug, Clone)]
struct ComposeState {
    draft: ComposeDraft,
    field: ComposeField,
    origin: ComposeOrigin,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComposeField {
    To,
    Subject,
    Body,
}

#[derive(Debug, Clone)]
enum ComposeOrigin {
    Inbox,
    Thread(String),
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
            gmail: GmailClient {
                pub client: reqwest::blocking::Client::new(),
                pub access_token: String::new(),
                pub sender_email: "test@example.com".into(),
            },
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

    fn drain_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::InboxLoaded(result) => {
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
            AppEvent::ThreadLoaded(result) => {
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
            AppEvent::MessageSent(result, post_send) => {
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

    fn handle_input(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press {
                    self.handle_key_event(key_event);
                }
            }
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
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

    fn handle_inbox_key(&mut self, key_event: KeyEvent) {
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

    fn handle_thread_key(&mut self, key_event: KeyEvent) {
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

    fn handle_compose_key(&mut self, key_event: KeyEvent) {
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
            (KeyCode::Tab, _) | (KeyCode::Enter, _)
                if self
                    .compose
                    .as_ref()
                    .map(|compose| compose.field != ComposeField::Body)
                    .unwrap_or(false) =>
            {
                logging::info("ACTION compose field next");
                if let Some(compose) = self.compose.as_mut() {
                    compose.field = compose.field.next();
                }
            }
            (KeyCode::BackTab, _) => {
                logging::info("ACTION compose field previous");
                if let Some(compose) = self.compose.as_mut() {
                    compose.field = compose.field.previous();
                }
            }
            _ => {
                let Some(compose) = self.compose.as_mut() else {
                    return;
                };

                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Enter, _) => {
                        logging::info("ACTION compose body newline");
                        compose.draft.body.push('\n');
                    }
                    (KeyCode::Backspace, _) => {
                        logging::info("ACTION compose backspace");
                        match compose.field {
                            ComposeField::To => {
                                compose.draft.to.pop();
                            }
                            ComposeField::Subject => {
                                compose.draft.subject.pop();
                            }
                            ComposeField::Body => {
                                compose.draft.body.pop();
                            }
                        }
                    }
                    (KeyCode::Char(ch), KeyModifiers::NONE)
                    | (KeyCode::Char(ch), KeyModifiers::SHIFT) => match compose.field {
                        ComposeField::To => {
                            logging::info("ACTION compose type To");
                            compose.draft.to.push(ch);
                        }
                        ComposeField::Subject => {
                            logging::info("ACTION compose type Subject");
                            compose.draft.subject.push(ch);
                        }
                        ComposeField::Body => {
                            logging::info("ACTION compose type Body");
                            compose.draft.body.push(ch);
                        }
                    },
                    _ => {}
                }
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.inbox.is_empty() {
            return;
        }

        let max_index = self.inbox.len().saturating_sub(1) as isize;
        let next = (self.selected as isize + delta).clamp(0, max_index);
        self.selected = next as usize;
    }

    fn request_inbox(&mut self) {
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

    fn request_thread(&mut self, thread_id: String) {
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

    fn open_new_compose(&mut self) {
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

    fn open_reply_compose(&mut self) {
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

    fn cancel_compose(&mut self) {
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

    fn send_compose(&mut self) {
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

    fn draw(&self, frame: &mut Frame) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(frame.area());

        match self.view {
            View::Inbox | View::Loading(_) => self.draw_inbox(frame, areas[0]),
            View::Thread => self.draw_thread(frame, areas[0]),
            View::Compose => self.draw_compose(frame, areas[0]),
        }

        self.draw_status(frame, areas[1]);
    }

    fn draw_inbox(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);

        let items = self
            .inbox
            .iter()
            .map(|thread| {
                let mut lines = vec![Line::from(vec![
                    Span::styled(
                        truncate(&thread.from, 28),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        truncate(&thread.subject, 36),
                        if thread.unread {
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ),
                ])];
                lines.push(Line::from(truncate(&thread.snippet, 60)));
                lines.push(Line::from(format!("{} message(s)", thread.message_count)));
                ListItem::new(Text::from(lines))
            })
            .collect::<Vec<_>>();

        let mut state = ListState::default();
        if !self.inbox.is_empty() {
            state.select(Some(self.selected));
        }

        let list = List::new(items)
            .block(Block::default().title("Inbox").borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_stateful_widget(list, chunks[0], &mut state);

        let detail = self
            .selected_thread
            .as_ref()
            .and_then(|thread| self.inbox.iter().find(|item| item.id == thread.id).cloned())
            .or_else(|| self.inbox.get(self.selected).cloned());

        let content = match (self.selected_thread.as_ref(), detail) {
            (Some(thread_detail), _) if matches!(self.view, View::Thread) => {
                render_thread_text(thread_detail)
            }
            (_, Some(summary)) => render_summary_text(&summary),
            _ => Text::from("Select a thread to preview it.\n\nPress n to compose a new message."),
        };

        let preview = Paragraph::new(content)
            .block(Block::default().title("Preview").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(preview, chunks[1]);
    }

    fn draw_thread(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().title("Thread").borders(Borders::ALL);
        let body = self
            .selected_thread
            .as_ref()
            .map(render_thread_text)
            .unwrap_or_else(|| Text::from("No thread loaded."));

        let paragraph = Paragraph::new(body).block(block).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn draw_compose(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().title("Compose").borders(Borders::ALL);
        frame.render_widget(block, area);

        let inner = area.inner(ratatui::layout::Margin::new(2, 1));
        let compose = self.compose.as_ref();
        let draft = compose.map(|compose| &compose.draft);
        let field = compose.map(|compose| compose.field);

        let mut lines = Vec::new();
        lines.push(line_for_field(
            "To",
            draft.map(|draft| draft.to.as_str()).unwrap_or(""),
            field == Some(ComposeField::To),
        ));
        lines.push(line_for_field(
            "Subject",
            draft.map(|draft| draft.subject.as_str()).unwrap_or(""),
            field == Some(ComposeField::Subject),
        ));
        lines.push(Line::from("Body:"));

        let body_text = draft.map(|draft| draft.body.as_str()).unwrap_or("");
        for line in body_text.lines() {
            lines.push(Line::from(line.to_string()));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(
            "Tab/Enter move fields, Ctrl+S or F5 sends, Esc cancels, q quits.",
        ));

        if let Some(error) = compose.and_then(|compose| compose.error.as_deref()) {
            lines.push(Line::from(vec![
                Span::styled("Error: ", Style::default().fg(Color::Red)),
                Span::raw(error.to_string()),
            ]));
        }

        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }

    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        let status = match &self.view {
            View::Loading(message) => format!("{message}  |  {}", self.status),
            View::Inbox => format!("Inbox  |  {}", self.status),
            View::Thread => format!("Thread  |  {}", self.status),
            View::Compose => format!("Compose  |  {}", self.status),
        };

        let footer = Paragraph::new(status)
            .block(Block::default().borders(Borders::ALL).title("Status"))
            .wrap(Wrap { trim: false });
        frame.render_widget(footer, area);
    }
}

fn line_for_field(label: &str, value: &str, active: bool) -> Line<'static> {
    let style = if active {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::styled(format!("{label}: "), style),
        Span::styled(value.to_string(), style),
    ])
}

fn render_summary_text(summary: &ThreadSummary) -> Text<'static> {
    Text::from(vec![
        Line::from(vec![
            Span::styled("From: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(summary.from.clone()),
        ]),
        Line::from(vec![
            Span::styled("Subject: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(summary.subject.clone()),
        ]),
        Line::from(vec![
            Span::styled("Date: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(summary.date.clone()),
        ]),
        Line::from(""),
        Line::from(summary.snippet.clone()),
        Line::from(""),
        Line::from("Press Enter to open the thread, n to compose, y to reply."),
    ])
}

fn render_thread_text(thread: &ThreadDetail) -> Text<'static> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Subject: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(thread.subject.clone()),
        ]),
        Line::from(vec![
            Span::styled("Messages: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(thread.messages.len().to_string()),
        ]),
        Line::from(""),
    ];

    for (index, message) in thread.messages.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(
                format!("Message {}", index + 1),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  {}", message.date)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("From: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(message.from.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("To: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(message.to.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Subject: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(message.subject.clone()),
        ]));
        lines.push(Line::from(""));
        for line in message.body.lines() {
            lines.push(Line::from(line.to_string()));
        }
        lines.push(Line::from(""));
        lines.push(Line::from("────────────────────────────────────────"));
        lines.push(Line::from(""));
    }

    Text::from(lines)
}

fn truncate(value: &str, limit: usize) -> String {
    let text = value.trim();
    let char_count = text.chars().count();

    if char_count <= limit {
        return text.to_string();
    }

    if limit == 0 {
        return String::new();
    }

    let mut truncated: String = text.chars().take(limit.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

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

impl ComposeField {
    fn next(self) -> Self {
        match self {
            ComposeField::To => ComposeField::Subject,
            ComposeField::Subject => ComposeField::Body,
            ComposeField::Body => ComposeField::To,
        }
    }

    fn previous(self) -> Self {
        match self {
            ComposeField::To => ComposeField::Body,
            ComposeField::Subject => ComposeField::To,
            ComposeField::Body => ComposeField::Subject,
        }
    }
}
