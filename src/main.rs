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

    terminal.clear();

    loop {
        terminal.draw(|f| {
            let size = f.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50)
                ])
                .split(size);

            let block = Block::default()
                .title("Testing the TUI")
                .borders(Borders::ALL);

            let desc_block = Block::default()
                .title("Second Block")
                .borders(Borders::ALL);


            let paragraph = Paragraph::new(format!("Counter: {}", counter)).block(block);

            let desc_para = Paragraph::new("Description").block(desc_block);

            f.render_widget(paragraph, chunks[0]);
            f.render_widget(desc_para, chunks[1]);
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
