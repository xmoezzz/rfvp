//! Thread-safe conversion progress shared by CLI and future GUI clients.
use anyhow::{Result, anyhow};
use serde::Serialize;
use std::sync::{Arc, Mutex};

/// Reject aliased or overlapping source/destination trees before touching files.
pub fn validate_paths(input: &std::path::Path, output: &std::path::Path) -> Result<()> {
    let input = input.canonicalize()?;
    if !input.is_dir() {
        return Err(anyhow!("input must be a game directory"));
    }
    let absolute = std::path::absolute(output)?;
    let mut existing = absolute.as_path();
    let mut suffix = Vec::new();
    while !existing.exists() {
        suffix.push(
            existing
                .file_name()
                .ok_or_else(|| anyhow!("invalid output path"))?
                .to_owned(),
        );
        existing = existing
            .parent()
            .ok_or_else(|| anyhow!("invalid output parent"))?;
    }
    let mut output = existing.canonicalize()?;
    for part in suffix.into_iter().rev() {
        output.push(part);
    }
    if input.starts_with(&output) || output.starts_with(&input) {
        return Err(anyhow!(
            "input and output must be separate, non-overlapping directories"
        ));
    }
    if output.exists() && !output.is_dir() {
        return Err(anyhow!("output is not a directory"));
    }
    if output.is_dir() {
        let mut pending = vec![output];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    return Err(anyhow!(
                        "output contains a symlink: {}",
                        entry.path().display()
                    ));
                }
                if kind.is_dir() {
                    pending.push(entry.path());
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct ProgressEvent {
    pub kind: &'static str,
    pub stage: String,
    pub completed: usize,
    pub total: usize,
    pub resource: Option<String>,
    pub message: Option<String>,
}

type Observer = Box<dyn FnMut(&ProgressEvent) -> Result<()> + Send>;
struct State {
    event: ProgressEvent,
    observer: Observer,
    stopped: bool,
}

#[derive(Clone)]
pub struct ConversionContext(Arc<Mutex<State>>);

impl Default for ConversionContext {
    fn default() -> Self {
        Self::new(|_| Ok(()))
    }
}

impl ConversionContext {
    /// Returning an error from the observer cancels at the next resource boundary.
    /// Callbacks are serialized even when converters use multiple worker threads.
    pub fn new(observer: impl FnMut(&ProgressEvent) -> Result<()> + Send + 'static) -> Self {
        Self(Arc::new(Mutex::new(State {
            event: ProgressEvent {
                kind: "started",
                stage: String::new(),
                completed: 0,
                total: 0,
                resource: None,
                message: None,
            },
            observer: Box::new(observer),
            stopped: false,
        })))
    }

    pub fn human() -> Self {
        Self::new(|event| {
            if let Some(message) = &event.message {
                eprintln!("{message}");
            } else if event.kind == "started" {
                eprintln!("{}: {} resources", event.stage, event.total);
            } else if event.kind == "resource"
                && (event.completed % 200 == 0 || event.completed == event.total)
            {
                eprintln!("{}: {}/{}", event.stage, event.completed, event.total);
            }
            Ok(())
        })
    }

    pub fn check(&self) -> Result<()> {
        let state = self
            .0
            .lock()
            .map_err(|_| anyhow!("conversion observer failed"))?;
        if state.stopped {
            return Err(anyhow!("conversion cancelled by observer"));
        }
        Ok(())
    }

    fn emit(&self, change: impl FnOnce(&mut ProgressEvent)) -> Result<()> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| anyhow!("conversion observer failed"))?;
        if state.stopped {
            return Err(anyhow!("conversion cancelled by observer"));
        }
        change(&mut state.event);
        let event = state.event.clone();
        if let Err(error) = (state.observer)(&event) {
            state.stopped = true;
            return Err(error);
        }
        Ok(())
    }

    pub fn start(&self, stage: impl Into<String>, total: usize) -> Result<()> {
        let stage = stage.into();
        self.emit(|event| {
            event.kind = "started";
            event.stage = stage;
            event.completed = 0;
            event.total = total;
            event.resource = None;
            event.message = None;
        })
    }

    pub fn advance(&self, resource: impl Into<String>) -> Result<()> {
        let resource = resource.into();
        self.emit(|event| {
            event.kind = "resource";
            event.completed += 1;
            event.resource = Some(resource);
            event.message = None;
        })
    }

    pub fn message(&self, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        self.emit(|event| {
            event.kind = "message";
            event.message = Some(message);
            event.resource = None;
        })
    }

    pub fn finish(&self) -> Result<()> {
        self.emit(|event| {
            event.kind = "finished";
            event.resource = None;
            event.message = None;
        })
    }
}
