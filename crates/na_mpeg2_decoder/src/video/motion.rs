use super::utils::avg_u8;
use super::videodsp::emulated_edge_mc;
use super::frame::Frame;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionOp {
    Put,
    Avg,
}

#[inline]
fn op_store(op: MotionOp, dst: &mut u8, val: u8) {
    match op {
        MotionOp::Put => *dst = val,
        MotionOp::Avg => *dst = avg_u8(*dst, val),
    }
}

#[inline]
fn interp_2tap(a: u8, b: u8) -> u8 {
    avg_u8(a, b)
}

#[inline]
fn interp_4tap(a: u8, b: u8, c: u8, d: u8) -> u8 {
    // Bilinear interpolation at half-pel in both directions.
    // Exact integer rounding: (a + b + c + d + 2) >> 2.
    ((a as u16 + b as u16 + c as u16 + d as u16 + 2) >> 2) as u8
}

#[derive(Debug)]
pub struct MotionCompensator {
    edge_y: Vec<u8>,
    edge_uv: Vec<u8>,
}

impl MotionCompensator {
    pub fn new() -> Self {
        Self {
            edge_y: vec![0u8; 17 * 17],
            edge_uv: vec![0u8; 17 * 17],
        }
    }

    fn mc_plane(
        dst: &mut [u8],
        dst_stride: usize,
        src: &[u8],
        src_stride: usize,
        w: isize,
        h: isize,
        dst_x: usize,
        dst_y: usize,
        src_x: isize,
        src_y: isize,
        bw: usize,
        bh: usize,
        dxy: u32,
        op: MotionOp,
        scratch: &mut [u8],
        scratch_stride: usize,
    ) {
        emulated_edge_mc(
            scratch,
            src,
            scratch_stride as isize,
            src_stride as isize,
            bw + 1,
            bh + 1,
            src_x,
            src_y,
            w,
            h,
        );

        let dx = (dxy & 1) as usize;
        let dy = ((dxy >> 1) & 1) as usize;

        for yy in 0..bh {
            for xx in 0..bw {
                let base = yy * scratch_stride + xx;
                let a = scratch[base];
                let val = match (dx, dy) {
                    (0, 0) => a,
                    (1, 0) => interp_2tap(a, scratch[base + 1]),
                    (0, 1) => interp_2tap(a, scratch[base + scratch_stride]),
                    (1, 1) => interp_4tap(
                        a,
                        scratch[base + 1],
                        scratch[base + scratch_stride],
                        scratch[base + scratch_stride + 1],
                    ),
                    _ => a,
                };
                let di = (dst_y + yy) * dst_stride + (dst_x + xx);
                op_store(op, &mut dst[di], val);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn mpeg_motion_internal(
        &mut self,
        cur: &mut Frame,
        op: MotionOp,
        ref_frame: &Frame,
        field_based: bool,
        bottom_field: bool,
        field_select: bool,
        mb_x: usize,
        mb_y: usize,
        motion_x: i32,
        motion_y: i32,
        h: usize,
        is_16x8: bool,
    ) {
        let (chroma_x_shift, chroma_y_shift) = cur.chroma_shifts();

        let field_based_i = if field_based { 1 } else { 0 };
        let block_y_half = (field_based_i | if is_16x8 { 1 } else { 0 }) as usize;

        let dxy = (((motion_y & 1) << 1) | (motion_x & 1)) as u32;
        let src_x = (mb_x as i32) * 16 + (motion_x >> 1);
        let src_y = ((mb_y as i32) << (4 - block_y_half)) + (motion_y >> 1);

        let (uvdxy, uvsrc_x, uvsrc_y) = if chroma_y_shift != 0 {
            let mx = motion_x / 2;
            let my = motion_y / 2;
            (
                (((my & 1) << 1) | (mx & 1)) as u32,
                (mb_x as i32) * 8 + (mx >> 1),
                ((mb_y as i32) << (3 - block_y_half)) + (my >> 1),
            )
        } else {
            if chroma_x_shift != 0 {
                // 4:2:2
                let mx = motion_x / 2;
                (
                    (((motion_y & 1) << 1) | (mx & 1)) as u32,
                    (mb_x as i32) * 8 + (mx >> 1),
                    src_y,
                )
            } else {
                // 4:4:4
                (dxy, src_x, src_y)
            }
        };

        // Destination view
        let (base_y, stride_y) = if field_based {
            let base = if bottom_field { cur.linesize_y } else { 0 };
            (base, cur.linesize_y * 2)
        } else {
            (0, cur.linesize_y)
        };
        let (base_uv, stride_uv) = if field_based {
            let base = if bottom_field { cur.linesize_u } else { 0 };
            (base, cur.linesize_u * 2)
        } else {
            (0, cur.linesize_u)
        };

        // Reference view.
        // In field-based motion compensation, the reference is accessed with doubled stride
        // (sampling every other line). Field selection only adds a one-line offset.
        let ref_stride_y = if field_based {
            ref_frame.linesize_y * 2
        } else {
            ref_frame.linesize_y
        };
        let ref_stride_uv = if field_based {
            ref_frame.linesize_u * 2
        } else {
            ref_frame.linesize_u
        };
        let ref_base_y = if field_select { ref_frame.linesize_y } else { 0 };
        let ref_base_uv = if field_select { ref_frame.linesize_u } else { 0 };

        // Destination coordinates.
        let dst_x = mb_x * 16;
        let dst_y = if field_based { mb_y * 8 } else { mb_y * 16 };

        // Luma
        {
            let dst_plane = &mut cur.data_y;
            let src_plane = &ref_frame.data_y;
            Self::mc_plane(
                &mut dst_plane[base_y..],
                stride_y,
                &src_plane[ref_base_y..],
                ref_stride_y,
                ref_frame.width as isize,
                ref_frame.height as isize,
                dst_x,
                dst_y,
                src_x as isize,
                src_y as isize,
                16,
                h,
                dxy,
                op,
                &mut self.edge_y,
                17,
            );
        }

        let bw_uv = 16 >> chroma_x_shift;
        let bh_uv = h >> chroma_y_shift;
        let dst_x_uv = mb_x * bw_uv;
        let dst_y_uv = if field_based { mb_y * (8 >> chroma_y_shift) } else { mb_y * (16 >> chroma_y_shift) };

        // U
        {
            let dst_u = &mut cur.data_u;
            let src_u = &ref_frame.data_u;
            Self::mc_plane(
                &mut dst_u[base_uv..],
                stride_uv,
                &src_u[ref_base_uv..],
                ref_stride_uv,
                (ref_frame.width >> chroma_x_shift) as isize,
                (ref_frame.height >> chroma_y_shift) as isize,
                dst_x_uv,
                dst_y_uv,
                uvsrc_x as isize,
                uvsrc_y as isize,
                bw_uv,
                bh_uv,
                uvdxy,
                op,
                &mut self.edge_uv,
                17,
            );
        }

        // V
        {
            let dst_v = &mut cur.data_v;
            let src_v = &ref_frame.data_v;
            Self::mc_plane(
                &mut dst_v[base_uv..],
                stride_uv,
                &src_v[ref_base_uv..],
                ref_stride_uv,
                (ref_frame.width >> chroma_x_shift) as isize,
                (ref_frame.height >> chroma_y_shift) as isize,
                dst_x_uv,
                dst_y_uv,
                uvsrc_x as isize,
                uvsrc_y as isize,
                bw_uv,
                bh_uv,
                uvdxy,
                op,
                &mut self.edge_uv,
                17,
            );
        }
    }

    /// Motion compensation driver corresponding to MPEG-1/2 subset of `mpv_motion_internal()`.
    #[allow(clippy::too_many_arguments)]
    pub fn mpv_motion(
        &mut self,
        cur: &mut Frame,
        ref_frame: &Frame,
        dir: usize,
        op: MotionOp,
        mv_type: i32,
        picture_structure: i32,
        mb_x: usize,
        mb_y: usize,
        mv: &[[[i32; 2]; 4]; 2],
        field_select: &[[i32; 2]; 2],
        first_field: bool,
    ) {
        match mv_type {
            0 => {
                // MV_TYPE_16X16
                self.mpeg_motion_internal(
                    cur,
                    op,
                    ref_frame,
                    false,
                    false,
                    false,
                    mb_x,
                    mb_y,
                    mv[dir][0][0],
                    mv[dir][0][1],
                    16,
                    false,
                );
            }
            2 => {
                // MV_TYPE_FIELD
                if picture_structure == 3 {
                    // Frame picture: process top and bottom fields separately.
                    for i in 0..2 {
                        self.mpeg_motion_internal(
                            cur,
                            op,
                            ref_frame,
                            true,
                            i == 1,
                            field_select[dir][i] != 0,
                            mb_x,
                            mb_y,
                            mv[dir][i][0],
                            mv[dir][i][1],
                            8,
                            false,
                        );
                    }
                } else {
                    // Field picture: single motion.
                    self.mpeg_motion_internal(
                        cur,
                        op,
                        ref_frame,
                        false,
                        false,
                        field_select[dir][0] != 0,
                        mb_x,
                        mb_y >> 1,
                        mv[dir][0][0],
                        mv[dir][0][1],
                        16,
                        false,
                    );
                }
            }
            1 => {
                // MV_TYPE_16X8
                for i in 0..2 {
                    self.mpeg_motion_internal(
                        cur,
                        op,
                        ref_frame,
                        false,
                        false,
                        field_select[dir][i] != 0,
                        mb_x,
                        (mb_y & !1) + i,
                        mv[dir][i][0],
                        mv[dir][i][1],
                        8,
                        true,
                    );
                }
            }
            3 => {
                // MV_TYPE_DMV
                if picture_structure == 3 {
                    // Frame: two passes.
                    let mut op2 = op;
                    for i in 0..2 {
                        for j in 0..2 {
                            self.mpeg_motion_internal(
                                cur,
                                op2,
                                ref_frame,
                                true,
                                j == 1,
                                (j ^ i) != 0,
                                mb_x,
                                mb_y,
                                mv[dir][2 * i + j][0],
                                mv[dir][2 * i + j][1],
                                8,
                                false,
                            );
                        }
                        op2 = MotionOp::Avg;
                    }
                } else {
                    // Field picture.
                    let mut op2 = op;
                    for i in 0..2 {
                        self.mpeg_motion_internal(
                            cur,
                            op2,
                            ref_frame,
                            false,
                            false,
                            picture_structure != (i as i32 + 1),
                            mb_x,
                            mb_y >> 1,
                            mv[dir][2 * i][0],
                            mv[dir][2 * i][1],
                            16,
                            false,
                        );
                        op2 = MotionOp::Avg;
                        let _ = first_field;
                    }
                }
            }
            _ => {}
        }
    }
}
