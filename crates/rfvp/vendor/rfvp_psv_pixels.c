/* SPDX-License-Identifier: MPL-2.0
 * Exact NVSG channel conversion for the Vita's ARM NEON. No alpha math,
 * filtering, quantization, dimensions or game-specific resource assumptions.
 * The caller provides nonoverlapping buffers sized for count packed pixels.
 */
#include <arm_neon.h>
#include <stddef.h>
#include <stdint.h>

void rfvp_psv_bgra_to_rgba(const uint8_t *src, uint8_t *dst, size_t count) {
    while (count >= 16) {
        uint8x16x4_t pixels = vld4q_u8(src);
        uint8x16_t blue = pixels.val[0];
        pixels.val[0] = pixels.val[2];
        pixels.val[2] = blue;
        vst4q_u8(dst, pixels);
        src += 64;
        dst += 64;
        count -= 16;
    }
    while (count-- != 0) {
        dst[0] = src[2]; dst[1] = src[1]; dst[2] = src[0]; dst[3] = src[3];
        src += 4; dst += 4;
    }
}

void rfvp_psv_bgr_to_rgba(const uint8_t *src, uint8_t *dst, size_t count) {
    while (count >= 16) {
        uint8x16x3_t input = vld3q_u8(src);
        uint8x16x4_t pixels;
        pixels.val[0] = input.val[2];
        pixels.val[1] = input.val[1];
        pixels.val[2] = input.val[0];
        pixels.val[3] = vdupq_n_u8(255);
        vst4q_u8(dst, pixels);
        src += 48;
        dst += 64;
        count -= 16;
    }
    while (count-- != 0) {
        dst[0] = src[2]; dst[1] = src[1]; dst[2] = src[0]; dst[3] = 255;
        src += 3; dst += 4;
    }
}
