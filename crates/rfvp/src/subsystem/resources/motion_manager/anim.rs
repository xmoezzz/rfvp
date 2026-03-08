use anyhow::{bail, Result};

use crate::subsystem::resources::prim::{PrimManager, INVAILD_PRIM_HANDLE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpriteAnimMode {
    Loop,
    Once,
}

#[derive(Debug, Clone)]
struct SpriteAnimStep {
    sprt_prim_id: i16,
    time_ms: i32,
}

#[derive(Debug, Clone)]
struct SpriteAnim {
    prim_id: u32,
    mode: SpriteAnimMode,
    steps: Vec<SpriteAnimStep>,
    current: usize,
    time_left_ms: i32,
    running: bool,
}

impl SpriteAnim {
    fn new(prim_id: u32, first_sprt_prim_id: i16, time_ms: i32, mode: SpriteAnimMode) -> Self {
        Self {
            prim_id,
            mode,
            steps: vec![SpriteAnimStep { sprt_prim_id: first_sprt_prim_id, time_ms }],
            current: 0,
            time_left_ms: time_ms,
            running: true,
        }
    }

    fn append_step(&mut self, sprt_prim_id: i16, time_ms: i32) {
        self.steps.push(SpriteAnimStep { sprt_prim_id, time_ms });
    }

    fn current_sprt(&self) -> i16 {
        self.steps[self.current].sprt_prim_id
    }

    fn last_sprt(&self) -> i16 {
        self.steps.last().map(|s| s.sprt_prim_id).unwrap_or(INVAILD_PRIM_HANDLE)
    }
}

#[derive(Debug, Default)]
pub struct SpriteAnimContainer {
    anims: Vec<SpriteAnim>,
}

impl SpriteAnimContainer {
    pub fn new() -> Self {
        Self { anims: vec![] }
    }

    pub fn set_motion(&mut self, prim_id: u32, sprt_prim_id: i32, time_ms: i32, typ: i32) -> Result<()> {
        if prim_id as i32 == INVAILD_PRIM_HANDLE as i32 {
            return Ok(());
        }
        if !(1..=4095).contains(&(prim_id as i32)) {
            bail!("MotionAnim: invalid prim_id {}", prim_id);
        }
        if !(1..=4095).contains(&sprt_prim_id) {
            bail!("MotionAnim: invalid sprt prim id {}", sprt_prim_id);
        }
        if !(1..=300000).contains(&time_ms) {
            bail!("MotionAnim: invalid time {}", time_ms);
        }

        let mode = if typ == 1 {
            SpriteAnimMode::Loop
        } else {
            SpriteAnimMode::Once
        };

        self.anims.retain(|a| a.prim_id != prim_id);
        self.anims.push(SpriteAnim::new(prim_id, sprt_prim_id as i16, time_ms, mode));
        Ok(())
    }

    pub fn append_motion(&mut self, prim_id: u32, sprt_prim_id: i32, time_ms: i32) -> Result<()> {
        if !(1..=4095).contains(&(prim_id as i32)) {
            bail!("MotionAnim: invalid prim_id {}", prim_id);
        }
        if !(1..=4095).contains(&sprt_prim_id) {
            bail!("MotionAnim: invalid sprt prim id {}", sprt_prim_id);
        }
        if !(1..=300000).contains(&time_ms) {
            bail!("MotionAnim: invalid time {}", time_ms);
        }

        if let Some(anim) = self.anims.iter_mut().find(|a| a.prim_id == prim_id && a.running) {
            anim.append_step(sprt_prim_id as i16, time_ms);
        }
        Ok(())
    }

    pub fn stop_motion(&mut self, prim_id: u32) -> Result<()> {
        self.anims.retain(|a| a.prim_id != prim_id);
        Ok(())
    }

    pub fn test_motion(&self, prim_id: u32) -> bool {
        self.anims.iter().any(|a| a.prim_id == prim_id && a.running)
    }

    fn is_paused_by_ancestors(prim_manager: &PrimManager, prim_id: i16) -> bool {
        let mut cur = prim_id;
        loop {
            let prim = prim_manager.get_prim_immutable(cur);
            if prim.get_paused() {
                return true;
            }
            let parent = prim.get_parent();
            drop(prim);
            if parent == INVAILD_PRIM_HANDLE {
                return false;
            }
            cur = parent;
        }
    }

    pub fn update(&mut self, prim_manager: &PrimManager, elapsed_ms: i32) {
        if elapsed_ms == 0 {
            return;
        }

        let delta_ms = elapsed_ms.abs();
        let mut finished: Vec<u32> = Vec::new();

        for anim in self.anims.iter_mut() {
            if !anim.running || anim.steps.is_empty() {
                continue;
            }

            let prim_id = anim.prim_id as i16;
            if Self::is_paused_by_ancestors(prim_manager, prim_id) {
                continue;
            }

            {
                let mut prim = prim_manager.get_prim(prim_id);
                prim.apply_attr(0x40);
            }

            let mut remaining = delta_ms;
            while remaining >= anim.time_left_ms {
                remaining -= anim.time_left_ms;
                let next = anim.current + 1;
                if next < anim.steps.len() {
                    anim.current = next;
                    anim.time_left_ms = anim.steps[anim.current].time_ms;
                } else {
                    match anim.mode {
                        SpriteAnimMode::Loop => {
                            anim.current = 0;
                            anim.time_left_ms = anim.steps[0].time_ms;
                        }
                        SpriteAnimMode::Once => {
                            let mut prim = prim_manager.get_prim(prim_id);
                            prim.set_sprt(anim.last_sprt());
                            anim.running = false;
                            finished.push(anim.prim_id);
                            break;
                        }
                    }
                }
            }

            if !anim.running {
                continue;
            }

            anim.time_left_ms -= remaining;
            let mut prim = prim_manager.get_prim(prim_id);
            prim.set_sprt(anim.current_sprt());
        }

        if !finished.is_empty() {
            self.anims.retain(|a| !finished.contains(&a.prim_id));
        }
    }
}

impl SpriteAnimContainer {
    pub fn debug_dump(&self, max: usize) -> String {
        let mut out = String::new();
        let mut n = 0usize;
        for a in &self.anims {
            if !a.running {
                continue;
            }
            if n >= max {
                break;
            }
            out.push_str(&format!(
                "  [anim] prim={} mode={:?} current={} time_left_ms={} steps={:?}
",
                a.prim_id,
                a.mode,
                a.current,
                a.time_left_ms,
                a.steps
                    .iter()
                    .map(|s| (s.sprt_prim_id, s.time_ms))
                    .collect::<Vec<_>>()
            ));
            n += 1;
        }
        out
    }

    pub fn debug_running_count(&self) -> usize {
        self.anims.iter().filter(|a| a.running).count()
    }
}
