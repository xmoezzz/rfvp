# Mask dissolve

Mask dissolves (`MaskFadeIn`, `MaskFadeInOut`, `MaskFadeOut`) composite a black
mask over the main scene before dissolve2 and the HUD/custom root. They use the
existing motion timing and retain the source Type 3 / Single8Bit values.
The mask reveal evaluates `clamp(source_alpha + 2 * progress - 1, 0, 1)`.
Progress zero is clear, progress one is black, and halfway retains the source
mask coverage. This is a mask fade approximation, not a captured-scene wipe.

The desktop/mobile GPU renderer and software renderer share the continuous-alpha
mask frame with the portable host renderer. The RGBA allocation is reused, and
pixels/GPU data are updated only when source or phase changes. Completing the
effect releases its working buffer and GPU texture. Other platforms retain their
existing image conversion formats; mask data is never reduced to 1-bit.

## PlayStation

`rfvp-psx-convert` keeps Area reduction for Type 3 and appends a versioned,
GPU-ready mask plane. The original compressed 8-bit plane and NVSG metadata
remain intact. Types 0/1/2, CLUT pictures and Type 4 glyphs are unchanged.

The derivative encodes fixed 8x8 Bayer coverage thresholds in CLUT8 indices.
Its palette contains only transparent `0x0000`, STP white `0xffff` and opaque
white `0x7fff`. At the midpoint the 64-pixel mean coverage differs from original
alpha by less than one 8-bit level. A dissolve changes only the 512-byte palette;
there is no per-frame, per-pixel CPU dithering or temporal noise.

The PS1 primitive always enables semi-transparency for this native mask format,
uses blend mode 0 and the unmodified three-state CLUT. It modulates white texels
to black and blends directly over the scene framebuffer. It does not attempt to
sample an independent mask against an arbitrary second color texture.

The CPU image holds original alpha plus the GPU indices, approximately the same
pixel RAM as the former two-channel LumaA8 representation, plus palette/header
overhead. GPU pixels use 8bpp instead of the former direct 16bpp upload. Index
zero remains transparent padding throughout the animation. Ordinary CLUT
textures keep their existing semi-transparency behavior.

Existing PS1 discs need their Type 3 resources reconverted. To preserve all
previously converted color pictures and glyphs:

```sh
cargo run -p rfvp-psx --release -- \
  --input /path/to/original/game --output /path/to/converted/disc \
  --images-only --masks-only --image-pack graph
```

Rebuild that disc with `platform/ps1/build_ps1.sh /path/to/converted/disc`.
Unconverted legacy masks remain readable and log a reconversion hint.
