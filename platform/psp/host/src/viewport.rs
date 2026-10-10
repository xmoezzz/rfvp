#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct PspViewport {
    pub design_w: u32,
    pub design_h: u32,
    pub target_w: u32,
    pub target_h: u32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl PspViewport {
    pub fn new(design_w: u32, design_h: u32, target_w: u32, target_h: u32) -> Self {
        let sx = target_w as f32 / design_w.max(1) as f32;
        let sy = target_h as f32 / design_h.max(1) as f32;
        let scale = if sx < sy { sx } else { sy };
        let physical_w = (design_w as f32 * scale) as i32;
        let physical_h = (design_h as f32 * scale) as i32;
        Self {
            design_w,
            design_h,
            target_w,
            target_h,
            scale_x: scale,
            scale_y: scale,
            offset_x: (target_w as i32 - physical_w) / 2,
            offset_y: (target_h as i32 - physical_h) / 2,
        }
    }

    pub fn physical_to_logical(&self, x: i32, y: i32) -> (i32, i32, bool) {
        let lx = ((x - self.offset_x) as f32 / self.scale_x.max(f32::EPSILON)) as i32;
        let ly = ((y - self.offset_y) as f32 / self.scale_y.max(f32::EPSILON)) as i32;
        let inside = lx >= 0 && ly >= 0 && lx < self.design_w as i32 && ly < self.design_h as i32;
        (lx, ly, inside)
    }
}
