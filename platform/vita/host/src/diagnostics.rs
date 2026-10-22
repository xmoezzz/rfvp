//! Bounded, main-thread frame history. File writes happen outside measured frames.
use alloc::{collections::VecDeque, format, string::String, vec::Vec};
use core::fmt::Write;
use rfvp::{perf, RfvpTickResult};

pub const BUILD: &str = "rfvp-psv-diag-20261019-06";
pub const REPORT_EVERY: u64 = 120;
const SLOW_US: u64 = 50_000;

extern "C" {
    fn sceKernelGetProcessTimeWide() -> u64;
    fn rfvp_psv_audio_diag(out: *mut i32);
}

pub fn clock_us() -> u64 {
    unsafe { sceKernelGetProcessTimeWide() }
}

#[derive(Clone)]
struct Frame {
    index: u64,
    at_us: u64,
    total_us: u64,
    vm_us: u64,
    scene_us: u64,
    render_us: u64,
    present_us: u64,
    draws: u64,
    ops: u32,
    stages: [u32; perf::STAGES.len()],
}

impl Frame {
    fn write(&self, out: &mut String) {
        let _ = writeln!(out, "frame={} t={}us total={:.3}ms vm={:.3} scene={:.3} render={:.3} present={:.3} draws={} ops={}",
            self.index, self.at_us, self.total_us as f64 / 1000.0,
            self.vm_us as f64 / 1000.0, self.scene_us as f64 / 1000.0,
            self.render_us as f64 / 1000.0, self.present_us as f64 / 1000.0, self.draws, self.ops);
        for (name, &us) in perf::STAGES.iter().zip(&self.stages) {
            if us != 0 { let _ = write!(out, " {name}={:.3}ms", us as f64 / 1000.0); }
        }
        out.push('\n');
    }
}

pub struct Diagnostics {
    header: String,
    top: Vec<Frame>,
    recent: VecDeque<Frame>,
    slow_count: u64,
    frames: u64,
    window_frames: u64,
    window_us: u64,
    totals: [u64; perf::STAGES.len()],
    pub last_write_us: u64,
}

impl Diagnostics {
    pub fn new(header: String) -> Self {
        Self { header, top: Vec::with_capacity(8), recent: VecDeque::with_capacity(12),
            slow_count: 0, frames: 0, window_frames: 0, window_us: 0,
            totals: [0; perf::STAGES.len()], last_write_us: 0 }
    }

    pub fn record(&mut self, tick: RfvpTickResult, started: u64, ended: u64, present_us: u64, draws: u64) {
        let frame = Frame { index: tick.frame_index, at_us: ended,
            total_us: ended.saturating_sub(started), vm_us: tick.vm_us,
            scene_us: tick.scene_us, render_us: tick.render_us, present_us, draws,
            ops: perf::take_ops(), stages: perf::take() };
        self.frames += 1;
        self.window_frames += 1;
        self.window_us += frame.total_us;
        for (total, &us) in self.totals.iter_mut().zip(&frame.stages) { *total += us as u64; }
        if frame.total_us >= SLOW_US {
            self.slow_count += 1;
            if self.recent.len() == 12 { self.recent.pop_front(); }
            self.recent.push_back(frame.clone());
        }
        if self.top.len() < 8 || self.top.last().is_some_and(|last| frame.total_us > last.total_us) {
            self.top.push(frame);
            self.top.sort_by(|a, b| b.total_us.cmp(&a.total_us));
            self.top.truncate(8);
        }
    }

    pub fn report(&mut self) -> String {
        let count = self.window_frames.max(1) as f64;
        let mut out = format!("build={BUILD}\n{}\nframes={} slow_frames_over_50ms={}\nwindow_frames={} measured_frame_avg_ms={:.3} previous_report_write_ms={:.3}\n\
            Times are inclusive wall times (nested stages overlap; audio can preempt main).\n\
            Frame times exclude diagnostic formatting/file writes; writes can still disturb play.\n\
            Core render_us also contains audio command/save work; see audio_cmds/save stages.\n",
            self.header, self.frames, self.slow_count, self.window_frames,
            self.window_us as f64 / 1000.0 / count, self.last_write_us as f64 / 1000.0);
        out.push_str("window stage averages:\n");
        for (name, &us) in perf::STAGES.iter().zip(&self.totals) {
            if us != 0 { let _ = writeln!(out, " {name}={:.3}ms", us as f64 / 1000.0 / count); }
        }
        out.push_str("\nslowest 8 frames since launch:\n");
        for frame in &self.top { frame.write(&mut out); }
        out.push_str("\nmost recent 12 frames over 50ms:\n");
        for frame in &self.recent { frame.write(&mut out); }
        let audio = rfvp_psv::audio::diagnostic_stats();
        let mut hardware = [0i32; 7];
        unsafe { rfvp_psv_audio_diag(hardware.as_mut_ptr()) };
        let _ = writeln!(out, "\naudio cumulative (approximate, concurrent):\n pulls={} nonzero_buffers={} mix_errors={} load_errors={} play_errors={} last_mix_error={} max_pull_us={} max_audio_lock_wait_us={}\n hardware: thread_id={} start_rc={} port={} volume_rc={} output_errors={} last_output_error={} output_calls={}\n hardware -2147483648 means not attempted; negative rc/port indicates failure.\n mix error codes: 1=Io 2=NotFound 3=InvalidData 4=InvalidArgument 5=Unsupported 6=OutOfMemory 7=CapacityExceeded 8=EndOfFile 9=Backend\n nonzero_buffers describes the combined mix, not proof of voice playback.\n",
            audio[0], audio[1], audio[2], audio[3], audio[4], audio[5], audio[6], audio[7],
            hardware[0], hardware[1], hardware[2], hardware[3], hardware[4], hardware[5], hardware[6]);
        out.push_str("recent engine/resource/audio events (diag timestamps use same clock as frames):\n");
        out.push_str(&rfvp::ring_log::dump());
        self.window_frames = 0;
        self.window_us = 0;
        self.totals.fill(0);
        out
    }
}
