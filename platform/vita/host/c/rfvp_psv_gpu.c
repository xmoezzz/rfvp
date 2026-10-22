/* The Vita renderer: the engine's sprites drawn with the GPU through vita2d.
 *
 * Textures are GPU textures (the engine hands them over already shrunk to the screen scale);
 * a sprite is a textured triangle strip with one tint, alpha or additive blended, optionally
 * clipped to a rectangle. vita2d owns the display buffers, presents at vblank and paces the game
 * loop. */

#include <stdint.h>
#include <stddef.h>
#include <string.h>

#include <psp2/display.h>
#include <psp2/gxm.h>
#include <psp2/kernel/processmgr.h>
#include <psp2/kernel/sysmem.h>
#include <vita2d.h>

#define RFVP_GPU_SCREEN_WIDTH 960
#define RFVP_GPU_SCREEN_HEIGHT 544
/* vita2d's per-frame pool, which the vertex data of the frame's draws comes from. */
#define RFVP_GPU_POOL_SIZE (2 * 1024 * 1024)
/* Textures freed while the GPU may still read them wait here for the end of the frame. */
#define RFVP_GPU_MAX_DEFERRED_FREES 256

static int g_gpu_ready;

/* vita2d's shaders and the fragment programs its draw calls use (libvita2d globals). */
extern const SceGxmProgram color_v_gxp_start;
extern const SceGxmProgram color_f_gxp_start;
extern const SceGxmProgram texture_v_gxp_start;
extern const SceGxmProgram texture_tint_f_gxp_start;
extern SceGxmFragmentProgram *_vita2d_colorFragmentProgram;
extern SceGxmFragmentProgram *_vita2d_textureTintFragmentProgram;

/* Additive blending as the engine (and the original exe) does it: SRC_ALPHA / ONE, leaving the
 * destination alpha alone. vita2d's own "add" mode is ONE / ONE, which ignores the source alpha
 * and turns a faint full-screen light layer into solid white. */
static SceGxmFragmentProgram *g_add_texture_tint;
static SceGxmFragmentProgram *g_add_color;

static int rfvp_gpu_make_additive_programs(void) {
    static const SceGxmBlendInfo additive = {
        .colorFunc = SCE_GXM_BLEND_FUNC_ADD,
        .alphaFunc = SCE_GXM_BLEND_FUNC_ADD,
        .colorSrc = SCE_GXM_BLEND_FACTOR_SRC_ALPHA,
        .colorDst = SCE_GXM_BLEND_FACTOR_ONE,
        .alphaSrc = SCE_GXM_BLEND_FACTOR_ZERO,
        .alphaDst = SCE_GXM_BLEND_FACTOR_ONE,
        .colorMask = SCE_GXM_COLOR_MASK_ALL,
    };
    SceGxmShaderPatcher *patcher = vita2d_get_shader_patcher();
    SceGxmShaderPatcherId tint_id;
    SceGxmShaderPatcherId color_id;
    if (sceGxmShaderPatcherRegisterProgram(patcher, &texture_tint_f_gxp_start, &tint_id) < 0 ||
        sceGxmShaderPatcherRegisterProgram(patcher, &color_f_gxp_start, &color_id) < 0) {
        return -1;
    }
    if (sceGxmShaderPatcherCreateFragmentProgram(patcher, tint_id, SCE_GXM_OUTPUT_REGISTER_FORMAT_UCHAR4,
                                                 SCE_GXM_MULTISAMPLE_NONE, &additive, &texture_v_gxp_start,
                                                 &g_add_texture_tint) < 0 ||
        sceGxmShaderPatcherCreateFragmentProgram(patcher, color_id, SCE_GXM_OUTPUT_REGISTER_FORMAT_UCHAR4,
                                                 SCE_GXM_MULTISAMPLE_NONE, &additive, &color_v_gxp_start,
                                                 &g_add_color) < 0) {
        return -1;
    }
    return 0;
}
static vita2d_texture *g_deferred_frees[RFVP_GPU_MAX_DEFERRED_FREES];
static unsigned g_deferred_free_count;
static uint64_t g_present_us;
static uint64_t g_draw_calls;
/* The display buffer the last frame was drawn into: on screen after the swap, and intact until
 * the next frame is drawn. */
static const void *g_last_frame;

int32_t rfvp_psv_gpu_init(void) {
    if (g_gpu_ready) {
        return 0;
    }
    if (vita2d_init_advanced(RFVP_GPU_POOL_SIZE) < 0) {
        return -9;
    }
    vita2d_set_clear_color(0xFF000000u);
    vita2d_set_vblank_wait(1);
    /* Big textures in CDRAM, small ones (text, icons) in plain memory, so a small texture does
     * not take a whole 256 KiB CDRAM unit. */
    vita2d_texture_set_alloc_memblock_type(SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW);
    if (rfvp_gpu_make_additive_programs() != 0) {
        vita2d_fini();
        return -9;
    }
    g_gpu_ready = 1;
    return 0;
}

static void rfvp_gpu_free_deferred(void) {
    for (unsigned i = 0; i < g_deferred_free_count; ++i) {
        vita2d_free_texture(g_deferred_frees[i]);
    }
    g_deferred_free_count = 0;
}

void rfvp_psv_gpu_fini(void) {
    if (!g_gpu_ready) {
        return;
    }
    vita2d_wait_rendering_done();
    rfvp_gpu_free_deferred();
    vita2d_fini();
    g_gpu_ready = 0;
}

/* An RGBA8 texture (bytes r, g, b, a; rows `width` pixels apart), or NULL. */
void *rfvp_psv_gpu_create_texture(uint32_t width, uint32_t height, const uint8_t *rgba, int linear) {
    if (!g_gpu_ready || width == 0 || height == 0) {
        return NULL;
    }
    const size_t bytes = (size_t)width * (size_t)height * 4u;
    vita2d_texture_set_alloc_memblock_type(bytes >= 256u * 1024u ? SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW
                                                                 : SCE_KERNEL_MEMBLOCK_TYPE_USER_RW_UNCACHE);
    vita2d_texture *texture =
        vita2d_create_empty_texture_format(width, height, SCE_GXM_TEXTURE_FORMAT_U8U8U8U8_ABGR);
    if (texture == NULL) {
        return NULL;
    }
    /* vita2d leaves these unset for a plain texture, and vita2d_free_texture frees them when they
     * are not zero: an unrelated memory block could go with it. */
    texture->gxm_rtgt = NULL;
    texture->depth_UID = 0;
    texture->palette_UID = 0;
    const SceGxmTextureFilter filter = linear ? SCE_GXM_TEXTURE_FILTER_LINEAR : SCE_GXM_TEXTURE_FILTER_POINT;
    vita2d_texture_set_filters(texture, filter, filter);

    if (rgba != NULL) {
        uint8_t *dst = (uint8_t *)vita2d_texture_get_datap(texture);
        const unsigned stride = vita2d_texture_get_stride(texture);
        for (uint32_t y = 0; y < height; ++y) {
            memcpy(dst + (size_t)y * stride, rgba + (size_t)y * width * 4u, (size_t)width * 4u);
        }
    }
    return texture;
}

/* Replace a rectangle of a texture with RGBA8 pixels (rows `w` pixels apart). */
void rfvp_psv_gpu_update_texture(void *handle, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                                 const uint8_t *rgba) {
    vita2d_texture *texture = (vita2d_texture *)handle;
    if (texture == NULL || rgba == NULL) {
        return;
    }
    const uint32_t tw = vita2d_texture_get_width(texture);
    const uint32_t th = vita2d_texture_get_height(texture);
    if (x >= tw || y >= th) {
        return;
    }
    if (x + w > tw) {
        w = tw - x;
    }
    if (y + h > th) {
        h = th - y;
    }
    uint8_t *dst = (uint8_t *)vita2d_texture_get_datap(texture);
    const unsigned stride = vita2d_texture_get_stride(texture);
    for (uint32_t row = 0; row < h; ++row) {
        memcpy(dst + (size_t)(y + row) * stride + (size_t)x * 4u, rgba + (size_t)row * w * 4u, (size_t)w * 4u);
    }
}

void rfvp_psv_gpu_destroy_texture(void *handle) {
    if (handle == NULL) {
        return;
    }
    if (g_deferred_free_count == RFVP_GPU_MAX_DEFERRED_FREES) {
        vita2d_wait_rendering_done();
        rfvp_gpu_free_deferred();
    }
    g_deferred_frees[g_deferred_free_count++] = (vita2d_texture *)handle;
}

void rfvp_psv_gpu_begin_frame(void) {
    if (!g_gpu_ready) {
        return;
    }
    vita2d_start_drawing();
    vita2d_clear_screen();
}

/* Clip the following draws to a rectangle of the screen, or stop clipping. */
void rfvp_psv_gpu_set_clip(int enabled, int x0, int y0, int x1, int y1) {
    if (enabled) {
        vita2d_set_clip_rectangle(x0, y0, x1, y1);
        if (!vita2d_get_clipping_enabled()) {
            vita2d_enable_clipping();
        }
    } else if (vita2d_get_clipping_enabled()) {
        vita2d_disable_clipping();
    }
}

/* A sprite: four corners in triangle-strip order, each `x, y, u, v` (screen pixels, normalized
 * texture coordinates), with one tint (bytes r, g, b, a). */
void rfvp_psv_gpu_draw_sprite(void *handle, const float *corners, uint32_t tint_rgba, int additive) {
    vita2d_texture *texture = (vita2d_texture *)handle;
    if (texture == NULL || corners == NULL) {
        return;
    }
    vita2d_texture_vertex *vertices =
        (vita2d_texture_vertex *)vita2d_pool_memalign(4 * sizeof(vita2d_texture_vertex), sizeof(vita2d_texture_vertex));
    if (vertices == NULL) {
        return;
    }
    for (int i = 0; i < 4; ++i) {
        vertices[i].x = corners[i * 4 + 0];
        vertices[i].y = corners[i * 4 + 1];
        vertices[i].z = 0.5f;
        vertices[i].u = corners[i * 4 + 2];
        vertices[i].v = corners[i * 4 + 3];
    }
    SceGxmFragmentProgram *normal = _vita2d_textureTintFragmentProgram;
    if (additive) {
        _vita2d_textureTintFragmentProgram = g_add_texture_tint;
    }
    vita2d_draw_array_textured(texture, SCE_GXM_PRIMITIVE_TRIANGLE_STRIP, vertices, 4, tint_rgba);
    _vita2d_textureTintFragmentProgram = normal;
    ++g_draw_calls;
}

void rfvp_psv_gpu_draw_rect(float x, float y, float w, float h, uint32_t rgba, int additive) {
    SceGxmFragmentProgram *normal = _vita2d_colorFragmentProgram;
    if (additive) {
        _vita2d_colorFragmentProgram = g_add_color;
    }
    vita2d_draw_rectangle(x, y, w, h, rgba);
    _vita2d_colorFragmentProgram = normal;
    ++g_draw_calls;
}

/* Finish the frame's drawing and wait for the GPU, so textures freed during the frame can go
 * and the frame can be read back for a save thumbnail. */
void rfvp_psv_gpu_end_frame(void) {
    if (!g_gpu_ready) {
        return;
    }
    rfvp_psv_gpu_set_clip(0, 0, 0, 0, 0);
    vita2d_end_drawing();
    vita2d_wait_rendering_done();
    rfvp_gpu_free_deferred();
    g_last_frame = vita2d_get_current_fb();
}

/* The frame just drawn, as RGBA8 (960x544, rows 960 pixels apart). */
const uint8_t *rfvp_psv_gpu_frame_pixels(void) {
    return (const uint8_t *)g_last_frame;
}

/* Show the frame from the next vblank on, waiting for it. */
void rfvp_psv_gpu_present(void) {
    if (!g_gpu_ready) {
        return;
    }
    const uint64_t start = sceKernelGetProcessTimeWide();
    vita2d_swap_buffers();
    g_present_us += sceKernelGetProcessTimeWide() - start;
}

/* Time spent waiting to present and draw calls made since the last call, for the debug report. */
void rfvp_psv_gpu_stats(uint64_t *present_us, uint64_t *draw_calls) {
    *present_us = g_present_us;
    *draw_calls = g_draw_calls;
    g_present_us = 0;
    g_draw_calls = 0;
}
