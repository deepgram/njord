use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::io;

pub struct UI {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl UI {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        
        Ok(Self { terminal })
    }
    
    pub fn cleanup(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }
    
    pub fn draw_welcome(&mut self) -> Result<()> {
        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Min(0)].as_ref())
                .split(f.size());
            
            let welcome_text = vec![
                Line::from(vec![
                    Span::styled("Njord", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(" - Interactive LLM REPL"),
                ]),
                Line::from(""),
                Line::from("Named after the Norse god of the sea and sailors,"),
                Line::from("Njord guides you through the vast ocean of AI conversations."),
                Line::from(""),
                Line::from("Type your message or use slash commands:"),
                Line::from("  /help - Show all commands"),
                Line::from("  /models - List available models"),
                Line::from("  /quit - Exit Njord"),
            ];
            
            let paragraph = Paragraph::new(welcome_text)
                .block(Block::default().title("Welcome").borders(Borders::ALL))
                .wrap(Wrap { trim: true });
            
            f.render_widget(paragraph, chunks[0]);
        })?;
        
        Ok(())
    }
    
    pub fn read_input(&mut self) -> Result<Option<String>> {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') => return Ok(Some("/quit".to_string())),
                    KeyCode::Esc => return Ok(Some("/quit".to_string())),
                    _ => {}
                }
            }
        }
        Ok(None)
    }
}

impl Drop for UI {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
