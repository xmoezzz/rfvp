#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Yuv420p,
    Yuv422p,
    Yuv444p,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub format: PixelFormat,
    pub data_y: Vec<u8>,
    pub data_u: Vec<u8>,
    pub data_v: Vec<u8>,
    pub linesize_y: usize,
    pub linesize_u: usize,
    pub linesize_v: usize,
    pub pts_90k: Option<i64>,
}

impl Frame {
    pub fn new(width: usize, height: usize, format: PixelFormat) -> Self {
        let (cx, cy) = match format {
            PixelFormat::Yuv420p => (1usize, 1usize),
            PixelFormat::Yuv422p => (1usize, 0usize),
            PixelFormat::Yuv444p => (0usize, 0usize),
        };
        let w_uv = width >> cx;
        let h_uv = height >> cy;
        let y = vec![0u8; width * height];
        let u = vec![128u8; w_uv * h_uv];
        let v = vec![128u8; w_uv * h_uv];
        Self {
            width,
            height,
            format,
            data_y: y,
            data_u: u,
            data_v: v,
            linesize_y: width,
            linesize_u: w_uv,
            linesize_v: w_uv,
            pts_90k: None,
        }
    }

    #[inline]
    pub fn chroma_shifts(&self) -> (usize, usize) {
        match self.format {
            PixelFormat::Yuv420p => (1, 1),
            PixelFormat::Yuv422p => (1, 0),
            PixelFormat::Yuv444p => (0, 0),
        }
    }
}
