use std::io;

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    widgets::{Block, Borders, Paragraph},
    layout::{Layout, Constraint, Direction},
};

fn main() -> Result<(), io::Error>{
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut counter = 0;

    terminal.clear().expect("Failed to CLSSCR");

    loop {
        terminal.draw(|f| {
            let size = f.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(100),
                ])
                .split(size);

            let block = Block::default()
                .title("Testing the TUI - Press q to quit")
                .borders(Borders::ALL);

            let paragraph = Paragraph::new(format!("Counter: {}", counter)).block(block);

            f.render_widget(paragraph, chunks[0]);
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            if key.code == KeyCode::Char('q') {
                terminal.clear().expect("Failed to CLSSCR");
                break;
            }

            if key.code == KeyCode::Char('e'){
                counter += 1;
            }
        }
    }

    disable_raw_mode()?;
    Ok(())
}
