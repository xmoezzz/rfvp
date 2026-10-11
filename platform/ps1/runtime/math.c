/* The few math routines the engine's Rust code calls and PSn00bSDK has no library for. */

float fminf(float a, float b) {
    if (a != a) return b;
    if (b != b) return a;
    return a < b ? a : b;
}

float fmaxf(float a, float b) {
    if (a != a) return b;
    if (b != b) return a;
    return a > b ? a : b;
}

/* a - b * trunc(a / b), for the magnitudes the engine uses (angles, positions). */
float fmodf(float a, float b) {
    if (b == 0.0f || a != a || b != b) return 0.0f / 0.0f;
    float q = a / b;
    long long n = (long long) q;
    return a - (float) n * b;
}
