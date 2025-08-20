

use rand::Rng;

#[derive(Debug, Clone, Copy)]
pub struct SnowFlake {
    unk1: i32,
    speed: f32,
    x_pos: f32,
    y_pos: f32,
}

impl SnowFlake {
    pub fn new() -> Self {
        SnowFlake {
            unk1: 0,
            speed: 0.0,
            x_pos: 0.0,
            y_pos: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnowMotion {
    flakes: Vec<SnowFlake>,
    running: bool,
    width: u32,
    height: u32,
    arg3: i32,
    start_y: i32,
    speed: i32,
    x_variation: i32,
    start_x: i32,
    end_x: i32,
    arg9: i32,
    item_count: u32,
    y_offset: i32,
    x_offset: i32,
    horizontal_speed: i32,
    arg14: i32,
    arg15: i32,
    arg16: i32,
    arg17: i32,
}

impl SnowMotion {
    pub fn new() -> Self {
        SnowMotion {
            flakes: vec![],
            running: false,
            width: 0,
            height: 0,
            arg3: 0,
            start_y: 0,
            speed: 0,
            x_variation: 0,
            start_x: 0,
            end_x: 0,
            arg9: 0,
            item_count: 0,
            y_offset: 0,
            x_offset: 0,
            horizontal_speed: 0,
            arg14: 0,
            arg15: 0,
            arg16: 0,
            arg17: 0,
        }
    }

    fn initialize_snow_motion_parameters(
        &self,
        screen_width: u32,
        screen_height: u32,
    ) -> SnowFlake {
        let width = if self.width != 0 {
            self.width
        } else {
            screen_width
        } as f64;
        let height = if self.height != 0 {
            self.height
        } else {
            screen_height
        } as f32;
        let x_start = self.start_x as f32;
        let x_range = (self.end_x - self.start_x) as i32;
        let x_movement: f32;
        let mut rng = rand::thread_rng();

        if x_range != 0 {
            let x_step = x_start + (rng.gen::<i32>() % x_range) as f32;
            x_movement = (rng.gen::<i32>() % 256) as f32 * 0.00390625 + x_step;
        } else {
            x_movement = x_start;
        }

        let y_step = 1000.0 / x_movement as f64;
        let v21 = self.start_y as f64 * 0.5 * y_step;
        let v14 = 0.0 - v21;
        let v16 = rng.gen::<f32>();
        let v22 = self.speed as f64 * 0.5 * y_step;
        let x_step_b = 0.0 - v22;
        let v18 = rng.gen::<f32>();
        let v7 = rng.gen::<i32>() % self.x_variation;

        let a2a = width + v21;
        let a2b = a2a - v14;
        let unk1 = v7;
        let x_pos = (v16 % a2b as f32) + v14 as f32;
        let a2d = height + v22 as f32;
        let a2e = a2d - x_step_b as f32;
        let y_pos = (v18 % a2e) + x_step_b as f32;
        let speed = x_movement;

        SnowFlake {
            unk1,
            speed,
            x_pos,
            y_pos,
        }
    }

    fn initialize_snow_motion_parameters2(
        width: f32,
        height: f32,
        start_x: i32,
        end_x: i32,
        start_y: i32,
        speed: i32,
        x_variation: i32,
    ) -> SnowFlake {
        let x_start = start_x as f32;
        let x_range = (end_x - start_x) as i32;
        let x_movement: f32;
        let mut rng = rand::thread_rng();

        if x_range != 0 {
            let x_step = x_start + (rng.gen::<i32>() % x_range) as f32;
            x_movement = (rng.gen::<i32>() % 256) as f32 * 0.00390625 + x_step;
        } else {
            x_movement = x_start;
        }

        let y_step = 1000.0 / x_movement as f64;
        let v21 = start_y as f64 * 0.5 * y_step;
        let v14 = 0.0 - v21;
        let v16 = rng.gen::<f32>();
        let v22 = speed as f64 * 0.5 * y_step;
        let x_step_b = 0.0 - v22;
        let v18 = rng.gen::<f32>();
        let v7 = rng.gen::<i32>() % x_variation;

        let a2a = width + v21 as f32;
        let a2b = a2a - v14 as f32;
        let unk1 = v7;
        let x_pos = (v16 % a2b as f32) + v14 as f32;
        let a2d = height + v22 as f32;
        let a2e = a2d - x_step_b as f32;
        let y_pos = (v18 % a2e) + x_step_b as f32;
        let speed = x_movement;

        SnowFlake {
            unk1,
            speed,
            x_pos,
            y_pos,
        }
    }

    fn snow_flake_sort(&mut self) {
        self.flakes.sort_by(|a, b| {
            let res = a.speed.partial_cmp(&b.speed);
            if let Some(res) = res {
                res
            } else {
                std::cmp::Ordering::Equal
            }
        });
    }

    fn set_snow_item(item: &mut SnowFlake, speed: f32, width: f32, height: f32) {
        if speed != 0.0 {
            let center_x = width * 0.5;
            let center_y = height * 0.5;

            let speed_ratio = 1000.0 / item.speed as f64 / (1000.0 / (speed + item.speed) as f64);

            item.speed += speed;

            item.x_pos =
                ((item.x_pos as f64 - center_x as f64) * speed_ratio + center_x as f64) as f32;
            item.y_pos =
                ((item.y_pos as f64 - center_y as f64) * speed_ratio + center_y as f64) as f32;
        }
    }

    fn update_snow_item(&self, a2: &mut SnowFlake, width: f32, height: f32) {
        if a2.speed != 0.0
            && (self.x_offset != 0 || self.y_offset != 0 || self.horizontal_speed != 0)
        {
            let v4 = width;
            let a2a = height;

            let v10 = (self.start_y as f32) * 0.5;
            let v9 = 0.5 * (self.speed as f32);
            let v16 = self.start_x as f32;
            let v17 = self.end_x as f32;
            let a3 = -(self.horizontal_speed as f32) * 13.0 / 1000.0;

            Self::set_snow_item(a2, a3, width, height);

            let a2b = 1000.0 / a2.speed;
            let v5 = a2b;
            let a2c = -(self.y_offset as f32) * a2b * 13.0 / 1000.0;
            let mut v6 = v5;
            let v12 = 13.0 * (-(self.x_offset as f32) * v5) / 1000.0;
            a2.x_pos += v12;
            a2.y_pos += a2c;

            let v13 = -v10;
            while v13 * v5 <= a2.x_pos {
                let v7 = v9;
                if v6 * v10 + v4 < a2.x_pos {
                    break;
                }
                if -v7 * v6 > a2.y_pos {
                    break;
                }
                if v7 * v6 + a2a < a2.y_pos {
                    break;
                }
                if v16 > a2.speed {
                    break;
                }
                if v17 < a2.speed {
                    break;
                }
                Self::set_snow_item(a2, a3, width, height);

                let a2d = 1000.0 / a2.speed;
                let v8 = a2d;
                let a2e = -(self.y_offset as f32) * a2d * 13.0 / 1000.0;
                v6 = v8;
                let v18 = 13.0 * (-(self.x_offset as f32) * v8) / 1000.0;
                a2.x_pos += v18;
                a2.y_pos += a2e;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_snow_item2(
        a2: &mut SnowFlake,
        width: f32,
        height: f32,
        x_offset: i32,
        y_offset: i32,
        horizontal_speed: i32,
        start_x: i32,
        end_x: i32,
        start_y: i32,
        speed: i32,
    ) {
        if a2.speed != 0.0 && (x_offset != 0 || y_offset != 0 || horizontal_speed != 0) {
            let v4 = width;
            let a2a = height;

            let v10 = (start_y as f32) * 0.5;
            let v9 = 0.5 * (speed as f32);
            let v16 = start_x as f32;
            let v17 = end_x as f32;
            let a3 = -(horizontal_speed as f32) * 13.0 / 1000.0;

            Self::set_snow_item(a2, a3, width, height);

            let a2b = 1000.0 / a2.speed;
            let v5 = a2b;
            let a2c = -(y_offset as f32) * a2b * 13.0 / 1000.0;
            let mut v6 = v5;
            let v12 = 13.0 * (-(x_offset as f32) * v5) / 1000.0;
            a2.x_pos += v12;
            a2.y_pos += a2c;

            let v13 = -v10;
            while v13 * v5 <= a2.x_pos {
                let v7 = v9;
                if v6 * v10 + v4 < a2.x_pos {
                    break;
                }
                if -v7 * v6 > a2.y_pos {
                    break;
                }
                if v7 * v6 + a2a < a2.y_pos {
                    break;
                }
                if v16 > a2.speed {
                    break;
                }
                if v17 < a2.speed {
                    break;
                }
                Self::set_snow_item(a2, a3, width, height);

                let a2d = 1000.0 / a2.speed;
                let v8 = a2d;
                let a2e = -(y_offset as f32) * a2d * 13.0 / 1000.0;
                v6 = v8;
                let v18 = 13.0 * (-(x_offset as f32) * v8) / 1000.0;
                a2.x_pos += v18;
                a2.y_pos += a2e;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_snow(
        &mut self,
        a2: i32,
        a3: i32,
        a4: i32,
        a5: i32,
        a6: i32,
        a7: i32,
        a8: i32,
        a9: i32,
        a10: i32,
        a11: i32,
        a12: i32,
        a13: i32,
        a14: i32,
        a15: i32,
        a16: i32,
        a17: i32,
        a18: i32,
        screen_width: u32,
        screen_height: u32,
    ) {
        self.width = a2 as u32;
        self.height = a3 as u32;
        self.arg3 = a4;
        self.start_y = a5;
        self.speed = a6;
        self.x_variation = a7;
        self.start_x = a8;
        self.end_x = a9;
        self.arg9 = a17;
        self.item_count = a10 as u32;
        self.y_offset = a11;
        self.x_offset = a12;
        self.horizontal_speed = a13;
        self.arg14 = a14;
        self.arg15 = a15;
        self.arg16 = a16;
        self.arg17 = a18;

        self.flakes.clear();
        for _ in 0..self.item_count {
            self.flakes
                .push(self.initialize_snow_motion_parameters(screen_width, screen_height));
        }
        self.snow_flake_sort();
        self.running = false;
    }

    pub fn update(&mut self, elapsed: i32, screen_width: u32, screen_height: u32) {
        if self.running {
            let elapsed = if elapsed < 0 { -elapsed } else { elapsed };
            let width = if self.width == 0 {
                screen_width
            } else {
                self.width
            } as f32;
            let height = if self.height != 0 {
                screen_height
            } else {
                self.height
            } as f32;
            let randomness_factor = self.arg14;
            // let start_x = self.start_x as f32;
            // let end_x = self.end_x as f32;
            // let start_y = (self.start_y as f32) * 0.5;
            // let speed_factor = 0.5 * (self.speed as f32);

            for item in &mut self.flakes {
                let elapsed_seconds = elapsed as f32 / 1000.0;
                let random_x_offset = (rand::random::<i32>() % (2 * randomness_factor + 1)
                    - randomness_factor) as f32
                    + self.x_offset as f32;
                let random_y_offset = (rand::random::<i32>() % (2 * randomness_factor + 1)
                    - randomness_factor) as f32
                    + self.y_offset as f32;

                item.x_pos += self.horizontal_speed as f32 * elapsed_seconds
                    + random_x_offset * elapsed_seconds;
                item.y_pos += random_y_offset * elapsed_seconds;

                if item.x_pos < 0.0 || item.x_pos > width || item.y_pos < 0.0 || item.y_pos > height
                {
                    let mut new_flake = Self::initialize_snow_motion_parameters2(
                        width,
                        height,
                        self.start_x,
                        self.end_x,
                        self.start_y,
                        self.speed,
                        self.x_variation,
                    );

                    Self::update_snow_item2(
                        &mut new_flake,
                        width,
                        height,
                        self.x_offset,
                        self.y_offset,
                        self.horizontal_speed,
                        self.start_x,
                        self.end_x,
                        self.start_y,
                        self.speed,
                    );

                    *item = new_flake;
                }
            }

            if self.running && self.item_count > 0 {
                self.snow_flake_sort();
            }
        }
    }
}

pub struct SnowMotionContainer {
    motion: Vec<SnowMotion>,
}

impl SnowMotionContainer {
    pub fn new() -> Self {
        SnowMotionContainer {
            motion: vec![SnowMotion::new(); 2],
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_motion(
        &mut self,
        id: u32,
        a2: i32,
        a3: i32,
        a4: i32,
        a5: i32,
        a6: i32,
        a7: i32,
        a8: i32,
        a9: i32,
        a10: i32,
        a11: i32,
        a12: i32,
        a13: i32,
        a14: i32,
        a15: i32,
        a16: i32,
        a17: i32,
        a18: i32,
        screen_width: u32,
        screen_height: u32,
    ) {
        self.motion[id as usize].set_snow(
            a2,
            a3,
            a4,
            a5,
            a6,
            a7,
            a8,
            a9,
            a10,
            a11,
            a12,
            a13,
            a14,
            a15,
            a16,
            a17,
            a18,
            screen_width,
            screen_height,
        );
    }

    pub fn test_snow_motion(&self, id: u32) -> bool {
        self.motion[id as usize].running
    }

    pub fn start_snow_motion(&mut self, id: u32) {
        self.motion[id as usize].running = true;
    }

    pub fn stop_snow_motion(&mut self, id: u32) {
        self.motion[id as usize].running = false;
    }

    pub fn exec_snow_motion(&mut self, elapsed: i32, screen_width: u32, screen_height: u32) {
        for motion in &mut self.motion {
            motion.update(elapsed, screen_width, screen_height);
        }
    }
}
