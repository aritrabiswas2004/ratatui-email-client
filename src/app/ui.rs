use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use super::{
    App,
    render::{render_summary_text, render_thread_text, truncate},
    state::{ComposeField, View, line_col_for_index},
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
        let footer_height = if self
            .compose
            .as_ref()
            .and_then(|compose| compose.error.as_ref())
            .is_some()
        {
            3
        } else {
            2
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(footer_height),
            ])
            .split(inner);

        let compose = self.compose.as_ref();
        let draft = compose.map(|compose| &compose.draft);
        let field = compose.map(|compose| compose.field);

        self.draw_compose_field(
            frame,
            chunks[0],
            "To",
            draft.map(|draft| draft.to.as_str()).unwrap_or(""),
            compose.map(|compose| compose.to_cursor).unwrap_or(0),
            field == Some(ComposeField::To),
        );
        self.draw_compose_field(
            frame,
            chunks[1],
            "Subject",
            draft.map(|draft| draft.subject.as_str()).unwrap_or(""),
            compose.map(|compose| compose.subject_cursor).unwrap_or(0),
            field == Some(ComposeField::Subject),
        );
        self.draw_compose_body(frame, chunks[2]);
        self.draw_compose_footer(frame, chunks[3]);
    }

    fn draw_compose_field(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        value: &str,
        cursor: usize,
        active: bool,
    ) {
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(if active {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            });
        let inner = block.inner(area);
        let inner_width = inner.width as usize;
        let scroll_x = cursor.saturating_sub(inner_width.saturating_sub(1));
        let paragraph = Paragraph::new(value.to_string()).block(block).scroll((0, scroll_x as u16));
        frame.render_widget(paragraph, area);

        if active && inner_width > 0 {
            frame.set_cursor_position((
                inner.x + (cursor - scroll_x) as u16,
                inner.y,
            ));
        }
    }

    fn draw_compose_body(&self, frame: &mut Frame, area: Rect) {
        let Some(compose) = self.compose.as_ref() else {
            return;
        };

        let block = Block::default()
            .title("Body")
            .borders(Borders::ALL)
            .border_style(if compose.field == ComposeField::Body {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            });
        let inner = block.inner(area);
        let inner_width = inner.width as usize;
        let inner_height = inner.height as usize;
        let (line, col) = line_col_for_index(&compose.draft.body, compose.body_cursor);
        let scroll_y = line.saturating_sub(inner_height.saturating_sub(1));
        let scroll_x = col.saturating_sub(inner_width.saturating_sub(1));
        let body_text = if compose.draft.body.is_empty() {
            Text::from("")
        } else {
            Text::from(
                compose
                    .draft
                    .body
                    .split('\n')
                    .map(|line| Line::from(line.to_string()))
                    .collect::<Vec<_>>(),
            )
        };
        let paragraph = Paragraph::new(body_text)
            .block(block)
            .scroll((scroll_y as u16, scroll_x as u16));
        frame.render_widget(paragraph, area);

        if compose.field == ComposeField::Body && inner_width > 0 && inner_height > 0 {
            frame.set_cursor_position((
                inner.x + (col - scroll_x) as u16,
                inner.y + (line - scroll_y) as u16,
            ));
        }
    }

    fn draw_compose_footer(&self, frame: &mut Frame, area: Rect) {
        let compose = self.compose.as_ref();
        let mut lines = vec![Line::from(
            "Tab cycles fields, Shift+Tab goes back, Ctrl+S sends, Esc cancels, q quits.",
        )];

        if let Some(error) = compose.and_then(|compose| compose.error.as_deref()) {
            lines.push(Line::from(vec![
                Span::styled("Error: ", Style::default().fg(Color::Red)),
                Span::raw(error.to_string()),
            ]));
        }

        let paragraph = Paragraph::new(Text::from(lines));
        frame.render_widget(paragraph, area);
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
