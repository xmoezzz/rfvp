use anyhow::{bail, Result};

use crate::subsystem::resources::prim::{PrimManager, INVAILD_PRIM_HANDLE};

#[derive(Debug, Clone)]
struct SpriteAnim {
    prim_id: u32,
    base_graph_id: i32,
    start: i32,
    end: i32,
    current: i32,
    elapsed_ms: i32,
    running: bool,
}

impl SpriteAnim {
    fn new(prim_id: u32, base_graph_id: i32, start: i32, end: i32) -> Self {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };
        Self {
            prim_id,
            base_graph_id,
            start,
            end,
            current: start,
            elapsed_ms: 0,
            running: true,
        }
    }

    fn current_texture_id(&self) -> i16 {
        (self.base_graph_id + self.current) as i16
    }
}

#[derive(Debug, Default)]
pub struct SpriteAnimContainer {
    anims: Vec<SpriteAnim>,
    /// Fixed frame interval in milliseconds (baseline).
    frame_interval_ms: i32,
}

impl SpriteAnimContainer {
    pub fn new() -> Self {
        Self { anims: vec![], frame_interval_ms: 100 }
    }

    pub fn set_motion(&mut self, prim_id: u32, base_graph_id: i32, start: i32, end: i32) -> Result<()> {
        if prim_id as i32 == INVAILD_PRIM_HANDLE as i32 {
            return Ok(());
        }
        if !(0..=4095).contains(&(prim_id as i32)) {
            bail!("MotionAnim: invalid prim_id {}", prim_id);
        }
        self.anims.retain(|a| a.prim_id != prim_id);
        self.anims.push(SpriteAnim::new(prim_id, base_graph_id, start, end));
        Ok(())
    }

    pub fn stop_motion(&mut self, prim_id: u32) -> Result<()> {
        self.anims.retain(|a| a.prim_id != prim_id);
        Ok(())
    }

    pub fn test_motion(&self, prim_id: u32) -> bool {
        self.anims.iter().any(|a| a.prim_id == prim_id && a.running)
    }

    pub fn update(&mut self, prim_manager: &PrimManager, elapsed_ms: i32) {
        if elapsed_ms <= 0 {
            return;
        }

        for anim in self.anims.iter_mut() {
            if !anim.running {
                continue;
            }

            let mut prim = prim_manager.get_prim(anim.prim_id as i16);

            // Match other motion containers: a paused prim freezes its motion state.
            if prim.get_paused() {
                continue;
            }
            prim.apply_attr(0x40);

            anim.elapsed_ms = anim.elapsed_ms.saturating_add(elapsed_ms);

            while anim.elapsed_ms >= self.frame_interval_ms {
                anim.elapsed_ms -= self.frame_interval_ms;

                let next = anim.current + 1;
                if next > anim.end {
                    anim.current = anim.start;
                } else {
                    anim.current = next;
                }
            }

            prim.set_texture_id(anim.current_texture_id());
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
                "  [anim] prim={} base_graph={} range=[{},{}] cur={} elapsed_ms={} frame_interval_ms={}\n",
                a.prim_id,
                a.base_graph_id,
                a.start,
                a.end,
                a.current,
                a.elapsed_ms,
                self.frame_interval_ms
            ));
            n += 1;
        }
        out
    }

    pub fn debug_running_count(&self) -> usize {
        self.anims.iter().filter(|a| a.running).count()
    }
}

