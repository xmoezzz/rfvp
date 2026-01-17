use anyhow::{anyhow, bail, Result};

#[derive(Debug, Clone)]
pub struct H264Config {
    pub width: u32,
    pub height: u32,

    /// AVCDecoderConfigurationRecord (aka `avcC` box payload).
    pub avcc: Vec<u8>,

    /// NAL length field size in bytes (typically 4).
    pub nal_length_size: usize,

    /// SPS NAL units without start code or length prefix.
    pub sps: Vec<Vec<u8>>,

    /// PPS NAL units without start code or length prefix.
    pub pps: Vec<Vec<u8>>,
}

impl H264Config {
    pub fn parse_from_avcc(width: u32, height: u32, avcc: &[u8]) -> Result<Self> {
        if avcc.len() < 7 {
            bail!("avcC too short");
        }
        if avcc[0] != 1 {
            bail!("unsupported avcC version {}", avcc[0]);
        }

        // byte 4: 6 bits reserved (111111) + 2 bits lengthSizeMinusOne
        let nal_length_size = ((avcc[4] & 0b11) as usize) + 1;
        if nal_length_size < 1 || nal_length_size > 4 {
            bail!("invalid nal_length_size={}", nal_length_size);
        }

        let mut off = 5;

        // byte 5: 3 bits reserved (111) + 5 bits numOfSPS
        if off >= avcc.len() {
            bail!("avcC truncated");
        }
        let num_sps = (avcc[off] & 0b1_1111) as usize;
        off += 1;

        let mut sps = Vec::with_capacity(num_sps);
        for _ in 0..num_sps {
            if off + 2 > avcc.len() {
                bail!("avcC truncated in SPS length");
            }
            let len = u16::from_be_bytes([avcc[off], avcc[off + 1]]) as usize;
            off += 2;
            if off + len > avcc.len() {
                bail!("avcC truncated in SPS data");
            }
            sps.push(avcc[off..off + len].to_vec());
            off += len;
        }

        if off >= avcc.len() {
            bail!("avcC truncated before PPS count");
        }
        let num_pps = avcc[off] as usize;
        off += 1;

        let mut pps = Vec::with_capacity(num_pps);
        for _ in 0..num_pps {
            if off + 2 > avcc.len() {
                bail!("avcC truncated in PPS length");
            }
            let len = u16::from_be_bytes([avcc[off], avcc[off + 1]]) as usize;
            off += 2;
            if off + len > avcc.len() {
                bail!("avcC truncated in PPS data");
            }
            pps.push(avcc[off..off + len].to_vec());
            off += len;
        }

        if sps.is_empty() || pps.is_empty() {
            return Err(anyhow!("avcC must contain at least one SPS and PPS"));
        }

        Ok(Self {
            width,
            height,
            avcc: avcc.to_vec(),
            nal_length_size,
            sps,
            pps,
        })
    }

    /// Microsoft Media Foundation expects MF_MT_MPEG_SEQUENCE_HEADER to contain Annex B SPS/PPS
    /// concatenated with start codes.
    pub fn annexb_sequence_header(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for ps in self.sps.iter().chain(self.pps.iter()) {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(ps);
        }
        out
    }

    /// Convert a MP4/AVCC sample (length-prefixed NAL units) into Annex B (start-code delimited).
    pub fn avcc_sample_to_annexb(&self, sample: &[u8]) -> Result<Vec<u8>> {
        let n = self.nal_length_size;
        if n == 0 || n > 4 {
            bail!("invalid nal_length_size");
        }

        let mut off = 0usize;
        let mut out = Vec::with_capacity(sample.len() + 64);

        while off + n <= sample.len() {
            let len = match n {
                1 => sample[off] as usize,
                2 => u16::from_be_bytes([sample[off], sample[off + 1]]) as usize,
                3 => ((sample[off] as usize) << 16) | ((sample[off + 1] as usize) << 8) | (sample[off + 2] as usize),
                4 => u32::from_be_bytes([sample[off], sample[off + 1], sample[off + 2], sample[off + 3]]) as usize,
                _ => unreachable!(),
            };
            off += n;

            if off + len > sample.len() {
                bail!("avcc sample truncated: off={} len={} size={}", off, len, sample.len());
            }

            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(&sample[off..off + len]);
            off += len;
        }

        if off != sample.len() {
            bail!("avcc sample has trailing bytes: off={} size={}", off, sample.len());
        }

        Ok(out)
    }
}
