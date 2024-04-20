use mp4::Mp4Track;

/// This struct converts NAL units from the MP4 to the Annex B format, expected by openh264.
///
/// It also inserts SPS and PPS units from the MP4 header into the stream.
/// They are also required for Annex B format to be decodable, but are not present in the MP4 bitstream, as they are stored in the headers.
pub struct Mp4BitstreamConverter {
    length_size: u8,
    sequence_parameter_sets: Vec<Vec<u8>>,
    picture_parameter_sets: Vec<Vec<u8>>,

    new_idr: bool,
    sps_seen: bool,
    pps_seen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NalType {
    Unspecified = 0,
    Slice = 1,
    Dpa = 2,
    Dpb = 3,
    Dpc = 4,
    IdrSlice = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    Aud = 9,
    EndSequence = 10,
    EndStream = 11,
    FillerData = 12,
    SpsExt = 13,
    Prefix = 14,
    SubSps = 15,
    Dps = 16,
    Reserved17 = 17,
    Reserved18 = 18,
    AuxiliarySlice = 19,
    ExtenSlice = 20,
    DepthExtenSlice = 21,
    Reserved22 = 22,
    Reserved23 = 23,
    Unspecified24 = 24,
    Unspecified25 = 25,
    Unspecified26 = 26,
    Unspecified27 = 27,
    Unspecified28 = 28,
    Unspecified29 = 29,
    Unspecified30 = 30,
    Unspecified31 = 31,
}

impl From<u8> for NalType {
    fn from(value: u8) -> Self {
        use NalType::*;
        match value {
            0 => Unspecified,
            1 => Slice,
            2 => Dpa,
            3 => Dpb,
            4 => Dpc,
            5 => IdrSlice,
            6 => Sei,
            7 => Sps,
            8 => Pps,
            9 => Aud,
            10 => EndSequence,
            11 => EndStream,
            12 => FillerData,
            13 => SpsExt,
            14 => Prefix,
            15 => SubSps,
            16 => Dps,
            17 => Reserved17,
            18 => Reserved18,
            19 => AuxiliarySlice,
            20 => ExtenSlice,
            21 => DepthExtenSlice,
            22 => Reserved22,
            23 => Reserved23,
            24 => Unspecified24,
            25 => Unspecified25,
            26 => Unspecified26,
            27 => Unspecified27,
            28 => Unspecified28,
            29 => Unspecified29,
            30 => Unspecified30,
            31 => Unspecified31,
            _ => panic!("Invalid NAL type"),
        }
    }
}

impl Mp4BitstreamConverter {
    /// Create a new converter for the given track.
    /// The track must contain AVC1 configuration.
    pub fn for_mp4_track(track: &Mp4Track) -> Self {
        let config_box = &track
            .trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()
            .expect("The track does not contain AVC1 configuration")
            .avcc;

        Self {
            length_size: config_box.length_size_minus_one + 1,
            sequence_parameter_sets: config_box
                .sequence_parameter_sets
                .iter()
                .cloned()
                .map(|v| v.bytes)
                .collect(),
            picture_parameter_sets: config_box
                .picture_parameter_sets
                .iter()
                .cloned()
                .map(|v| v.bytes)
                .collect(),

            new_idr: true,
            sps_seen: false,
            pps_seen: false,
        }
    }

    /// Convert a single packet from the MP4 format to the Annex B format.
    ///
    /// It clears the `out` vector and appends the converted packet to it.
    pub fn convert_packet(&mut self, packet: &[u8], out: &mut Vec<u8>) {
        let mut stream = packet;
        out.clear();
        while !stream.is_empty() {
            // read the length of the NAL unit
            let mut nal_size = 0;
            for _ in 0..self.length_size {
                nal_size = (nal_size << 8) | stream[0] as u32;
                stream = &stream[1..];
            }

            if nal_size == 0 {
                continue;
            }

            let nal = &stream[..nal_size as usize];
            stream = &stream[nal_size as usize..];

            let nal_type = NalType::from(nal[0] & 0x1F);

            match nal_type {
                NalType::Sps => {
                    self.sps_seen = true;
                }
                NalType::Pps => {
                    self.pps_seen = true;
                }
                NalType::IdrSlice => {
                    // If this is a new IDR picture following an IDR picture, reset the idr flag.
                    // Just check first_mb_in_slice to be 1
                    if !self.new_idr && nal[1] & 0x80 != 0 {
                        self.new_idr = true;
                    }
                    // insert SPS & PPS NAL units if they were not seen
                    if self.new_idr && !self.sps_seen && !self.pps_seen {
                        self.new_idr = false;
                        for sps in self.sequence_parameter_sets.iter() {
                            out.extend([0, 0, 1]);
                            out.extend(sps);
                        }
                        for pps in self.picture_parameter_sets.iter() {
                            out.extend([0, 0, 1]);
                            out.extend(pps);
                        }
                    }
                    // insert only PPS if SPS was seen
                    if self.new_idr && self.sps_seen && !self.pps_seen {
                        for pps in self.picture_parameter_sets.iter() {
                            out.extend([0, 0, 1]);
                            out.extend(pps);
                        }
                    }
                }
                _ => {}
            }

            out.extend([0, 0, 1]);
            out.extend(nal);

            if !self.new_idr && nal_type == NalType::Slice {
                self.new_idr = true;
                self.sps_seen = false;
                self.pps_seen = false;
            }
        }
    }
}
