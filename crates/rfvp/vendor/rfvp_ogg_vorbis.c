#include "rfvp_ogg_vorbis.h"

#include <limits.h>
#include <stdlib.h>

#define STB_VORBIS_NO_STDIO 1
#include "stb_vorbis.c"

struct RfvpOggVorbis {
    stb_vorbis *decoder;
    int channels;
};

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

    handle->decoder = decoder;
    handle->channels = info.channels;
    out_info->sample_rate = info.sample_rate;
    out_info->channels = (uint16_t)info.channels;
    *out_decoder = handle;
    return 0;
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
    free(decoder);
}
