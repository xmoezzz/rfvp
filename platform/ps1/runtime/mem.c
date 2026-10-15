/* memcpy, memmove and memset a word at a time. PSn00bSDK's libc copies a byte at a time (with a
 * load delay each: about 8 cycles a byte), and the engine copies a lot: values moved in Rust,
 * vectors growing, pixels. These replace them (CMakeLists.txt: the linker keeps the first
 * definition, and this object comes before the library). */
/* Loops here must stay loops, not calls to themselves. */
#pragma GCC optimize("no-tree-loop-distribute-patterns")

#include <stddef.h>
#include <stdint.h>

/* An unaligned word, with the R3000's lwl/lwr pair. */
static inline uint32_t load_unaligned(const uint8_t* p) {
    uint32_t v;
    __asm__("lwl %0, 3(%1)\n\tlwr %0, 0(%1)" : "=&r"(v) : "r"(p) : "memory");
    return v;
}

void* memcpy(void* dst, const void* src, size_t n) {
    uint8_t* d = dst;
    const uint8_t* s = src;
    if (n >= 8) {
        /* Align the destination; then whole words, from an aligned or unaligned source. */
        while ((uintptr_t) d & 3) {
            *d++ = *s++;
            n--;
        }
        uint32_t* dw = (uint32_t*) d;
        if (((uintptr_t) s & 3) == 0) {
            const uint32_t* sw = (const uint32_t*) s;
            for (; n >= 16; n -= 16, dw += 4, sw += 4) {
                uint32_t a = sw[0], b = sw[1], c = sw[2], e = sw[3];
                dw[0] = a;
                dw[1] = b;
                dw[2] = c;
                dw[3] = e;
            }
            for (; n >= 4; n -= 4) *dw++ = *sw++;
            s = (const uint8_t*) sw;
        } else {
            for (; n >= 4; n -= 4, s += 4) *dw++ = load_unaligned(s);
        }
        d = (uint8_t*) dw;
    }
    while (n--) *d++ = *s++;
    return dst;
}

void* memmove(void* dst, const void* src, size_t n) {
    uint8_t* d = dst;
    const uint8_t* s = src;
    if (d <= s || d >= s + n) return memcpy(dst, src, n);
    /* Overlapping, destination after the source: from the end. */
    d += n;
    s += n;
    if (n >= 8 && (((uintptr_t) d | (uintptr_t) s) & 3) == 0) {
        uint32_t* dw = (uint32_t*) d;
        const uint32_t* sw = (const uint32_t*) s;
        for (; n >= 4; n -= 4) *--dw = *--sw;
        d = (uint8_t*) dw;
        s = (const uint8_t*) sw;
    }
    while (n--) *--d = *--s;
    return dst;
}

void* memset(void* dst, int c, size_t n) {
    uint8_t* d = dst;
    if (n >= 8) {
        while ((uintptr_t) d & 3) {
            *d++ = (uint8_t) c;
            n--;
        }
        uint32_t w = (uint8_t) c * 0x01010101u;
        uint32_t* dw = (uint32_t*) d;
        for (; n >= 16; n -= 16, dw += 4) {
            dw[0] = w;
            dw[1] = w;
            dw[2] = w;
            dw[3] = w;
        }
        for (; n >= 4; n -= 4) *dw++ = w;
        d = (uint8_t*) dw;
    }
    while (n--) *d++ = (uint8_t) c;
    return dst;
}
