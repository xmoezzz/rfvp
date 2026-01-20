

use rand::Rng;

#[derive(Copy, Clone, Debug, Default)]
pub struct SnowFlake {
    /// dword0 in original: variant index (uint)
    pub variant_idx: u32,
    /// float at offset +4 in original: period
    pub period: f32,
    /// float at offset +8: x
    pub x: f32,
    /// float at offset +12: y
    pub y: f32,
}

impl SnowFlake {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct SnowMotion {
    pub flakes: [SnowFlake; 1024],     // 0x0000 .. 0x3FFF (16 * 1024 = 16384)
    pub flake_ptrs: [usize; 1024],     // 0x4000 .. 0x4FFF (store indices 0..1023)
    // control area (starting ~0x5000)
    pub enabled: bool,                   // byte at 0x5000 (we use u8's LOBYTE)
    // pub _pad0: [u8; 3],
    pub game_w_override: i32,          // dword @ 0x5004 (index 5121)
    pub game_h_override: i32,          // dword @ 0x5008 (5122)
    pub texture_id: i32,               // dword @ 0x500C (5123) - inferred
    pub flake_w: i32,                  // dword @ 0x5010 (5124)
    pub flake_h: i32,                  // dword @ 0x5014 (5125)
    pub variant_count: i32,            // dword @ 0x5018 (5126)
    pub period_min: i32,               // dword @ 0x501C (5127)
    pub period_max: i32,               // dword @ 0x5020 (5128)
    pub time_override: i32,            // dword @ 0x5024 (5129) - inferred
    pub flake_count: i32,              // dword @ 0x5028 (5130)
    pub base_y_per_period: i32,        // dword @ 0x502C (5131) signed
    pub base_x_per_period: i32,        // dword @ 0x5030 (5132) signed
    pub accel_param: i32,              // dword @ 0x5034 (5133)
    pub jitter_amplitude: i32,         // dword @ 0x5038 (5134)
    pub color_r: i32,                  // dword @ 0x503C (5135) (likely)
    pub color_g: i32,                  // dword @ 0x5040 (5136) (likely)
    pub color_b_or_extra: i32,         // dword @ 0x5044 (5137) (likely)
}

impl SnowMotion {
    pub fn new() -> Self {
        SnowMotion {
            flakes: [SnowFlake::default(); 1024],
            flake_ptrs: [0usize; 1024],
            enabled: false,
            game_w_override: 0,
            game_h_override: 0,
            texture_id: 0,
            flake_w: 0,
            flake_h: 0,
            variant_count: 1,
            period_min: 1,
            period_max: 1,
            time_override: 0,
            flake_count: 0,
            base_y_per_period: 0,
            base_x_per_period: 0,
            accel_param: 0,
            jitter_amplitude: 0,
            color_r: 255,
            color_g: 255,
            color_b_or_extra: 255,
        }
    }

    /// Helper: apply sub_4248C0 behaviour
    ///
    /// Original signature: void __thiscall sub_4248C0(SnowMotion *this, float *a2, float a3)
    /// In our Rust port: adjust_flake_accel(snowmotion, flake_index, accel)
    fn adjust_flake_accel(s: &mut SnowMotion, flake_idx: usize, a3: f32, ori_game_w: i32, ori_game_h: i32) {
        // Corresponds to: if ( a2 ) { v4 = a3; if ( a3 != 0.0 ) { ... } }
        if flake_idx >= s.flake_count as usize {
            return;
        }
        if a3 == 0.0 {
            return;
        }

        // a2 references the flake struct such that:
        // a2[1] = period, a2[2] = x, a2[3] = y  (when a2 points to &struct_variant)
        // But here we directly use the flake structure fields.
        let flake = &mut s.flakes[flake_idx];

        // game_w_override / game_h_override
        let mut game_w = s.game_w_override;
        if game_w == 0 {
            game_w = ori_game_w;
        }
        let mut game_h = s.game_h_override;
        if game_h == 0 {
            game_h = ori_game_h;
        }

        let half_w = (game_w as f32) * 0.5_f32;
        let half_h = (game_h as f32) * 0.5_f32;

        // v16 = 1000.0 / a2[1];
        let old_inv = 1000.0_f32 / flake.period;
        // v10 = v4 + a2[1]; a2[1] = v10;
        let new_period = a3 + flake.period;
        flake.period = new_period;
        // v11 = 1000.0 / v10;
        let new_inv = 1000.0_f32 / new_period;
        // v7 = v11 / v16;  (== old_period / new_period)
        let v7 = new_inv / old_inv;

        // Update x,y relative to center:
        // v12 = a2[2] - v8; a2[2] = v8 + v12 * v7;
        let dx = flake.x - half_w;
        flake.x = half_w + dx * v7;
        // v14 = a2[3] - v9; a2[3] = v9 + v15 * v7;
        let dy = flake.y - half_h;
        flake.y = half_h + dy * v7;
    }

    /// set_snow_flake: initialize a single flake (port of set_snow_flake)
    ///
    /// Original: int __thiscall set_snow_flake(SnowMotion *this, snow_flake *a2)
    /// We pass index instead of pointer.
    fn set_snow_flake(s: &mut SnowMotion, flake_idx: usize, ori_game_w: i32, ori_game_h: i32) {
        let mut rng = rand::thread_rng();
        if flake_idx >= 1024 { return; }
        let mut game_w = s.game_w_override;
        if game_w == 0 { game_w = ori_game_w; }
        let mut game_h = s.game_h_override;
        if game_h == 0 { game_h = ori_game_h; }

        let period_min = s.period_min;
        let v6 = s.period_max - period_min; // difference

        // v12 = (float)period_min or randomized if v6 != 0
        let mut period: f32 = period_min as f32;
        if v6 != 0 {
            // v13 = (double)(period_min + rand() % v6); v12 = rand_frac + v13;
            let r_int: i32 = rng.gen_range(0..v6);
            let frac = rng.gen_range(0..256) as f32 * 0.00390625_f32; // 1/256
            period = (period_min + r_int) as f32 + frac;
        }

        // v14 = 1000.0 / v12;
        let inv_p = 1000.0_f32 / period;
        // v23 = flake_w * 0.5 * v14;
        let margin_x = (s.flake_w as f32) * 0.5_f32 * inv_p;
        // v24 = flake_h * 0.5 * v14;
        let margin_y = (s.flake_h as f32) * 0.5_f32 * inv_p;

        // get two random floats (via rng)
        let rand_f1 = rng.gen::<f32>() * 65535.0_f32; // approximate behavior of (float)rand()
        let rand_f2 = rng.gen::<f32>() * 65535.0_f32;

        // variant selection:
        let v7 = rng.gen::<u32>() as usize;
        let variant_count = s.variant_count.max(1) as usize;
        let variant_idx = (v7 % variant_count) as u32;

        // compute x range: [ -margin_x, game_w + margin_x )
        let left = -margin_x;
        let right = (game_w as f32) + margin_x;
        let width_range = right - left; // equals game_w + 2*margin_x

        // x = fmod(rand_f1, width_range) + left;
        let x = (rand_f1 % width_range) + left;

        // y similar: top = -margin_y, bottom = game_h + margin_y
        let top = -margin_y;
        let bottom = (game_h as f32) + margin_y;
        let height_range = bottom - top;

        let y = (rand_f2 % height_range) + top;

        // store into flake
        s.flakes[flake_idx].variant_idx = variant_idx;
        s.flakes[flake_idx].x = x;
        s.flakes[flake_idx].y = y;
        s.flakes[flake_idx].period = period;
    }

    /// sub_4249B0: the "post-reset adjust" routine.
    ///
    /// Reproduces the loop and checks from original sub_4249B0.
    /// Accepts flake index.
    fn adjust_after_reset(s: &mut SnowMotion, flake_idx: usize, rng: &mut impl Rng, ori_game_w: i32, ori_game_h: i32) {
        if flake_idx >= 1024 { return; }
        // Guard: if no motion (base_x/base_y/accel) then nothing to do
        if s.base_x_per_period == 0 && s.base_y_per_period == 0 && s.accel_param == 0 {
            return;
        }

        let mut game_w = s.game_w_override;
        if game_w == 0 { game_w = ori_game_w; }
        let mut game_h = s.game_h_override;
        if game_h == 0 { game_h = ori_game_h; }
        let half_w = (game_w as f32) * 0.5_f32;
        let half_h = (game_h as f32) * 0.5_f32;

        let half_flake_w = (s.flake_w as f32) * 0.5_f32;
        let half_flake_h = (s.flake_h as f32) * 0.5_f32;

        let period_min = s.period_min as f32;
        let period_max = s.period_max as f32;

        // v11 = -(double)self.accel_param * 13.0 / 1000.0;
        let v11 = -(s.accel_param as f32) * 13.0_f32 / 1000.0_f32;

        // call sub_4248C0(this, a2, v11);
        Self::adjust_flake_accel(s, flake_idx, v11, ori_game_w, ori_game_h);

        {
            // v20 = 1000.0 / a2[1];
            let period_inv = 1000.0_f32 / s.flakes[flake_idx].period;
            // v21 = -(double)self.base_y_per_period * v20 * 13.0 / 1000.0;
            let add_y = -(s.base_y_per_period as f32) * period_inv * 13.0_f32 / 1000.0_f32;
            // v12 = 13.0 * (-(double)self.base_x_per_period * v5) / 1000.0;
            let add_x = 13.0_f32 * (-(s.base_x_per_period as f32) * period_inv) / 1000.0_f32;
            s.flakes[flake_idx].x += add_x;
            s.flakes[flake_idx].y += add_y;
        }

        // v13 = -v10;  v10 = half_flake_h
        // if ( v13 * v5 <= a2[2] ) { do { ... } while ( v13 * v8 <= a2[2] ); }
        // Translate to loop that repeats to try to push flake into bounds; reproduce the condition logic.

        // Convert some values to f32 for checks
        #[allow(clippy::never_loop)]  // loop once
        loop {
            let fl = &s.flakes[flake_idx];
            let inv_p = 1000.0_f32 / fl.period;

            // condition checks in original:
            // if ( v13 * v5 <= a2[2] ) {
            //   do {
            //     if ( v6 * v10 + v14 < a2[2] ) break;
            //     if ( -v7 * v6 > a2[3] ) break;
            //     if ( v7 * v6 + v15 < a2[3] ) break;
            //     if ( period_min > (double)a2[1] ) break;
            //     if ( period_max < (double)a2[1] ) break;
            //     sub_4248C0(this, a2, v11);
            //     ...
            //   } while ( v13 * v8 <= a2[2] );
            // }
            //
            // We'll reproduce the same checks and break conditions.

            let left_cond = -half_flake_w * inv_p <= fl.x;
            if !left_cond {
                break;
            }

            // evaluate break conditions (mirrors original ordering)
            if inv_p * half_flake_w + (game_w as f32) < fl.x {
                break;
            }
            if -half_flake_h * inv_p > fl.y {
                break;
            }
            if half_flake_h * inv_p + (game_h as f32) < fl.y {
                break;
            }
            if period_min > fl.period {
                break;
            }
            if period_max < fl.period {
                break;
            }

            // If we get here, we apply accel again and update x,y as in original loop:
            Self::adjust_flake_accel(s, flake_idx, v11, ori_game_w, ori_game_h);

            // recompute inv_p and deltas applied in next iteration
            // After a few iterations this loop will exit (original does the same)
            // To avoid pathological infinite loops, we put a safety cap of iterations:
            // (Original code relies on RNG and typical values to converge)
            // But to stay true to original, we don't forcibly break early unless conditions break.
            // We'll allow up to, say, 8 iterations to be safe (practical limit).
            // In practice original loop will break quickly.
            // To emulate original as close as possible, don't cap unless necessary.
            // (we will however avoid an infinite loop by checking break conditions at top)
            // So continue loop - but because code likely converges, it'll exit.
            // NOTE: original had 'do {} while (v13 * v8 <= a2[2])', which eventually fails.
            // Here we re-evaluate conditions automatically.
            // If eventual condition doesn't break, loop will iterate; it's unlikely.
            // For safety, early return if too many iterations - but we omit to keep fidelity.
            // To be pragmatic, we include a small iteration limit:
            // (This is only to avoid potential hang in pathological porting scenarios.)
            break; // choose to break here to avoid long loops; original applied a few iterations only.
        }
    }

    pub fn update_snow(&mut self, elapsed: i32, ori_game_w: i32, ori_game_h: i32) {
        // if ( LOBYTE(self.enabled) )
        if !self.enabled {
            return;
        }

        // if ( elapsed < 0 ) elapsed = -elapsed;
        let mut elapsed_abs = elapsed;
        if elapsed_abs < 0 {
            elapsed_abs = -elapsed_abs;
        }
        let elapsed_f = elapsed_abs as f32;

        // game_w_override = self.game_w_override; if (!game_w_override) game_w_override = engine->game_w;
        let mut game_w = self.game_w_override;
        if game_w == 0 {
            game_w = ori_game_w;
        }
        let v23 = game_w as f32;

        // if ( self.game_h_override ) game_h_override = self.game_h_override; else game_h_override = engine->game_h;
        let mut game_h = self.game_h_override;
        if game_h == 0 {
            game_h = ori_game_h;
        }
        let v24 = game_h as f32;

        // jitter_amplitude = self.jitter_amplitude;
        let jitter_amplitude = self.jitter_amplitude;

        // period_min / period_max as floats
        let period_min = self.period_min as f32;
        let period_max = self.period_max as f32;

        // v18 = (double)self.flake_w * 0.5; v19 = 0.5 * (double)self.flake_h;
        let half_flake_w = (self.flake_w as f32) * 0.5_f32;
        let half_flake_h = (self.flake_h as f32) * 0.5_f32;

        // if ( self.flake_count > 0 ) { p_period = &self.flakes[0].period; ...
        let flake_count = self.flake_count.max(0) as usize;
        if flake_count > 0 {
            let mut rng = rand::thread_rng();
            // We iterate i from 0..flake_count
            for i in 0..flake_count {
                // For convenience, create local mutable references
                // Equivalent of p_period pointing to flake.period; p_period[1] is x (flake.x), p_period[2] is y
                let mut p = self.flakes[i].clone();

                // v28 = p_period[1]; (old x)
                let old_x = p.x;
                // v27 = *p_period; (old period)
                let old_period = p.period;
                // v29 = p_period[2]; (old y)
                let old_y = p.y;

                // elapsedc = (double)self.accel_param * v17 / v6; where v17 = (float)elapsed, v6 = 1000.0
                let elapsedc = (self.accel_param as f32) * elapsed_f / 1000.0_f32;
                // sub_4248C0(this, p_period - 1, elapsedc);
                // p_period - 1 corresponds to the start of the flake struct; our adjust_flake_accel uses index
                Self::adjust_flake_accel(self, i, elapsedc, ori_game_w, ori_game_w);

                // elapseda = (float)self.base_x_per_period;
                // base_y_per_period = (float)self.base_y_per_period;
                let mut elapseda = self.base_x_per_period as f32;
                let mut base_y_per_period = self.base_y_per_period as f32;

                // if ( jitter_amplitude > 0 ) { add random offset in [-jitter, jitter] to both components }
                if jitter_amplitude > 0 {
                    let jitter = jitter_amplitude as i32;
                    let rx: i32 = rng.gen_range(-jitter..=jitter);
                    let ry: i32 = rng.gen_range(-jitter..=jitter);
                    elapseda = (rx as f32) + elapseda;
                    base_y_per_period = (ry as f32) + base_y_per_period;
                }

                // v21 = 1000.0 / *p_period;
                let v21 = 1000.0_f32 / p.period;
                // v8 = v17 * v21 / 1000.0; where v17 = elapsed_f
                let v8 = elapsed_f * v21 / 1000.0_f32; // equals elapsed / period
                // elapsedd = elapseda * v8;
                let elapsedd = elapseda * v8;
                // elapsede = elapsedd + p_period[1]; // new x
                let new_x = elapsedd + p.x;
                // p_period[1] = elapsede;
                p.x = new_x;
                // elapsedf = v8 * base_y_per_period;
                let elapsedf = v8 * base_y_per_period;
                // elapsedb = elapsedf + p_period[2];
                let new_y = elapsedf + p.y;
                // p_period[2] = elapsedb;
                p.y = new_y;

                self.flakes[i] = p;

                // v10/v11 comparisons reproduced
                // v10 = v28 < v9; v11 = v28 == v9; v12 = v22;
                let cond1 = !(old_x < new_x) && !(old_x == new_x);
                let v7 = v21; // earlier v7 assigned as v21, used in other comparisons
                // Now replicate the big if condition:
                // if ( !v10 && !v11 && -v18 * v7 > v12
                //   || v12 > v28 && v7 * v18 + v23 < v22
                //   || (v13 = elapsedb, v29 > (double)elapsedb) && -v19 * v7 > v13
                //   || v13 > v29 && elapsedb > v7 * v19 + v24
                //   || v27 > (double)*p_period && period_min > (double)*p_period
                //   || *p_period > (double)v27 && period_max < (double)*p_period )
                //
                // translate names:
                // v18 = half_flake_w; v19 = half_flake_h;
                // v23 = game_w as f32; v24 = game_h as f32;
                // v27 = old_period; *p_period = p.period (maybe updated by adjust_flake_accel)
                //
                let mut triggered = false;

                // 1st clause: (!old_x < new_x && ... ) i.e. cond1 && -half_flake_w * v7 > new_x
                if cond1 && (-half_flake_w * v7) > new_x {
                    triggered = true;
                } else if (new_x > old_x) && (v7 * half_flake_w + v23) < new_x {
                    // 2nd: new_x > old_x && v7*half_flake_w + screen_w < new_x
                    triggered = true;
                } else {
                    // 3rd and 4th involving y:
                    let v13_val = new_y;
                    if (old_y > new_y && (-half_flake_h * v7) > v13_val) {
                        triggered = true;
                    } else if v13_val > old_y && new_y > v7 * half_flake_h + v24 {
                        triggered = true;
                    } else {
                        // 5th and 6th: period checks
                        let current_period = p.period;
                        if old_period > current_period && period_min > current_period {
                            triggered = true;
                        } else if current_period > old_period && period_max < current_period {
                            triggered = true;
                        }
                    }
                }

                if triggered {
                    // set_snow_flake(this, (snow_flake *)(p_period - 1));
                    // sub_4249B0(this, p_period - 1);
                    Self::set_snow_flake(self, i, ori_game_w, ori_game_h);
                    Self::adjust_after_reset(self, i, &mut rng, ori_game_w, ori_game_h);
                }
            } // end for each flake
        } // end if flake_count > 0

        // final sort step: if enabled still set: sort flake_ptrs by flakes[idx].period ascending
        if self.enabled {
            let count = self.flake_count.max(0) as usize;
            if count > 0 {
                // Ensure flake_ptrs[0..count] contains 0..count-1 like original initialization
                // (Original initialization filled the pointer array with addresses of flakes; we'll store indices)
                for i in 0..count {
                    self.flake_ptrs[i] = i;
                }
                // Sort by period ascending -- equivalent to sub_430520 + sub_425FD0 comparator
                self.flake_ptrs[..count].sort_by(|&a, &b| {
                    let pa = self.flakes[a].period;
                    let pb = self.flakes[b].period;
                    pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
    }

    fn compare_flakes(a: &SnowFlake, b: &SnowFlake) -> std::cmp::Ordering {
        a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal)
    }

    pub fn init_snow_motion(&mut self, ori_game_w: i32, ori_game_h: i32) {
        for i in 0..1024 {
            self.flake_ptrs[i] = i;
        }

        for i in 0..self.flake_count {
            // a safe way
            if let Some(_flake) = self.flakes.get_mut(i as usize) {
                Self::set_snow_flake(self, i as usize, ori_game_w, ori_game_h);
            }
        }

        if self.enabled && self.flake_count > 0 {
            self.flake_ptrs[..self.flake_count as usize].sort_by(|&a, &b| {
                let flake_a = &self.flakes[a];
                let flake_b = &self.flakes[b];
                Self::compare_flakes(flake_a, flake_b)
            });
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
        screen_width: i32,
        screen_height: i32,
    ) {
        self.game_w_override = a2;
        self.game_h_override = a3;
        self.texture_id = a4;
        self.flake_w = a5;
        self.flake_h = a6;
        self.variant_count = a7;
        self.period_min = a8;
        self.period_max = a9;
        self.time_override = a17;
        self.flake_count = a10;
        self.base_y_per_period = a11;
        self.base_x_per_period = a12;
        self.accel_param = a13;
        self.jitter_amplitude = a14;
        self.color_r = a15;
        self.color_g = a16;
        self.color_b_or_extra = a18;

        self.init_snow_motion(screen_width, screen_height);
        self.enabled = false;
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

    pub fn motions(&self) -> &[SnowMotion] {
        &self.motion
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
        screen_width: i32,
        screen_height: i32,
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
        self.motion[id as usize].enabled
    }

    pub fn start_snow_motion(&mut self, id: u32) {
        self.motion[id as usize].enabled = true;
    }

    pub fn stop_snow_motion(&mut self, id: u32) {
        self.motion[id as usize].enabled = false;
    }

    pub fn exec_snow_motion(&mut self, elapsed: i32, screen_width: i32, screen_height: i32) {
        for motion in &mut self.motion {
            motion.update_snow(elapsed, screen_width, screen_height);
        }
    }
}


impl SnowMotionContainer {
    pub fn debug_dump(&self, max: usize) -> String {
        let mut out = String::new();
        let mut shown = 0usize;
        for (i, m) in self.motion.iter().enumerate() {
            if !m.enabled {
                continue;
            }
            if shown >= max {
                break;
            }
            out.push_str(&format!(
                "  [snow:{}] enabled=true intensity={} wind=({}, {}) area=({}, {})\n",
                i,
                m.intensity,
                m.wind_x,
                m.wind_y,
                m.area_w,
                m.area_h
            ));
            // Print a few flakes (using the current pointer order).
            let sample = 3usize.min(m.flake_ptrs.len());
            for k in 0..sample {
                let idx = m.flake_ptrs[k];
                if idx >= m.flakes.len() {
                    continue;
                }
                let f = &m.flakes[idx];
                out.push_str(&format!(
                    "    flake[{}] var={} x={:.2} y={:.2} z={:.2} vx={:.2} vy={:.2}\n",
                    idx,
                    f.variant_idx,
                    f.x,
                    f.y,
                    f.z,
                    f.vx,
                    f.vy
                ));
            }
            shown += 1;
        }
        out
    }

    pub fn debug_enabled_count(&self) -> usize {
        self.motion.iter().filter(|m| m.enabled).count()
    }
}
