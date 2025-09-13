use std::{
    collections::{HashMap, VecDeque},
    io,
    thread,
    time::{Duration, SystemTime},
};
use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender, select};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem},
    Terminal,
};
use crossterm::{
    event::{self, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crate::capture::PlatformCapture;
use crate::capture::StdoutCapture;

#[derive(Clone, Debug)]
pub enum ResourceStatus {
    Loading,
    Loaded,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct ResourceInfo {
    pub id: String,
    pub rtype: String,
    pub status: ResourceStatus,
    pub size_bytes: Option<usize>,
    pub progress: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct InputEvent {
    pub ts: SystemTime,
    pub kind: String,
    pub detail: String,
}

enum ControlMsg {
    Pause,
    Resume,
    ClearLogs,
    Shutdown,
}

pub struct DebugUiHandle {
    resource_tx: Sender<ResourceInfo>,
    input_tx: Sender<InputEvent>,
    log_tx: Sender<String>,
    ctrl_tx: Sender<ControlMsg>,
    join_handle: Option<thread::JoinHandle<()>>,
    capture: Option<PlatformCapture>,
}

impl DebugUiHandle {
    pub fn push_resource(&self, r: ResourceInfo) { let _ = self.resource_tx.send(r); }
    pub fn push_input(&self, e: InputEvent) { let _ = self.input_tx.send(e); }
    pub fn log(&self, s: impl Into<String>) { let _ = self.log_tx.send(s.into()); }
    pub fn pause(&self) { let _ = self.ctrl_tx.send(ControlMsg::Pause); }
    pub fn resume(&self) { let _ = self.ctrl_tx.send(ControlMsg::Resume); }
    pub fn clear_logs(&self) { let _ = self.ctrl_tx.send(ControlMsg::ClearLogs); }

    pub fn shutdown(mut self) {
        let _ = self.ctrl_tx.send(ControlMsg::Shutdown);
        if let Some(jh) = self.join_handle.take() { let _ = jh.join(); }
    }

    pub fn start_capture(&mut self) -> Result<()> {
        if self.capture.is_some() { return Ok(()); }
        let cap = PlatformCapture::start(self.log_tx.clone())?;
        self.capture = Some(cap);
        Ok(())
    }

    pub fn stop_capture(&mut self) -> Result<()> {
        if let Some(c) = self.capture.take() { c.stop()?; }
        Ok(())
    }
}

impl Clone for DebugUiHandle {
    fn clone(&self) -> Self {
        DebugUiHandle {
            resource_tx: self.resource_tx.clone(),
            input_tx: self.input_tx.clone(),
            log_tx: self.log_tx.clone(),
            ctrl_tx: self.ctrl_tx.clone(),
            join_handle: None,
            capture: None,
        }
    }
}

pub fn start_debug_ui() -> DebugUiHandle {
    let (res_tx, res_rx) = unbounded::<ResourceInfo>();
    let (in_tx, in_rx) = unbounded::<InputEvent>();
    let (log_tx, log_rx) = unbounded::<String>();
    let (ctrl_tx, ctrl_rx) = unbounded::<ControlMsg>();

    let jh = thread::spawn(move || {
        let _ = ui_thread_main(res_rx, in_rx, log_rx, ctrl_rx);
    });

    DebugUiHandle {
        resource_tx: res_tx,
        input_tx: in_tx,
        log_tx,
        ctrl_tx,
        join_handle: Some(jh),
        capture: None,
    }
}

fn ui_thread_main(
    res_rx: Receiver<ResourceInfo>,
    in_rx: Receiver<InputEvent>,
    log_rx: Receiver<String>,
    ctrl_rx: Receiver<ControlMsg>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut resources: HashMap<String, ResourceInfo> = HashMap::new();
    let mut inputs: VecDeque<InputEvent> = VecDeque::with_capacity(200);
    let mut logs: VecDeque<String> = VecDeque::with_capacity(2000);
    let mut log_scroll: usize = 0;
    let mut paused = false;

    loop {
        select! {
            recv(res_rx) -> msg => if let Ok(r) = msg { resources.insert(r.id.clone(), r); },
            recv(in_rx) -> msg => if let Ok(e) = msg {
                if inputs.len() == 200 { inputs.pop_front(); }
                inputs.push_back(e);
            },
            recv(log_rx) -> msg => if let Ok(s) = msg {
                for line in s.split('\n') {
                    if logs.len() == 2000 { logs.pop_front(); }
                    logs.push_back(line.to_string());
                }
            },
            recv(ctrl_rx) -> msg => match msg {
                Ok(ControlMsg::Pause) => paused = true,
                Ok(ControlMsg::Resume) => paused = false,
                Ok(ControlMsg::ClearLogs) => logs.clear(),
                Ok(ControlMsg::Shutdown) => break,
                _ => {}
            },
            default(Duration::from_millis(100)) => {}
        }

        terminal.draw(|f| {
            let size = f.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(30),
                    Constraint::Percentage(30),
                    Constraint::Percentage(40),
                ])
                .split(size);

            let mut items: Vec<ListItem> = resources.values().map(|r| {
                let status = match &r.status {
                    crate::ui::ResourceStatus::Loading => "[Loading]".to_string(),
                    crate::ui::ResourceStatus::Loaded => "[Loaded]".to_string(),
                    crate::ui::ResourceStatus::Error(e) => format!("[Error:{}]", e),
                };
                ListItem::new(format!("{} | {} | {}", r.id, r.rtype, status))
            }).collect();
            let resources_list = List::new(items).block(Block::default().title("Resources").borders(Borders::ALL));
            f.render_widget(resources_list, chunks[0]);

            let in_items: Vec<ListItem> = inputs.iter().rev().take(20)
                .map(|e| ListItem::new(format!("{:?} {} {}", e.ts, e.kind, e.detail))).collect();
            let inputs_list = List::new(in_items).block(Block::default().title("Inputs").borders(Borders::ALL));
            f.render_widget(inputs_list, chunks[1]);

            let logs_vec: Vec<ListItem> = logs.iter().rev().skip(log_scroll)
                .take((chunks[2].height as usize).saturating_sub(2))
                .map(|s| ListItem::new(s.clone())).collect();
            let logs_list = List::new(logs_vec).block(Block::default().title("Logs - q quit").borders(Borders::ALL));
            f.render_widget(logs_list, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(10))? {
            if let CEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Up => { log_scroll = log_scroll.saturating_add(1); }
                    KeyCode::Down => { if log_scroll > 0 { log_scroll -= 1; } }
                    _ => {}
                }
            }
        }

        if paused { /* could freeze updates */ }
    }

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

