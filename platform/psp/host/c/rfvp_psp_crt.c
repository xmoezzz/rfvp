/*
 * The C library functions stb_vorbis needs on the PSP that nothing else provides.
 *
 * The PSP build links no C library. The psp crate exports malloc, free, the memory functions
 * and libm's math functions; what is left is here.
 */

#include <stddef.h>

/* Shell sort: stb_vorbis sorts codebook entries once per stream. */
void qsort(void *base, size_t count, size_t size, int (*cmp)(const void *, const void *)) {
    unsigned char *items = (unsigned char *)base;
    unsigned char tmp[64];
    if (size > sizeof(tmp)) {
        return;
    }
    for (size_t gap = count / 2; gap > 0; gap /= 2) {
        for (size_t i = gap; i < count; i++) {
            for (size_t j = i; j >= gap; j -= gap) {
                unsigned char *a = items + (j - gap) * size;
                unsigned char *b = items + j * size;
                if (cmp(a, b) <= 0) {
                    break;
                }
                __builtin_memcpy(tmp, a, size);
                __builtin_memcpy(a, b, size);
                __builtin_memcpy(b, tmp, size);
            }
        }
    }
}

double ldexp(double x, int e) {
    while (e > 0) {
        x *= 2.0;
        e--;
    }
    while (e < 0) {
        x *= 0.5;
        e++;
    }
    return x;
}
