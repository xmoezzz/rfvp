#include "rfvp_ogg_vorbis.h"

#include <limits.h>
#include <stdlib.h>

#define STB_VORBIS_NO_STDIO 1
#include "stb_vorbis.c"

struct RfvpOggVorbis {
    stb_vorbis *decoder;
    int channels;
    /* Streamed sounds only (read != NULL): stb_vorbis's pushdata API over a small buffer. */
    RfvpOggReadFn read;
    void *user;
    uint8_t *buf;
    int32_t buf_len;
    int32_t buf_cap;
    /* Where the next read starts. */
    uint64_t file_pos;
    int eof;
    /* A decoded frame not yet handed out. */
    float **pending;
    int pending_frames;
    int pending_pos;
};

/* The buffer of a streamed sound; it grows (up to the limit) if the headers need more. */
#define RFVP_OGG_STREAM_BUF 32768
#define RFVP_OGG_STREAM_BUF_MAX (1024 * 1024)

int32_t rfvp_ogg_open_memory(
    const uint8_t *bytes,
    size_t byte_len,
    RfvpOggVorbisInfo *out_info,
    RfvpOggVorbis **out_decoder
) {
    if (bytes == NULL || byte_len == 0u || out_info == NULL || out_decoder == NULL) {
        return -1;
    }
    if (byte_len > (size_t)INT_MAX) {
        return -2;
    }

    int error = 0;
    stb_vorbis *decoder = stb_vorbis_open_memory(bytes, (int)byte_len, &error, NULL);
    if (decoder == NULL) {
        (void)error;
        return -3;
    }

    stb_vorbis_info info = stb_vorbis_get_info(decoder);
    if (info.channels <= 0 || info.sample_rate == 0u || info.channels > UINT16_MAX) {
        stb_vorbis_close(decoder);
        return -4;
    }

    RfvpOggVorbis *handle = (RfvpOggVorbis *)malloc(sizeof(RfvpOggVorbis));
    if (handle == NULL) {
        stb_vorbis_close(decoder);
        return -5;
    }

    memset(handle, 0, sizeof(*handle));
    handle->decoder = decoder;
    handle->channels = info.channels;
    out_info->sample_rate = info.sample_rate;
    out_info->channels = (uint16_t)info.channels;
    *out_decoder = handle;
    return 0;
}

/* Top the buffer up from the stream; returns 0, or -1 on a read error. */
static int rfvp_ogg_fill(RfvpOggVorbis *h) {
    if (h->eof || h->buf_len >= h->buf_cap) {
        return 0;
    }
    int32_t want = h->buf_cap - h->buf_len;
    int32_t n = h->read(h->user, h->file_pos, h->buf + h->buf_len, want);
    if (n < 0) {
        return -1;
    }
    h->buf_len += n;
    h->file_pos += (uint64_t)n;
    if (n < want) {
        h->eof = 1;
    }
    return 0;
}

static void rfvp_ogg_consume(RfvpOggVorbis *h, int used) {
    if (used <= 0) {
        return;
    }
    memmove(h->buf, h->buf + used, (size_t)(h->buf_len - used));
    h->buf_len -= used;
}

static int rfvp_ogg_grow(RfvpOggVorbis *h) {
    if (h->buf_cap >= RFVP_OGG_STREAM_BUF_MAX) {
        return -1;
    }
    uint8_t *bigger = (uint8_t *)malloc((size_t)h->buf_cap * 2u);
    if (bigger == NULL) {
        return -1;
    }
    memcpy(bigger, h->buf, (size_t)h->buf_len);
    free(h->buf);
    h->buf = bigger;
    h->buf_cap *= 2;
    return 0;
}

/* Read the headers from the start of the stream and open the decoder on them. */
static int rfvp_ogg_open_headers(RfvpOggVorbis *h) {
    h->file_pos = 0;
    h->buf_len = 0;
    h->eof = 0;
    h->pending_frames = 0;
    h->pending_pos = 0;
    for (;;) {
        if (rfvp_ogg_fill(h) != 0) {
            return -1;
        }
        int used = 0;
        int error = 0;
        h->decoder = stb_vorbis_open_pushdata(h->buf, h->buf_len, &used, &error, NULL);
        if (h->decoder != NULL) {
            rfvp_ogg_consume(h, used);
            return 0;
        }
        if (error != VORBIS_need_more_data || h->eof || rfvp_ogg_grow(h) != 0) {
            return -1;
        }
    }
}

int32_t rfvp_ogg_open_stream(
    RfvpOggReadFn read,
    void *user,
    RfvpOggVorbisInfo *out_info,
    RfvpOggVorbis **out_decoder
) {
    if (read == NULL || out_info == NULL || out_decoder == NULL) {
        return -1;
    }
    RfvpOggVorbis *h = (RfvpOggVorbis *)malloc(sizeof(RfvpOggVorbis));
    if (h == NULL) {
        return -5;
    }
    memset(h, 0, sizeof(*h));
    h->read = read;
    h->user = user;
    h->buf_cap = RFVP_OGG_STREAM_BUF;
    h->buf = (uint8_t *)malloc((size_t)h->buf_cap);
    if (h->buf == NULL) {
        free(h);
        return -5;
    }

    if (rfvp_ogg_open_headers(h) != 0) {
        free(h->buf);
        free(h);
        return -3;
    }

    stb_vorbis_info info = stb_vorbis_get_info(h->decoder);
    if (info.channels <= 0 || info.sample_rate == 0u || info.channels > UINT16_MAX) {
        rfvp_ogg_close(h);
        return -4;
    }
    h->channels = info.channels;
    out_info->sample_rate = info.sample_rate;
    out_info->channels = (uint16_t)info.channels;
    *out_decoder = h;
    return 0;
}

static inline int16_t rfvp_ogg_to_i16(float f) {
    /* Rounded, as stb_vorbis's own conversion does. */
    float scaled = f * 32768.0f;
    int v = (int)(scaled + (scaled >= 0.0f ? 0.5f : -0.5f));
    if (v > 32767) {
        return 32767;
    }
    if (v < -32768) {
        return -32768;
    }
    return (int16_t)v;
}

static int32_t rfvp_ogg_decode_stream(RfvpOggVorbis *h, int16_t *out, int32_t max_samples) {
    int channels = h->channels;
    int32_t max_frames = max_samples / channels;
    int32_t frames = 0;
    while (frames < max_frames) {
        if (h->pending_pos < h->pending_frames) {
            int n = h->pending_frames - h->pending_pos;
            if (n > max_frames - frames) {
                n = max_frames - frames;
            }
            for (int i = 0; i < n; i++) {
                for (int c = 0; c < channels; c++) {
                    out[(frames + i) * channels + c] =
                        rfvp_ogg_to_i16(h->pending[c][h->pending_pos + i]);
                }
            }
            h->pending_pos += n;
            frames += n;
            continue;
        }
        if (h->buf_len < h->buf_cap / 2 && rfvp_ogg_fill(h) != 0) {
            return -2;
        }
        int got_channels = 0;
        float **output = NULL;
        int samples = 0;
        int used = stb_vorbis_decode_frame_pushdata(
            h->decoder, h->buf, h->buf_len, &got_channels, &output, &samples
        );
        if (used == 0) {
            /* Not a whole page in the buffer: read more, or it is the end. */
            if (h->eof) {
                break;
            }
            if (h->buf_len >= h->buf_cap && rfvp_ogg_grow(h) != 0) {
                return -2;
            }
            if (rfvp_ogg_fill(h) != 0) {
                return -2;
            }
            continue;
        }
        rfvp_ogg_consume(h, used);
        if (samples > 0) {
            h->pending = output;
            h->pending_frames = samples;
            h->pending_pos = 0;
        }
    }
    return frames * channels;
}

int32_t rfvp_ogg_decode_interleaved_i16(
    RfvpOggVorbis *decoder,
    int16_t *out_samples,
    int32_t max_interleaved_samples
) {
    if (decoder == NULL || decoder->decoder == NULL || out_samples == NULL || max_interleaved_samples < 0) {
        return -1;
    }
    if (max_interleaved_samples == 0) {
        return 0;
    }
    if (decoder->read != NULL) {
        return rfvp_ogg_decode_stream(decoder, out_samples, max_interleaved_samples);
    }

    int frames = stb_vorbis_get_samples_short_interleaved(
        decoder->decoder,
        decoder->channels,
        out_samples,
        max_interleaved_samples
    );
    if (frames < 0) {
        return -2;
    }
    return frames * decoder->channels;
}

int32_t rfvp_ogg_seek_start(RfvpOggVorbis *decoder) {
    if (decoder == NULL || decoder->decoder == NULL) {
        return -1;
    }
    if (decoder->read != NULL) {
        /* Start over: after stb_vorbis_flush_pushdata the decoder skips the first audio page
         * while it finds its place, which would cut the start of a looping song. */
        stb_vorbis_close(decoder->decoder);
        decoder->decoder = NULL;
        return rfvp_ogg_open_headers(decoder) == 0 ? 0 : -2;
    }
    return stb_vorbis_seek_start(decoder->decoder) ? 0 : -2;
}

void rfvp_ogg_close(RfvpOggVorbis *decoder) {
    if (decoder == NULL) {
        return;
    }
    if (decoder->decoder != NULL) {
        stb_vorbis_close(decoder->decoder);
        decoder->decoder = NULL;
    }
    free(decoder->buf);
    free(decoder);
}
