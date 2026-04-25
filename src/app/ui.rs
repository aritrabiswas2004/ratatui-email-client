use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use super::{
    App,
    render::{line_for_field, render_summary_text, render_thread_text, truncate},
    state::{ComposeField, View},
};

impl App {
    pub(super) fn draw(&self, frame: &mut Frame) {
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
