use std::sync::mpsc::{self, Sender, Receiver};
use std::thread;
use std::time::Duration;
use anyhow::Result;
use crossterm::{
    terminal::{enable_raw_mode, disable_raw_mode},
    event::{self, Event, KeyCode, KeyModifiers},
};
use ratatui::{prelude::*, widgets::*};

pub struct RatatuiApp {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}

impl RatatuiApp {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    fn run_loop(&mut self, rx: Receiver<String>) -> Result<()> {
        let mut logs: Vec<String> = vec![];

        loop {
            while let Ok(line) = rx.try_recv() {
                logs.push(line);
                if logs.len() > 200 {
                    logs.drain(0..logs.len() - 200);
                }
            }

            self.terminal.draw(|f| {
                let size = f.size();
                let block = Block::default()
                    .title("Logs (q / Ctrl+C for exit)")
                    .borders(Borders::ALL);
                let paragraph = Paragraph::new(logs.join("\n")).block(block);
                f.render_widget(paragraph, size);
            })?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q')
                        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
                    {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn spawn(rx: Receiver<String>) -> thread::JoinHandle<Result<()>> {
        thread::spawn(move || {
            let mut app = RatatuiApp::new()?;
            let res = app.run_loop(rx);
            disable_raw_mode()?;
            res
        })
    }
}
