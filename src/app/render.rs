use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

use crate::models::{ThreadDetail, ThreadSummary};

pub fn render_summary_text(summary: &ThreadSummary) -> Text<'static> {
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

pub fn render_thread_text(thread: &ThreadDetail) -> Text<'static> {
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

pub fn truncate(value: &str, limit: usize) -> String {
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
