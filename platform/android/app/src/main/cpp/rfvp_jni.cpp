#include <jni.h>
#include <android/native_window_jni.h>
#include <android/log.h>
#include <dlfcn.h>

#include <mutex>
#include <unordered_map>

#define LOG_TAG "rfvp_jni"
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)
#define LOGW(...) __android_log_print(ANDROID_LOG_WARN, LOG_TAG, __VA_ARGS__)
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)

// C ABI exported from librfvp.so (Rust).
// We resolve these at runtime via dlsym(RTLD_DEFAULT, ...).
using create_fn_t = void* (*)(void* native_window_ptr, uint32_t w_px, uint32_t h_px, double scale,
                              const char* game_dir_utf8, const char* nls_utf8);
using step_fn_t = int32_t (*)(void* handle, uint32_t dt_ms);
using resize_fn_t = void (*)(void* handle, uint32_t w_px, uint32_t h_px);
using set_surface_fn_t = void (*)(void* handle, void* native_window_ptr, uint32_t w_px, uint32_t h_px);
using touch_fn_t = void (*)(void* handle, int32_t phase, double x_px, double y_px);
using destroy_fn_t = void (*)(void* handle);
using init_context_fn_t = void (*)(void* java_vm_ptr, void* app_context_global_ref);

struct Api {
    create_fn_t create = nullptr;
    step_fn_t step = nullptr;
    resize_fn_t resize = nullptr;
    set_surface_fn_t set_surface = nullptr;
    touch_fn_t touch = nullptr;
    destroy_fn_t destroy = nullptr;
    init_context_fn_t init_context = nullptr;
};

static Api g_api;
static std::once_flag g_api_once;
static void* g_lib_handle = nullptr;

static std::once_flag g_ctx_once;
static JavaVM* g_java_vm = nullptr;
static jobject g_app_ctx = nullptr;

static void load_api_or_log() {
    std::call_once(g_api_once, []() {
const char* lib_path = "librfvp.so"; 
        g_lib_handle = dlopen(lib_path, RTLD_NOW);

        if (!g_lib_handle) {
            LOGE("dlopen failed: %s", dlerror());
            return;
        }

        auto load = [](const char* sym) -> void* {
            void* p = dlsym(g_lib_handle, sym);
            if (!p) {
                LOGE("dlsym failed: %s (%s)", sym, dlerror());
            }
            return p;
        };

        g_api.create = reinterpret_cast<create_fn_t>(load("rfvp_android_create"));
        g_api.step = reinterpret_cast<step_fn_t>(load("rfvp_android_step"));
        g_api.resize = reinterpret_cast<resize_fn_t>(load("rfvp_android_resize"));
        g_api.set_surface = reinterpret_cast<set_surface_fn_t>(load("rfvp_android_set_surface"));
        g_api.touch = reinterpret_cast<touch_fn_t>(load("rfvp_android_touch"));
        g_api.destroy = reinterpret_cast<destroy_fn_t>(load("rfvp_android_destroy"));
        g_api.init_context = reinterpret_cast<init_context_fn_t>(load("rfvp_android_init_context"));

        if (g_api.create && g_api.step && g_api.resize && g_api.set_surface && g_api.touch && g_api.destroy) {
            LOGI("rfvp_android_* symbols resolved");
        } else {
            LOGE("missing one or more rfvp_android_* symbols; check that librfvp.so exports them");
        }

        if (!g_api.init_context) {
            LOGW("rfvp_android_init_context is missing; audio backends may crash (ndk-context not initialized)");
        }
    });
}

extern "C" JNIEXPORT void JNICALL
Java_com_rfvp_launcher_NativeRfvp_nativeInitAndroidContext(JNIEnv* env, jclass, jobject app_context) {
    load_api_or_log();
    if (!g_api.init_context) {
        LOGE("nativeInitAndroidContext: rfvp_android_init_context is null (symbol missing)");
        return;
    }
    if (!app_context) {
        LOGE("nativeInitAndroidContext: app_context is null");
        return;
    }

    // Hold a GlobalRef to the context so it remains valid for the lifetime of the process.
    // ndk-context expects the jobject it receives to be a GlobalRef.
    std::call_once(g_ctx_once, [env, app_context]() {
        JavaVM* vm = nullptr;
        if (env->GetJavaVM(&vm) != JNI_OK || !vm) {
            LOGE("nativeInitAndroidContext: GetJavaVM failed");
            return;
        }

        jobject gref = env->NewGlobalRef(app_context);
        if (!gref) {
            LOGE("nativeInitAndroidContext: NewGlobalRef failed");
            return;
        }

        g_java_vm = vm;
        g_app_ctx = gref;
        g_api.init_context(reinterpret_cast<void*>(vm), reinterpret_cast<void*>(gref));
        LOGI("nativeInitAndroidContext: ndk-context initialized");
    });
}

// Keep one ANativeWindow ref per engine handle so the pointer stays valid while Rust uses it.
static std::mutex g_win_mu;
static std::unordered_map<jlong, ANativeWindow*> g_windows;

static void release_window_locked(jlong handle_key) {
    auto it = g_windows.find(handle_key);
    if (it != g_windows.end()) {
        if (it->second) {
            ANativeWindow_release(it->second);
        }
        g_windows.erase(it);
    }
}

static const char* get_utf8_or_null(JNIEnv* env, jstring s) {
    if (!s) return nullptr;
    return env->GetStringUTFChars(s, nullptr);
}

static void release_utf8(JNIEnv* env, jstring s, const char* p) {
    if (s && p) {
        env->ReleaseStringUTFChars(s, p);
    }
}

extern "C" JNIEXPORT jlong JNICALL
Java_com_rfvp_launcher_NativeRfvp_create(JNIEnv* env, jclass,
                                        jobject surface,
                                        jint width_px,
                                        jint height_px,
                                        jdouble scale,
                                        jstring game_dir_utf8,
                                        jstring nls_utf8) {
    load_api_or_log();
    if (!g_api.create) {
        return 0;
    }
    if (!surface) {
        LOGE("create: surface is null");
        return 0;
    }

    ANativeWindow* win = ANativeWindow_fromSurface(env, surface);
    if (!win) {
        LOGE("ANativeWindow_fromSurface returned null");
        return 0;
    }

    const char* game_dir = get_utf8_or_null(env, game_dir_utf8);
    const char* nls = get_utf8_or_null(env, nls_utf8);

    void* handle = g_api.create(reinterpret_cast<void*>(win),
                                static_cast<uint32_t>(width_px),
                                static_cast<uint32_t>(height_px),
                                static_cast<double>(scale),
                                game_dir,
                                nls);

    release_utf8(env, game_dir_utf8, game_dir);
    release_utf8(env, nls_utf8, nls);

    if (!handle) {
        ANativeWindow_release(win);
        LOGE("rfvp_android_create returned null");
        return 0;
    }

    jlong key = reinterpret_cast<jlong>(handle);
    {
        std::lock_guard<std::mutex> lk(g_win_mu);
        // Replace if already present (should not happen on a fresh create).
        release_window_locked(key);
        g_windows.emplace(key, win);
    }

    return key;
}

extern "C" JNIEXPORT jint JNICALL
Java_com_rfvp_launcher_NativeRfvp_step(JNIEnv*, jclass, jlong handle, jint dt_ms) {
    load_api_or_log();
    if (!g_api.step) {
        return 1;
    }
    if (handle == 0) {
        return 1;
    }
    return static_cast<jint>(g_api.step(reinterpret_cast<void*>(handle), static_cast<uint32_t>(dt_ms)));
}

extern "C" JNIEXPORT void JNICALL
Java_com_rfvp_launcher_NativeRfvp_resize(JNIEnv*, jclass, jlong handle, jint width_px, jint height_px) {
    load_api_or_log();
    if (!g_api.resize || handle == 0) {
        return;
    }
    g_api.resize(reinterpret_cast<void*>(handle), static_cast<uint32_t>(width_px), static_cast<uint32_t>(height_px));
}

extern "C" JNIEXPORT void JNICALL
Java_com_rfvp_launcher_NativeRfvp_setSurface(JNIEnv* env, jclass,
                                            jlong handle,
                                            jobject surface,
                                            jint width_px,
                                            jint height_px) {
    load_api_or_log();
    if (!g_api.set_surface || handle == 0) {
        return;
    }
    if (!surface) {
        LOGW("setSurface: surface is null (ignored)");
        return;
    }

    ANativeWindow* win = ANativeWindow_fromSurface(env, surface);
    if (!win) {
        LOGE("setSurface: ANativeWindow_fromSurface returned null");
        return;
    }

    g_api.set_surface(reinterpret_cast<void*>(handle), reinterpret_cast<void*>(win),
                      static_cast<uint32_t>(width_px), static_cast<uint32_t>(height_px));

    {
        std::lock_guard<std::mutex> lk(g_win_mu);
        // Swap out old window ref.
        release_window_locked(handle);
        g_windows.emplace(handle, win);
    }
}

extern "C" JNIEXPORT void JNICALL
Java_com_rfvp_launcher_NativeRfvp_touch(JNIEnv*, jclass, jlong handle, jint phase, jdouble x_px, jdouble y_px) {
    load_api_or_log();
    if (!g_api.touch || handle == 0) {
        return;
    }
    g_api.touch(reinterpret_cast<void*>(handle), static_cast<int32_t>(phase),
                static_cast<double>(x_px), static_cast<double>(y_px));
}

extern "C" JNIEXPORT void JNICALL
Java_com_rfvp_launcher_NativeRfvp_destroy(JNIEnv*, jclass, jlong handle) {
    load_api_or_log();
    if (!g_api.destroy || handle == 0) {
        return;
    }
    // Drop the Rust side first.
    g_api.destroy(reinterpret_cast<void*>(handle));

    // Release the native window ref we kept for this handle.
    {
        std::lock_guard<std::mutex> lk(g_win_mu);
        release_window_locked(handle);
    }
}
