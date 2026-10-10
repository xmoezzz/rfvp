use rfvp::host_api::{
    AudioParams, AudioStreamDesc, AudioStreamId, EncodedAudioKind, RfvpAudio, RfvpError, RfvpResult,
};

pub struct WiiUAudio;

impl WiiUAudio {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for WiiUAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpAudio for WiiUAudio {
    fn load_encoded(
        &mut self,
        _id: AudioStreamId,
        _kind: EncodedAudioKind,
        _bytes: &[u8],
    ) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn create_stream(&mut self, _id: AudioStreamId, _desc: AudioStreamDesc) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn submit_i16(&mut self, _id: AudioStreamId, _samples: &[i16]) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn submit_f32(&mut self, _id: AudioStreamId, _samples: &[f32]) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn play(
        &mut self,
        _id: AudioStreamId,
        _params: AudioParams,
        _fade_in_ms: u32,
    ) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn stop(&mut self, _id: AudioStreamId, _fade_ms: u32) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn pause(&mut self, _id: AudioStreamId) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn resume(&mut self, _id: AudioStreamId) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn set_params(&mut self, _id: AudioStreamId, _params: AudioParams) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn set_master_volume(&mut self, _volume: f32) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn destroy_stream(&mut self, _id: AudioStreamId) {}

    fn tick(&mut self, _delta_us: u64) -> RfvpResult<()> {
        Ok(())
    }
}
