#include <switch.h>
#include <dirent.h>
#include <EGL/egl.h>
#include <GLES2/gl2.h>
#include <math.h>
#include <malloc.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "rfvp_switch_host.h"

#define RFVP_FB_WIDTH 1280u
#define RFVP_FB_HEIGHT 720u

#define RFVP_GPU_TEXTURES_MAX 4096u
#define RFVP_AUDIO_BUFFER_COUNT 4u
#define RFVP_AUDIO_FRAME_COUNT 1024u
#define RFVP_AUDIO_CHANNELS 2u
#define RFVP_AUDIO_SAMPLE_COUNT (RFVP_AUDIO_FRAME_COUNT * RFVP_AUDIO_CHANNELS)
#define RFVP_AUDIO_BUFFER_BYTES (RFVP_AUDIO_SAMPLE_COUNT * sizeof(int16_t))

typedef struct RfvpGpuTexture {
    uint32_t id;
    uint32_t width;
    uint32_t height;
    uint64_t generation;
    GLuint gl_id;
} RfvpGpuTexture;

typedef struct RfvpGpuRenderer {
    int initialized;
    EGLDisplay display;
    EGLContext context;
    EGLSurface surface;
    GLuint program;
    GLuint vbo;
    GLuint white_texture;
    GLint attr_pos;
    GLint attr_uv;
    GLint attr_color;
    GLint uniform_texture;
    RfvpGpuTexture textures[RFVP_GPU_TEXTURES_MAX];
} RfvpGpuRenderer;

typedef struct RfvpSwitchAudioOut {
    int initialized;
    AudioOutBuffer buffers[RFVP_AUDIO_BUFFER_COUNT];
    void *buffer_mem[RFVP_AUDIO_BUFFER_COUNT];
} RfvpSwitchAudioOut;

typedef struct RfvpGpuVertex {
    float x;
    float y;
    float u;
    float v;
    float r;
    float g;
    float b;
    float a;
} RfvpGpuVertex;

static RfvpGpuRenderer g_renderer;
static RfvpSwitchAudioOut g_audio;

static const char *RFVP_VERTEX_SHADER_SRC =
    "attribute vec2 a_pos;\n"
    "attribute vec2 a_uv;\n"
    "attribute vec4 a_color;\n"
    "varying vec2 v_uv;\n"
    "varying vec4 v_color;\n"
    "void main() {\n"
    "    gl_Position = vec4(a_pos, 0.0, 1.0);\n"
    "    v_uv = a_uv;\n"
    "    v_color = a_color;\n"
    "}\n";

static const char *RFVP_FRAGMENT_SHADER_SRC =
    "precision mediump float;\n"
    "varying vec2 v_uv;\n"
    "varying vec4 v_color;\n"
    "uniform sampler2D u_tex;\n"
    "void main() {\n"
    "    gl_FragColor = texture2D(u_tex, v_uv) * v_color;\n"
    "}\n";

static int directory_readable(const char *path) {
    DIR *dir = opendir(path);
    if (dir == NULL) {
        return 0;
    }
    closedir(dir);
    return 1;
}

static void print_directory_probe(const char *path) {
    printf("\nGame root probe: %s\n", path);

    DIR *dir = opendir(path);
    if (dir == NULL) {
        printf("  not found or not readable\n");
        return;
    }

    printf("  readable. First entries:\n");
    int count = 0;
    struct dirent *ent = NULL;
    while ((ent = readdir(dir)) != NULL && count < 12) {
        if (strcmp(ent->d_name, ".") == 0 || strcmp(ent->d_name, "..") == 0) {
            continue;
        }
        printf("  - %s\n", ent->d_name);
        count++;
    }

    if (count == 0) {
        printf("  directory is empty\n");
    }

    closedir(dir);
}

static const char *select_game_root(void) {
    if (directory_readable("sdmc:/rfvp")) {
        return "sdmc:/rfvp";
    }
    if (directory_readable("sdmc:/switch/rfvp")) {
        return "sdmc:/switch/rfvp";
    }
    return "sdmc:/rfvp";
}

static uint32_t map_switch_buttons(u64 buttons) {
    uint32_t out = 0;
    if ((buttons & HidNpadButton_A) != 0) out |= RFVP_SWITCH_BUTTON_A;
    if ((buttons & HidNpadButton_B) != 0) out |= RFVP_SWITCH_BUTTON_B;
    if ((buttons & HidNpadButton_X) != 0) out |= RFVP_SWITCH_BUTTON_X;
    if ((buttons & HidNpadButton_Y) != 0) out |= RFVP_SWITCH_BUTTON_Y;
    if ((buttons & HidNpadButton_L) != 0) out |= RFVP_SWITCH_BUTTON_L;
    if ((buttons & HidNpadButton_R) != 0) out |= RFVP_SWITCH_BUTTON_R;
    if ((buttons & HidNpadButton_ZL) != 0) out |= RFVP_SWITCH_BUTTON_ZL;
    if ((buttons & HidNpadButton_ZR) != 0) out |= RFVP_SWITCH_BUTTON_ZR;
    if ((buttons & HidNpadButton_Plus) != 0) out |= RFVP_SWITCH_BUTTON_PLUS;
    if ((buttons & HidNpadButton_Minus) != 0) out |= RFVP_SWITCH_BUTTON_MINUS;
    if ((buttons & HidNpadButton_Up) != 0) out |= RFVP_SWITCH_BUTTON_UP;
    if ((buttons & HidNpadButton_Down) != 0) out |= RFVP_SWITCH_BUTTON_DOWN;
    if ((buttons & HidNpadButton_Left) != 0) out |= RFVP_SWITCH_BUTTON_LEFT;
    if ((buttons & HidNpadButton_Right) != 0) out |= RFVP_SWITCH_BUTTON_RIGHT;
    return out;
}

static float clamp_f32(float v, float lo, float hi) {
    if (v < lo) return lo;
    if (v > hi) return hi;
    return v;
}

static RfvpGpuTexture *find_texture(uint32_t id) {
    if (id == 0) {
        return NULL;
    }

    for (uint32_t i = 0; i < RFVP_GPU_TEXTURES_MAX; i++) {
        if (g_renderer.textures[i].id == id) {
            return &g_renderer.textures[i];
        }
    }

    return NULL;
}

static RfvpGpuTexture *alloc_texture(uint32_t id) {
    RfvpGpuTexture *tex = find_texture(id);
    if (tex != NULL) {
        return tex;
    }

    for (uint32_t i = 0; i < RFVP_GPU_TEXTURES_MAX; i++) {
        if (g_renderer.textures[i].id == 0) {
            g_renderer.textures[i].id = id;
            return &g_renderer.textures[i];
        }
    }

    return NULL;
}

static void transform_point(const RfvpSwitchMat4F32 *m, float x, float y, float *out_x, float *out_y) {
    *out_x = m->cols[0][0] * x + m->cols[1][0] * y + m->cols[3][0];
    *out_y = m->cols[0][1] * x + m->cols[1][1] * y + m->cols[3][1];
}

static float pixel_to_ndc_x(float x) {
    return (x / (float)RFVP_FB_WIDTH) * 2.0f - 1.0f;
}

static float pixel_to_ndc_y(float y) {
    return 1.0f - (y / (float)RFVP_FB_HEIGHT) * 2.0f;
}

static GLuint compile_shader(GLenum kind, const char *src) {
    GLuint shader = glCreateShader(kind);
    glShaderSource(shader, 1, &src, NULL);
    glCompileShader(shader);

    GLint ok = GL_FALSE;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (ok != GL_TRUE) {
        GLchar log_buf[1024];
        GLsizei len = 0;
        glGetShaderInfoLog(shader, sizeof(log_buf), &len, log_buf);
        printf("GL shader compile failed: %.*s\n", (int)len, log_buf);
        glDeleteShader(shader);
        return 0;
    }

    return shader;
}

static GLuint create_program(void) {
    GLuint vs = compile_shader(GL_VERTEX_SHADER, RFVP_VERTEX_SHADER_SRC);
    GLuint fs = compile_shader(GL_FRAGMENT_SHADER, RFVP_FRAGMENT_SHADER_SRC);
    if (vs == 0 || fs == 0) {
        if (vs != 0) glDeleteShader(vs);
        if (fs != 0) glDeleteShader(fs);
        return 0;
    }

    GLuint program = glCreateProgram();
    glAttachShader(program, vs);
    glAttachShader(program, fs);
    glBindAttribLocation(program, 0, "a_pos");
    glBindAttribLocation(program, 1, "a_uv");
    glBindAttribLocation(program, 2, "a_color");
    glLinkProgram(program);

    glDeleteShader(vs);
    glDeleteShader(fs);

    GLint ok = GL_FALSE;
    glGetProgramiv(program, GL_LINK_STATUS, &ok);
    if (ok != GL_TRUE) {
        GLchar log_buf[1024];
        GLsizei len = 0;
        glGetProgramInfoLog(program, sizeof(log_buf), &len, log_buf);
        printf("GL program link failed: %.*s\n", (int)len, log_buf);
        glDeleteProgram(program);
        return 0;
    }

    return program;
}

static int renderer_init(void) {
    memset(&g_renderer, 0, sizeof(g_renderer));

    static const EGLint config_attribs[] = {
        EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT,
        EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
        EGL_RED_SIZE, 8,
        EGL_GREEN_SIZE, 8,
        EGL_BLUE_SIZE, 8,
        EGL_ALPHA_SIZE, 8,
        EGL_DEPTH_SIZE, 0,
        EGL_STENCIL_SIZE, 0,
        EGL_NONE
    };
    static const EGLint context_attribs[] = {
        EGL_CONTEXT_CLIENT_VERSION, 2,
        EGL_NONE
    };

    g_renderer.display = eglGetDisplay(EGL_DEFAULT_DISPLAY);
    if (g_renderer.display == EGL_NO_DISPLAY) {
        return -1;
    }
    if (!eglInitialize(g_renderer.display, NULL, NULL)) {
        return -2;
    }
    if (!eglBindAPI(EGL_OPENGL_ES_API)) {
        return -3;
    }

    EGLConfig config = NULL;
    EGLint config_count = 0;
    if (!eglChooseConfig(g_renderer.display, config_attribs, &config, 1, &config_count) || config_count == 0) {
        return -4;
    }

    g_renderer.context = eglCreateContext(g_renderer.display, config, EGL_NO_CONTEXT, context_attribs);
    if (g_renderer.context == EGL_NO_CONTEXT) {
        return -5;
    }

    g_renderer.surface = eglCreateWindowSurface(g_renderer.display, config, nwindowGetDefault(), NULL);
    if (g_renderer.surface == EGL_NO_SURFACE) {
        return -6;
    }

    if (!eglMakeCurrent(g_renderer.display, g_renderer.surface, g_renderer.surface, g_renderer.context)) {
        return -7;
    }
    eglSwapInterval(g_renderer.display, 1);

    g_renderer.program = create_program();
    if (g_renderer.program == 0) {
        return -8;
    }

    g_renderer.attr_pos = glGetAttribLocation(g_renderer.program, "a_pos");
    g_renderer.attr_uv = glGetAttribLocation(g_renderer.program, "a_uv");
    g_renderer.attr_color = glGetAttribLocation(g_renderer.program, "a_color");
    g_renderer.uniform_texture = glGetUniformLocation(g_renderer.program, "u_tex");
    if (g_renderer.attr_pos < 0 || g_renderer.attr_uv < 0 || g_renderer.attr_color < 0 || g_renderer.uniform_texture < 0) {
        return -9;
    }

    glGenBuffers(1, &g_renderer.vbo);
    glGenTextures(1, &g_renderer.white_texture);
    glBindTexture(GL_TEXTURE_2D, g_renderer.white_texture);
    const uint8_t white[4] = {255, 255, 255, 255};
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, 1, 1, 0, GL_RGBA, GL_UNSIGNED_BYTE, white);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);

    glViewport(0, 0, RFVP_FB_WIDTH, RFVP_FB_HEIGHT);
    glDisable(GL_DEPTH_TEST);
    glDisable(GL_CULL_FACE);
    glEnable(GL_BLEND);
    glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
    glUseProgram(g_renderer.program);
    glUniform1i(g_renderer.uniform_texture, 0);

    g_renderer.initialized = 1;
    return 0;
}

static void renderer_shutdown(void) {
    if (!g_renderer.initialized) {
        return;
    }

    for (uint32_t i = 0; i < RFVP_GPU_TEXTURES_MAX; i++) {
        if (g_renderer.textures[i].gl_id != 0) {
            glDeleteTextures(1, &g_renderer.textures[i].gl_id);
            g_renderer.textures[i].gl_id = 0;
        }
    }
    if (g_renderer.white_texture != 0) {
        glDeleteTextures(1, &g_renderer.white_texture);
        g_renderer.white_texture = 0;
    }
    if (g_renderer.vbo != 0) {
        glDeleteBuffers(1, &g_renderer.vbo);
        g_renderer.vbo = 0;
    }
    if (g_renderer.program != 0) {
        glDeleteProgram(g_renderer.program);
        g_renderer.program = 0;
    }

    eglMakeCurrent(g_renderer.display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    if (g_renderer.surface != EGL_NO_SURFACE) {
        eglDestroySurface(g_renderer.display, g_renderer.surface);
    }
    if (g_renderer.context != EGL_NO_CONTEXT) {
        eglDestroyContext(g_renderer.display, g_renderer.context);
    }
    if (g_renderer.display != EGL_NO_DISPLAY) {
        eglTerminate(g_renderer.display);
    }
    memset(&g_renderer, 0, sizeof(g_renderer));
}

static void upload_texture_rgba8(const RfvpSwitchTextureUploadRgba8 *upload) {
    if (upload == NULL || upload->desc.id.value == 0 || upload->desc.width == 0 || upload->desc.height == 0) {
        return;
    }
    if (upload->data == NULL || upload->byte_len == 0) {
        return;
    }

    const size_t expected = (size_t)upload->desc.width * (size_t)upload->desc.height * 4u;
    if (upload->byte_len != expected) {
        return;
    }

    RfvpGpuTexture *tex = alloc_texture(upload->desc.id.value);
    if (tex == NULL) {
        return;
    }

    if (tex->gl_id == 0) {
        glGenTextures(1, &tex->gl_id);
    }

    glBindTexture(GL_TEXTURE_2D, tex->gl_id);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    glPixelStorei(GL_UNPACK_ALIGNMENT, 1);

    if (tex->width != upload->desc.width || tex->height != upload->desc.height) {
        glTexImage2D(
            GL_TEXTURE_2D,
            0,
            GL_RGBA,
            (GLsizei)upload->desc.width,
            (GLsizei)upload->desc.height,
            0,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            upload->data
        );
    } else if (tex->generation != upload->generation) {
        glTexSubImage2D(
            GL_TEXTURE_2D,
            0,
            0,
            0,
            (GLsizei)upload->desc.width,
            (GLsizei)upload->desc.height,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            upload->data
        );
    } else {
        return;
    }

    tex->width = upload->desc.width;
    tex->height = upload->desc.height;
    tex->generation = upload->generation;
}

static void set_vertex(RfvpGpuVertex *v, float x, float y, float u, float t, const RfvpSwitchColorF32 *color) {
    v->x = pixel_to_ndc_x(x);
    v->y = pixel_to_ndc_y(y);
    v->u = u;
    v->v = t;
    v->r = clamp_f32(color->r, 0.0f, 1.0f);
    v->g = clamp_f32(color->g, 0.0f, 1.0f);
    v->b = clamp_f32(color->b, 0.0f, 1.0f);
    v->a = clamp_f32(color->a, 0.0f, 1.0f);
}

static void draw_quad_vertices(GLuint texture, const RfvpGpuVertex vertices[6]) {
    glActiveTexture(GL_TEXTURE0);
    glBindTexture(GL_TEXTURE_2D, texture);
    glBindBuffer(GL_ARRAY_BUFFER, g_renderer.vbo);
    glBufferData(GL_ARRAY_BUFFER, sizeof(RfvpGpuVertex) * 6, vertices, GL_STREAM_DRAW);

    glEnableVertexAttribArray((GLuint)g_renderer.attr_pos);
    glEnableVertexAttribArray((GLuint)g_renderer.attr_uv);
    glEnableVertexAttribArray((GLuint)g_renderer.attr_color);
    glVertexAttribPointer((GLuint)g_renderer.attr_pos, 2, GL_FLOAT, GL_FALSE, sizeof(RfvpGpuVertex), (const void *)offsetof(RfvpGpuVertex, x));
    glVertexAttribPointer((GLuint)g_renderer.attr_uv, 2, GL_FLOAT, GL_FALSE, sizeof(RfvpGpuVertex), (const void *)offsetof(RfvpGpuVertex, u));
    glVertexAttribPointer((GLuint)g_renderer.attr_color, 4, GL_FLOAT, GL_FALSE, sizeof(RfvpGpuVertex), (const void *)offsetof(RfvpGpuVertex, r));
    glDrawArrays(GL_TRIANGLES, 0, 6);
}

static void draw_textured_quad_gpu(const RfvpSwitchTexturedQuad *quad) {
    RfvpGpuTexture *tex = find_texture(quad->texture.value);
    if (tex == NULL || tex->gl_id == 0 || tex->width == 0 || tex->height == 0) {
        return;
    }
    if (quad->dst.w == 0.0f || quad->dst.h == 0.0f) {
        return;
    }

    const float x0 = quad->dst.x;
    const float y0 = quad->dst.y;
    const float x1 = quad->dst.x + quad->dst.w;
    const float y1 = quad->dst.y + quad->dst.h;
    const float u0 = quad->uv.x;
    const float v0 = quad->uv.y;
    const float u1 = quad->uv.x + quad->uv.w;
    const float v1 = quad->uv.y + quad->uv.h;

    float px0, py0, px1, py1, px2, py2, px3, py3;
    transform_point(&quad->transform, x0, y0, &px0, &py0);
    transform_point(&quad->transform, x1, y0, &px1, &py1);
    transform_point(&quad->transform, x1, y1, &px2, &py2);
    transform_point(&quad->transform, x0, y1, &px3, &py3);

    RfvpGpuVertex vertices[6];
    set_vertex(&vertices[0], px0, py0, u0, v0, &quad->color);
    set_vertex(&vertices[1], px1, py1, u1, v0, &quad->color);
    set_vertex(&vertices[2], px2, py2, u1, v1, &quad->color);
    set_vertex(&vertices[3], px0, py0, u0, v0, &quad->color);
    set_vertex(&vertices[4], px2, py2, u1, v1, &quad->color);
    set_vertex(&vertices[5], px3, py3, u0, v1, &quad->color);
    draw_quad_vertices(tex->gl_id, vertices);
}

static void draw_fill_quad_gpu(const RfvpSwitchFillQuad *quad) {
    if (quad->dst.w == 0.0f || quad->dst.h == 0.0f) {
        return;
    }

    const float x0 = quad->dst.x;
    const float y0 = quad->dst.y;
    const float x1 = quad->dst.x + quad->dst.w;
    const float y1 = quad->dst.y + quad->dst.h;

    float px0, py0, px1, py1, px2, py2, px3, py3;
    transform_point(&quad->transform, x0, y0, &px0, &py0);
    transform_point(&quad->transform, x1, y0, &px1, &py1);
    transform_point(&quad->transform, x1, y1, &px2, &py2);
    transform_point(&quad->transform, x0, y1, &px3, &py3);

    RfvpGpuVertex vertices[6];
    set_vertex(&vertices[0], px0, py0, 0.0f, 0.0f, &quad->color);
    set_vertex(&vertices[1], px1, py1, 1.0f, 0.0f, &quad->color);
    set_vertex(&vertices[2], px2, py2, 1.0f, 1.0f, &quad->color);
    set_vertex(&vertices[3], px0, py0, 0.0f, 0.0f, &quad->color);
    set_vertex(&vertices[4], px2, py2, 1.0f, 1.0f, &quad->color);
    set_vertex(&vertices[5], px3, py3, 0.0f, 1.0f, &quad->color);
    draw_quad_vertices(g_renderer.white_texture, vertices);
}

static void renderer_draw_current_frame(void) {
    if (!g_renderer.initialized) {
        return;
    }

    glViewport(0, 0, RFVP_FB_WIDTH, RFVP_FB_HEIGHT);
    glUseProgram(g_renderer.program);
    glUniform1i(g_renderer.uniform_texture, 0);

    const RfvpSwitchRenderCommand *commands = rfvp_switch_host_global_render_commands();
    const size_t command_count = rfvp_switch_host_global_render_command_count();

    glClearColor(0.0f, 0.0f, 0.0f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);

    if (commands != NULL) {
        for (size_t i = 0; i < command_count; i++) {
            const RfvpSwitchRenderCommand *cmd = &commands[i];
            switch (cmd->kind) {
                case RFVP_SWITCH_RENDER_BEGIN_FRAME:
                case RFVP_SWITCH_RENDER_END_FRAME:
                case RFVP_SWITCH_RENDER_NONE:
                    break;
                case RFVP_SWITCH_RENDER_CLEAR:
                    glClearColor(
                        clamp_f32(cmd->payload.color.r, 0.0f, 1.0f),
                        clamp_f32(cmd->payload.color.g, 0.0f, 1.0f),
                        clamp_f32(cmd->payload.color.b, 0.0f, 1.0f),
                        clamp_f32(cmd->payload.color.a, 0.0f, 1.0f)
                    );
                    glClear(GL_COLOR_BUFFER_BIT);
                    break;
                case RFVP_SWITCH_RENDER_UPLOAD_TEXTURE_RGBA8:
                    upload_texture_rgba8(&cmd->payload.texture_upload);
                    break;
                case RFVP_SWITCH_RENDER_DRAW_TEXTURED_QUAD:
                    draw_textured_quad_gpu(&cmd->payload.textured_quad);
                    break;
                case RFVP_SWITCH_RENDER_DRAW_FILL_QUAD:
                    draw_fill_quad_gpu(&cmd->payload.fill_quad);
                    break;
                default:
                    break;
            }
        }
    }

    eglSwapBuffers(g_renderer.display, g_renderer.surface);
}

static void audio_fill_buffer(AudioOutBuffer *buffer) {
    int16_t *samples = (int16_t *)buffer->buffer;
    const size_t sample_count = RFVP_AUDIO_SAMPLE_COUNT;
    const size_t got = rfvp_switch_host_global_audio_pop_i16(samples, sample_count);

    if (got < sample_count) {
        memset(samples + got, 0, (sample_count - got) * sizeof(int16_t));
    }

    buffer->data_offset = 0;
    buffer->data_size = RFVP_AUDIO_BUFFER_BYTES;
    armDCacheFlush(buffer->buffer, buffer->buffer_size);
}

static int audio_init(void) {
    memset(&g_audio, 0, sizeof(g_audio));

    Result rc = audoutInitialize();
    if (R_FAILED(rc)) {
        return -1;
    }

    for (uint32_t i = 0; i < RFVP_AUDIO_BUFFER_COUNT; i++) {
        void *mem = memalign(0x1000, RFVP_AUDIO_BUFFER_BYTES);
        if (mem == NULL) {
            return -2;
        }
        memset(mem, 0, RFVP_AUDIO_BUFFER_BYTES);
        g_audio.buffer_mem[i] = mem;
        g_audio.buffers[i].next = NULL;
        g_audio.buffers[i].buffer = mem;
        g_audio.buffers[i].buffer_size = RFVP_AUDIO_BUFFER_BYTES;
        g_audio.buffers[i].data_offset = 0;
        g_audio.buffers[i].data_size = RFVP_AUDIO_BUFFER_BYTES;
        armDCacheFlush(mem, RFVP_AUDIO_BUFFER_BYTES);
        audoutAppendAudioOutBuffer(&g_audio.buffers[i]);
    }

    rc = audoutStartAudioOut();
    if (R_FAILED(rc)) {
        return -3;
    }

    g_audio.initialized = 1;
    return 0;
}

static void audio_pump(void) {
    if (!g_audio.initialized) {
        return;
    }

    for (;;) {
        AudioOutBuffer *released = NULL;
        uint32_t released_count = 0;
        Result rc = audoutWaitPlayFinish(&released, &released_count, 0);
        if (R_FAILED(rc) || released_count == 0 || released == NULL) {
            break;
        }

        audio_fill_buffer(released);
        audoutAppendAudioOutBuffer(released);
    }
}

static void audio_shutdown(void) {
    if (g_audio.initialized) {
        audoutStopAudioOut();
        audoutExit();
        g_audio.initialized = 0;
    }

    for (uint32_t i = 0; i < RFVP_AUDIO_BUFFER_COUNT; i++) {
        free(g_audio.buffer_mem[i]);
        g_audio.buffer_mem[i] = NULL;
    }
}

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    consoleInit(NULL);

    padConfigureInput(1, HidNpadStyleSet_NpadStandard);
    PadState pad;
    padInitializeDefault(&pad);
    hidInitializeTouchScreen();

    printf("RFVP Switch host\n");
    printf("Stage: RFVP core + Switch GLES2 GPU renderer + audout\n");
    printf("\n");
    printf("Rust host API:   %u\n", rfvp_switch_host_api_version());
    printf("Rust render API: %u\n", rfvp_switch_host_render_api_version());
    printf("Rust audio API:  %u\n", rfvp_switch_host_audio_api_version());
    printf("RFVP core ABI:   %u\n", rfvp_switch_host_core_abi_version());

    int init_result = rfvp_switch_host_global_init();
    printf("Rust host init:  %d\n", init_result);

    print_directory_probe("sdmc:/rfvp");
    print_directory_probe("sdmc:/switch/rfvp");

    const char *game_root = select_game_root();
    int load_result = rfvp_switch_host_global_load_game_root(game_root, "shiftjis", RFVP_FB_WIDTH, RFVP_FB_HEIGHT);
    printf("\nSelected game root: %s\n", game_root);
    printf("RFVP core load:     %d\n", load_result);
    if (load_result == -20) {
        printf("Core link disabled. Rebuild with RFVP_SWITCH_LINK_CORE=1 and RFVP_SWITCH_CORE_STATICLIB.\n");
    }

    int render_result = renderer_init();
    int audio_result = audio_init();
    printf("Switch GLES2 renderer: %d\n", render_result);
    printf("Switch audout:         %d\n", audio_result);
    printf("\nPress PLUS to exit. Touch maps to left click. A/B map to left/right click.\n");
    consoleUpdate(NULL);
    consoleExit(NULL);

    if (render_result != 0) {
        rfvp_switch_host_global_destroy_core();
        audio_shutdown();
        return render_result;
    }

    int prev_touch_active = 0;
    u64 last_tick = armGetSystemTick();
    while (appletMainLoop()) {
        padUpdate(&pad);
        const u64 down = padGetButtonsDown(&pad);
        const u64 up = padGetButtonsUp(&pad);
        const u64 held = padGetButtons(&pad);
        if ((down & HidNpadButton_Plus) != 0) {
            break;
        }

        RfvpSwitchInputFrame input;
        memset(&input, 0, sizeof(input));
        input.buttons_down = map_switch_buttons(down);
        input.buttons_up = map_switch_buttons(up);
        input.buttons_held = map_switch_buttons(held);

        HidTouchScreenState touch_state = {0};
        int touch_active = 0;
        if (hidGetTouchScreenStates(&touch_state, 1) > 0 && touch_state.count > 0) {
            touch_active = 1;
            input.touch_active = 1;
            input.touch_x = touch_state.touches[0].x;
            input.touch_y = touch_state.touches[0].y;
        }
        input.touch_down = (touch_active && !prev_touch_active) ? 1u : 0u;
        input.touch_up = (!touch_active && prev_touch_active) ? 1u : 0u;
        prev_touch_active = touch_active;

        const u64 now_tick = armGetSystemTick();
        const u64 elapsed_ns = armTicksToNs(now_tick - last_tick);
        last_tick = now_tick;
        uint32_t frame_time_ms = (uint32_t)((elapsed_ns + 500000u) / 1000000u);
        if (frame_time_ms == 0) {
            frame_time_ms = 1;
        } else if (frame_time_ms > 100) {
            frame_time_ms = 100;
        }

        (void)rfvp_switch_host_global_tick(frame_time_ms, &input);
        renderer_draw_current_frame();
        audio_pump();
    }

    rfvp_switch_host_global_destroy_core();
    audio_shutdown();
    renderer_shutdown();
    return 0;
}
