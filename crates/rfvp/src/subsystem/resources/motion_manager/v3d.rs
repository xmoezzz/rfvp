use anyhow::Result;
use crate::subsystem::resources::prim::PrimManager;

#[derive(Debug, Clone, PartialEq)]
pub enum V3dMotionType {
    None = 0,
    Linear,
    Accelerate,
    Decelerate,
    Rebound,
    Bounce,
}

impl TryFrom<i32> for V3dMotionType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(V3dMotionType::None),
            1 => Ok(V3dMotionType::Linear),
            2 => Ok(V3dMotionType::Accelerate),
            3 => Ok(V3dMotionType::Decelerate),
            4 => Ok(V3dMotionType::Rebound),
            5 => Ok(V3dMotionType::Bounce),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct V3dMotion {
    reverse: bool,
    src_x: i32,
    dst_x: i32,
    src_y: i32,
    dst_y: i32,
    src_z: i32,
    dst_z: i32,
    duration: i32,
    elapsed: i32,
    typ: V3dMotionType,
    paused: bool,
}

impl V3dMotion {
    pub fn new() -> Self {
        Self {
            reverse: false,
            src_x: 0,
            dst_x: 0,
            src_y: 0,
            dst_y: 0,
            src_z: 0,
            dst_z: 0,
            duration: 0,
            elapsed: 0,
            typ: V3dMotionType::None,
            paused: false,
        }
    }

    pub fn get_src_x(&self) -> i32 {
        self.src_x
    }

    pub fn get_dst_x(&self) -> i32 {
        self.dst_x
    }

    pub fn get_src_y(&self) -> i32 {
        self.src_y
    }

    pub fn get_dst_y(&self) -> i32 {
        self.dst_y
    }

    pub fn get_src_z(&self) -> i32 {
        self.src_z
    }

    pub fn get_dst_z(&self) -> i32 {
        self.dst_z
    }

    pub fn get_duration(&self) -> i32 {
        self.duration
    }

    pub fn get_elapsed(&self) -> i32 {
        self.elapsed
    }

    pub fn get_type(&self) -> V3dMotionType {
        self.typ.clone()
    }

    pub fn get_reverse(&self) -> bool {
        self.reverse
    }

    pub fn get_paused(&self) -> bool {
        self.paused
    }

    pub fn set_src_x(&mut self, src_x: i32) {
        self.src_x = src_x;
    }

    pub fn set_dst_x(&mut self, dst_x: i32) {
        self.dst_x = dst_x;
    }

    pub fn set_src_y(&mut self, src_y: i32) {
        self.src_y = src_y;
    }

    pub fn set_dst_y(&mut self, dst_y: i32) {
        self.dst_y = dst_y;
    }

    pub fn set_src_z(&mut self, src_z: i32) {
        self.src_z = src_z;
    }

    pub fn set_dst_z(&mut self, dst_z: i32) {
        self.dst_z = dst_z;
    }

    pub fn set_duration(&mut self, duration: i32) {
        self.duration = duration;
    }

    pub fn set_elapsed(&mut self, elapsed: i32) {
        self.elapsed = elapsed;
    }

    pub fn set_type(&mut self, typ: V3dMotionType) {
        self.typ = typ;
    }

    pub fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
}

#[derive(Debug, Clone)]
pub struct V3dMotionContainer {
    motion: V3dMotion,
    current_x: i32,
    current_y: i32,
    current_z: i32,
}

impl V3dMotionContainer {
    pub fn new() -> Self {
        Self {
            motion: V3dMotion::new(),
            current_x: 0,
            current_y: 0,
            current_z: 0,
        }
    }

    pub fn set_motion(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        dst_z: i32,
        duration: i32,
        typ: V3dMotionType,
        reverse: bool,
    ) -> Result<()> {
        if self.motion.get_type() != V3dMotionType::None {
            self.motion.set_type(V3dMotionType::None);
            self.current_x = self.motion.get_dst_x();
            self.current_y = self.motion.get_dst_y();
            self.current_z = self.motion.get_dst_z();
        }

        self.motion.set_src_x(self.current_x);
        self.motion.set_dst_x(dst_x);
        self.motion.set_src_y(self.current_y);
        self.motion.set_dst_y(dst_y);
        self.motion.set_src_z(self.current_z);
        self.motion.set_dst_z(dst_z);
        self.motion.set_duration(duration);
        self.motion.set_type(typ);
        self.motion.set_reverse(reverse);
        self.motion.set_elapsed(0);

        Ok(())
    }

    pub fn stop_motion(&mut self) -> Result<()> {
        if self.motion.get_type() != V3dMotionType::None {
            self.motion.set_type(V3dMotionType::None);
            self.current_x = self.motion.get_dst_x();
            self.current_y = self.motion.get_dst_y();
            self.current_z = self.motion.get_dst_z();
        }

        Ok(())
    }

    pub fn test_motion(&self) -> bool {
        self.motion.get_type() != V3dMotionType::None
    }

    pub fn set_v3d(&mut self, x: i32, y: i32, z: i32) {
        self.current_x = x;
        self.current_y = y;
        self.current_z = z;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.motion.set_paused(paused);
    }

    pub fn get_paused(&self) -> bool {
        self.motion.get_paused()
    }

    pub fn get_x(&self) -> i32 {
        self.current_x
    }

    pub fn get_y(&self) -> i32 {
        self.current_y
    }

    pub fn get_z(&self) -> i32 {
        self.current_z
    }

    pub fn exec_v3d_update(
        &mut self,
        prim_manager: &PrimManager,
        flag: bool,
        elapsed: i32,
    ) -> bool {
        if self.motion.typ != V3dMotionType::None && !self.motion.paused && !flag {
            // Mark all V3D-enabled prims dirty (attr |= 0x40).
            // The prim array is fixed-size (4096 slots).
            for idx in 1i16..4096i16 {
                let mut prim = prim_manager.get_prim(idx);
                let attr = prim.get_attr();
                if (attr & 4) != 0 {
                    prim.set_attr(attr | 0x40);
                }
            }

            let mut elapsed = elapsed;
            if self.motion.reverse && elapsed < 0 {
                elapsed = -elapsed;
            }
            self.motion.elapsed += elapsed;
            if self.motion.elapsed >= self.motion.duration {
                let _ = self.stop_motion();
                return false;
            }

            let delta_x = self.motion.dst_x as i64 - self.motion.src_x as i64;
            let delta_y = self.motion.dst_y as i64 - self.motion.src_y as i64;
            let delta_z = self.motion.dst_z as i64 - self.motion.src_z as i64;
            match self.motion.get_type() {
                V3dMotionType::Linear => {
                    let x = self.motion.src_x as i64
                        + delta_x * self.motion.elapsed as i64 / self.motion.duration as i64;
                    let y = self.motion.src_y as i64
                        + delta_y * self.motion.elapsed as i64 / self.motion.duration as i64;
                    let z = self.motion.src_z as i64
                        + delta_z * self.motion.elapsed as i64 / self.motion.duration as i64;

                    self.set_v3d(x as i32, y as i32, z as i32);
                }
                V3dMotionType::Accelerate => {
                    let square_elapsed = self.motion.elapsed as i64 * self.motion.elapsed as i64;
                    let square_duration = self.motion.duration as i64 * self.motion.duration as i64;

                    let x = self.motion.src_x as i64 + delta_x * square_elapsed / square_duration;
                    let y = self.motion.src_y as i64 + delta_y * square_elapsed / square_duration;
                    let z = self.motion.src_z as i64 + delta_z * square_elapsed / square_duration;

                    self.set_v3d(x as i32, y as i32, z as i32);
                }
                V3dMotionType::Decelerate => {
                    let time2 = (self.motion.duration as i64 - self.motion.elapsed as i64)
                        * (self.motion.duration as i64 - self.motion.elapsed as i64);
                    let square_duration = self.motion.duration as i64 * self.motion.duration as i64;

                    let x = self.motion.dst_x as i64 - delta_x * time2 / square_duration;
                    let y = self.motion.dst_y as i64 - delta_y * time2 / square_duration;
                    let z = self.motion.dst_z as i64 - delta_z * time2 / square_duration;

                    self.set_v3d(x as i32, y as i32, z as i32);
                }
                V3dMotionType::Rebound => {
                    let half_duration = self.motion.duration as i64 / 2;
                    let half_delta_x = delta_x / 2;
                    let half_delta_y = delta_y / 2;
                    let half_delta_z = delta_z / 2;

                    if self.motion.elapsed as i64 > half_duration {
                        let remain = self.motion.duration as i64 - self.motion.elapsed as i64;
                        let time2 = self.motion.duration as i64 - self.motion.duration as i64 / 2;

                        let x = self.motion.dst_x as i64 - (delta_x - half_delta_x) * (remain * remain) / (time2 * time2);
                        let y = self.motion.dst_y as i64 - (delta_y - half_delta_y) * (remain * remain) / (time2 * time2);
                        let z = self.motion.dst_z as i64 - (delta_z - half_delta_z) * (remain * remain) / (time2 * time2);

                        self.set_v3d(x as i32, y as i32, z as i32);
                    }
                    else {
                        let square_elapsed = self.motion.elapsed as i64 * self.motion.elapsed as i64;

                        let x = self.motion.src_x as i64 + half_delta_x * square_elapsed / (half_duration * half_duration);
                        let y = self.motion.src_y as i64 + half_delta_y * square_elapsed / (half_duration * half_duration);
                        let z = self.motion.src_z as i64 + half_delta_z * square_elapsed / (half_duration * half_duration);

                        self.set_v3d(x as i32, y as i32, z as i32);
                    }
                }
                V3dMotionType::Bounce => {
                    let half_duration = self.motion.duration as i64 / 2;
                    let half_delta_x = delta_x / 2;
                    let half_delta_y = delta_y / 2;
                    let half_delta_z = delta_z / 2;

                    if self.motion.elapsed as i64 > half_duration {

                        let x = half_delta_x
                            + self.motion.src_x as i64
                            + (delta_x - half_delta_x)
                            * (self.motion.elapsed as i64 - half_duration)
                            * (self.motion.elapsed as i64 - half_duration)
                            / (self.motion.duration as i64 - half_duration)
                            / (self.motion.duration as i64 - half_duration);

                        let y = half_delta_y
                            + self.motion.src_y as i64
                            + (delta_y - half_delta_y)
                            * (self.motion.elapsed as i64 - half_duration)
                            * (self.motion.elapsed as i64 - half_duration)
                            / (self.motion.duration as i64 - half_duration)
                            / (self.motion.duration as i64 - half_duration);

                        let z = half_delta_z
                            + self.motion.src_z as i64
                            + (delta_z - half_delta_z)
                            * (self.motion.elapsed as i64 - half_duration)
                            * (self.motion.elapsed as i64 - half_duration)
                            / (self.motion.duration as i64 - half_duration)
                            / (self.motion.duration as i64 - half_duration);

                        self.set_v3d(x as i32, y as i32, z as i32);
                    }
                    else {
                        let x = half_delta_x + self.motion.src_x as i64
                            - half_delta_x
                            * (half_duration - self.motion.elapsed as i64)
                            * (half_duration - self.motion.elapsed as i64)
                            / half_duration
                            / half_duration;

                        let y = half_delta_y + self.motion.src_y as i64
                            - half_delta_y
                            * (half_duration - self.motion.elapsed as i64)
                            * (half_duration - self.motion.elapsed as i64)
                            / half_duration
                            / half_duration;

                        let z = half_delta_z + self.motion.src_z as i64
                            - half_delta_z
                            * (half_duration - self.motion.elapsed as i64)
                            * (half_duration - self.motion.elapsed as i64)
                            / half_duration
                            / half_duration;

                        self.set_v3d(x as i32, y as i32, z as i32);
                    }
                }
                _ => {
                    let _ = self.stop_motion();
                }
            };
        }
        true
    }
}
