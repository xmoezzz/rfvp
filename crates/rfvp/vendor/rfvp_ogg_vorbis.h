#ifndef RFVP_OGG_VORBIS_H
#define RFVP_OGG_VORBIS_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RfvpOggVorbis RfvpOggVorbis;

typedef struct RfvpOggVorbisInfo {
    uint32_t sample_rate;
    uint16_t channels;
} RfvpOggVorbisInfo;

int32_t rfvp_ogg_open_memory(
    const uint8_t *bytes,
    size_t byte_len,
    RfvpOggVorbisInfo *out_info,
    RfvpOggVorbis **out_decoder
);

/* Returns decoded interleaved i16 sample count, not frame count. */
int32_t rfvp_ogg_decode_interleaved_i16(
    RfvpOggVorbis *decoder,
    int16_t *out_samples,
    int32_t max_interleaved_samples
);

int32_t rfvp_ogg_seek_start(RfvpOggVorbis *decoder);

void rfvp_ogg_close(RfvpOggVorbis *decoder);

#ifdef __cplusplus
}
#endif

#endif
