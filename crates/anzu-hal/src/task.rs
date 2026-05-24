//! Cooperative task scheduler for audio events (fade-in, fade-out, stop).
//!
//! Tasks run entirely on the host thread; `AudioSystem::tick(delta_ms)` drives them.
//! No OS threads or async runtime needed.

use crate::mixer::SoftMixer;

#[derive(Clone, Copy, Debug)]
pub enum Easing {
    Linear,
}

/// Volume transition from `from` → `to` over `total_ms` milliseconds.
#[derive(Debug)]
pub struct FadeTask {
    pub channel: usize,
    pub from_vol: f32,
    pub to_vol: f32,
    pub elapsed_ms: u32,
    pub total_ms: u32,
    pub easing: Easing,
    /// Stop the channel when the fade reaches `to_vol`.
    pub stop_on_done: bool,
}

impl FadeTask {
    fn t(&self) -> f32 {
        if self.total_ms == 0 {
            return 1.0;
        }
        (self.elapsed_ms as f32 / self.total_ms as f32).clamp(0.0, 1.0)
    }

    fn current_vol(&self) -> f32 {
        let t = self.t();
        match self.easing {
            Easing::Linear => self.from_vol + (self.to_vol - self.from_vol) * t,
        }
    }

    fn is_done(&self) -> bool {
        self.elapsed_ms >= self.total_ms
    }
}

/// Panning transition for a channel.
#[derive(Debug)]
pub struct PanTask {
    pub channel: usize,
    pub from_pan: f32,
    pub to_pan: f32,
    pub elapsed_ms: u32,
    pub total_ms: u32,
}

impl PanTask {
    fn t(&self) -> f32 {
        if self.total_ms == 0 { 1.0 } else {
            (self.elapsed_ms as f32 / self.total_ms as f32).clamp(0.0, 1.0)
        }
    }
    fn current_pan(&self) -> f32 {
        self.from_pan + (self.to_pan - self.from_pan) * self.t()
    }
    fn is_done(&self) -> bool { self.elapsed_ms >= self.total_ms }
}

/// Delayed-stop: stop a channel after `delay_ms` milliseconds.
#[derive(Debug)]
pub struct StopTask {
    pub channel: usize,
    pub elapsed_ms: u32,
    pub delay_ms: u32,
}

impl StopTask {
    fn is_done(&self) -> bool { self.elapsed_ms >= self.delay_ms }
}

#[derive(Default)]
pub struct TaskScheduler {
    fades: Vec<FadeTask>,
    pans: Vec<PanTask>,
    stops: Vec<StopTask>,
}

impl TaskScheduler {
    pub fn cancel_fades_for(&mut self, channel: usize) {
        self.fades.retain(|t| t.channel != channel);
    }

    pub fn cancel_pans_for(&mut self, channel: usize) {
        self.pans.retain(|t| t.channel != channel);
    }

    pub fn cancel_stops_for(&mut self, channel: usize) {
        self.stops.retain(|t| t.channel != channel);
    }

    pub fn schedule_fade(&mut self, task: FadeTask) {
        self.cancel_fades_for(task.channel);
        self.fades.push(task);
    }

    pub fn schedule_pan(&mut self, task: PanTask) {
        self.cancel_pans_for(task.channel);
        self.pans.push(task);
    }

    pub fn schedule_stop(&mut self, task: StopTask) {
        self.cancel_stops_for(task.channel);
        self.stops.push(task);
    }

    /// Advance all tasks by `delta_ms` and apply the resulting state to `mixer`.
    pub fn tick(&mut self, delta_ms: u32, mixer: &mut SoftMixer) {
        // Fades
        for task in &mut self.fades {
            task.elapsed_ms = task.elapsed_ms.saturating_add(delta_ms);
            let vol = task.current_vol();
            mixer.set_channel_volume(task.channel, vol);
            if task.is_done() && task.stop_on_done {
                mixer.stop_channel(task.channel);
            }
        }
        self.fades.retain(|t| !t.is_done());

        // Pans
        for task in &mut self.pans {
            task.elapsed_ms = task.elapsed_ms.saturating_add(delta_ms);
            let pan = task.current_pan();
            mixer.set_channel_pan(task.channel, pan);
        }
        self.pans.retain(|t| !t.is_done());

        // Stops
        for task in &mut self.stops {
            task.elapsed_ms = task.elapsed_ms.saturating_add(delta_ms);
            if task.is_done() {
                mixer.stop_channel(task.channel);
            }
        }
        self.stops.retain(|t| !t.is_done());
    }
}
