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

/*
 * Reads up to `len` bytes at `offset` of a sound into `buf`; returns the count (less than `len`
 * only at the end) or a negative value on error.
 */
typedef int32_t (*RfvpOggReadFn)(void *user, uint64_t offset, uint8_t *buf, int32_t len);

/*
 * Open a sound read through `read` as it decodes, holding only a small buffer of it.
 * `user` is passed to `read` and must stay valid until rfvp_ogg_close.
 */
int32_t rfvp_ogg_open_stream(
    RfvpOggReadFn read,
    void *user,
    RfvpOggVorbisInfo *out_info,
    RfvpOggVorbis **out_decoder
);

int32_t rfvp_ogg_seek_start(RfvpOggVorbis *decoder);

void rfvp_ogg_close(RfvpOggVorbis *decoder);

#ifdef __cplusplus
}
#endif

#endif
