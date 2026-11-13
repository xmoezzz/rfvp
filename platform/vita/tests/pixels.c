/* Run on an ARM NEON host with:
 * cc -std=c11 -O2 -fsanitize=address,undefined platform/vita/tests/pixels.c \
 *    crates/rfvp/vendor/rfvp_psv_pixels.c -o /tmp/rfvp-psv-pixels-test
 * /tmp/rfvp-psv-pixels-test
 */
#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

void rfvp_psv_bgra_to_rgba(const uint8_t *, uint8_t *, size_t);
void rfvp_psv_bgr_to_rgba(const uint8_t *, uint8_t *, size_t);

int main(void) {
    /* Exercise vector boundaries, scalar tails, unaligned inputs, and all alpha
     * byte values. Exact input allocations let ASan catch accidental overreads. */
    const size_t counts[] = {0, 1, 15, 16, 17, 31, 32, 33, 257};
    for (size_t n = 0; n < sizeof(counts) / sizeof(counts[0]); ++n) {
        const size_t count = counts[n];
        for (size_t bpp = 3; bpp <= 4; ++bpp) {
            for (size_t offset = 0; offset <= 1; ++offset) {
                uint8_t *allocation = malloc(count * bpp + offset + (count == 0));
                uint8_t *output = malloc(count * 4 + 2);
                assert(allocation != NULL && output != NULL);
                uint8_t *source = allocation + offset;
                for (size_t i = 0; i < count; ++i) {
                    source[i * bpp] = (uint8_t)(i * 37);
                    source[i * bpp + 1] = (uint8_t)(i * 53);
                    source[i * bpp + 2] = (uint8_t)(i * 71);
                    if (bpp == 4) source[i * bpp + 3] = (uint8_t)i;
                }
                memset(output, 0xa5, count * 4 + 2);
                if (bpp == 3) rfvp_psv_bgr_to_rgba(source, output + 1, count);
                else rfvp_psv_bgra_to_rgba(source, output + 1, count);
                assert(output[0] == 0xa5 && output[count * 4 + 1] == 0xa5);
                for (size_t i = 0; i < count; ++i) {
                    assert(output[1 + i * 4] == source[i * bpp + 2]);
                    assert(output[1 + i * 4 + 1] == source[i * bpp + 1]);
                    assert(output[1 + i * 4 + 2] == source[i * bpp]);
                    assert(output[1 + i * 4 + 3] == (bpp == 4 ? source[i * bpp + 3] : 255));
                }
                free(output);
                free(allocation);
            }
        }
    }
    return 0;
}
