# Low-resolution UI experiment — 2026-10-12

Scope: **PSP, PS2, PS1, and the 3DS top screen only**. This is an isolated, offline experiment. It does not modify the engine, pack converters, platform backends, Cargo files, or original game assets. There are no console/emulator execution results yet.

Open [the comparison report](output/report.html), [the focused overview](output/summary.png), or [the raw measurements](output/measurements.csv). Generated game-derived assets and source-code snapshots stay in ignored `output/`; do not publish that folder with the tool.

## What was tested

17 real, first-frame UI assets from `AKASEKA_HD`, `sakura`, and `AngelWish`, plus four controlled labels with known text. Their design resolutions are read from the HCB headers: 1920×1080, 1280×720, and 800×600 respectively. The controlled labels use a 1920×1080 design space.

21 sources × four platforms × six methods = **504 cases**:

| Method | Processing | Encoding |
|---|---|---|
| `area_current` | Current integer-overlap area downsampling | Current backend format; PS1 default force4 selection and dithering |
| `area_compact` | Same area downsampling | Candidate compact format; PS1 dithering disabled |
| `lanczos` | Lanczos on separate premultiplied channels | Candidate compact format; PS1 dithering disabled |
| `coverage` | Area + modest local alpha expansion | Candidate compact format; PS1 dithering disabled |
| `redraw_fit` | Known text rerasterized inside the existing target canvas | Candidate compact format |
| `redraw_min12` | Known text rerasterized, font size ≥12; canvas grows if necessary | Candidate compact format |

`coverage` is an experimental heuristic, **not a reproduction of a research paper**. The redraw methods replace the annotated label with a plain white font and dark outline. They deliberately trade artwork/style for clarity. They do not reconstruct the original artistic lettering. For unannotated sources the redraw columns retain the Lanczos candidate; these are marked in the report.

The original Japanese font BIZUDGothic does not cover every simplified Chinese annotation. The experiment checks for missing glyphs and uses the local macOS STHeiti Medium font for those strings. Font paths/hashes are recorded; system font files are not copied or distributed. A portable build would need an appropriately licensed bundled CJK fallback.

## Platform models and their limits

| Platform | Raster target | Baseline | Compact candidate |
|---|---|---|---|
| PSP | 480×272, aspect fit | RGBA8888 | CLUT8 mathematical model with a 256-entry RGBA palette |
| PS2 | 640×448; uses current renderer's non-square TV pixel mapping | PSMCT32, GS alpha 0…128 | PSMT8 mathematical model |
| PS1 | 320×240, aspect fit | Actual snapshotted project CLUT4/CLUT8 quantizer and force4 decision | Same quantizer, dithering disabled |
| 3DS | 400×240 top screen, aspect fit | RGBA8, power-of-two allocation | RGBA4, native tex3ds encoded and independently decoded |

PS1 is compiled with **standalone rustc** against a source snapshot, so the experiment does not invoke Cargo or share Claude's build output. The snapshot and font SHA-256 values are in `results.json`; all original asset hashes and pack offsets are in `sources.json`.

PS1 previews follow the current normal sprite path at **sprite alpha=255**: zero CLUT words are transparent; nonzero words are opaque because the primitive's semi-transparency flag is not enabled in this path. A PNG preview using the stored continuous alpha would give a misleading result. These previews still exclude GPU sampling rules, tint rounding, framebuffer dithering, CRT behavior, scene composition, and alternative sprite alpha/blend modes.

PS1 memory estimates include uploaded tile row alignment and both runtime palettes; they exclude shelf fragmentation and object bookkeeping. PSP figures model renderer pixel storage plus per-texture palettes, using the present stride policy; a native CLUT upload implementation must check its own alignment, palette sharing and swizzle requirements. PS2 reports **pixel payload only**, not GS page allocations or texture-manager peak residency. 3DS figures include minimum-eight/power-of-two padding, but not object bookkeeping or staging buffers. All figures exclude engine CPU copies and total-scene residency.

3DS RGBA4 is encoded with local `tex3ds -f rgba4 -r -z none`. The experiment decodes its 8×8 Morton layout and checks every padded texel against 4-bit channel truncation. All 126 3DS cases passed; this verifies the asset encoder/decoder, **not runtime GPU upload or display**. No ETC1/ETC1A4, SDF runtime, neural model, PS2 native PSMT8 upload, or PSP native CLUT8 upload was implemented.

Validation also passed: the snapshotted PS1 quantizer's four unit tests; integer area-resize equivalence to direct summation; premultiplied-alpha roundtrip; 504 report image references; and Python syntax compilation. All compilation outputs are confined to this experiment.

## Results and decisions

**There is no universally better replacement for area downsampling.** On these labels, Lanczos and alpha expansion sometimes help and sometimes merge strokes or make halos worse. Forcing rerendered text back into tiny boxes can be worse than retaining the original artwork.

1. **PSP:** a CLUT8 candidate looks viable for selected flat UI. The 17 real assets' modeled renderer storage changes from 67,872 B to 34,376 B with unchanged dimensions. This is a candidate allocation estimate, not a measured PSP memory reduction. Prefer per-resource opt-in and verify actual palette upload/alignment before integration.
2. **PS2:** existing dimensions are much less restrictive here. All five English/digit OCR probes are already exact with the baseline; they provide no evidence for a mandatory text redesign. PSMT8 changes these 17 assets' pixel payload from 117,164 B to 46,699 B, but GS layout, CLUT ordering, alpha and streaming must be implemented and measured separately.
3. **PS1:** there is already a compact native CLUT pipeline. Removing dithering or changing the resizer is insufficient for very small CJK labels. The high-resolution Japanese exit prompt becomes **73×8**; the plain minimum-12 redraw needs **138×17**. Modeled uploaded texture bytes increase from **384 B to 1,288 B**. It is clearer but requires an explicit UI layout change, not a drop-in replacement.
4. **3DS:** same-size RGBA4 halves these real assets' padded allocations from **89,344 B to 44,672 B**. Enlarging labels can cross power-of-two boundaries: the same exit prompt's **91×10 RGBA8** allocation is **8,192 B**, while **138×17 RGBA4** needs **16,384 B**. Minimum text size must be planned together with atlas/page packing or geometry; a per-image enlargement rule can consume more memory than the baseline.

The English OCR check has **only five probes per platform** (two real labels, three controls). All methods use identical black backing, inverted grayscale, nearest-neighbor 4× enlargement and Tesseract English `--psm 7`. It is an automatic regression signal, not a human readability measurement; no Chinese/Japanese OCR language is installed. For example, the 3DS control `SAVE 012345` changes from `SANE012205` in the baseline to an exact match after minimum-12 redraw, but that redraw also increases its canvas. A general quality improvement cannot be inferred from five strings.

An additional PS1 stability probe ran three sources eight times each. The two real sources were pixel-stable; the small digit control produced two decoded results, differing in up to 2.34% of pixels. Hash-map/median-cut tie behavior in the snapshotted quantizer can affect marginal examples. Record outputs and inspect differences rather than treating one OCR result as a stable quality guarantee.

## Safe integration sequence

1. Add per-resource rules to an offline tool: content type, method, permitted format, confirmed text, font fallback, and whether geometry may change. Keep game masks and click regions out of cosmetic processing.
2. Trial same-size compact formats first: PSP CLUT8, PS2 PSMT8, and 3DS RGBA4. Implement and validate their native loaders independently; current runtime backends cannot consume all candidates as-is.
3. Preserve PS1's existing CLUT format and texture-page handling. Opt into special target-sized text only for confirmed illegible labels; reserve layout space explicitly.
4. Rebuild full button states and validate scene screenshots, script crops, anchors, click regions, font coverage, staging memory and actual peak residency on emulator/hardware. The current first-frame previews cannot validate these behaviors.

The 2026 TIGER work motivates separating glyph structure from image appearance, but it is a **super-resolution** method; this experiment does not claim its reported gains transfer to downsampling. Ordinary UI with clear source strings does not need generative restoration. References: [TIGER / CVPR 2026](https://arxiv.org/abs/2510.21590), [official code](https://github.com/OpenVeraTeam/TiGeSR), [perceptually based downscaling](https://cgl.ethz.ch/publications/papers/paperOzt15b.php), [GLYPH-SR](https://arxiv.org/abs/2510.26339).

## Reproduce

From the repository root, with Python containing Pillow/numpy, rustc, tex3ds and Tesseract on the machine:

```sh
python3 experiments/ui-legibility-20261012-codex/run.py
```

`--no-ocr` omits OCR checks. The script reads the three local game directories and writes only `output/`. It does not produce a complete runnable game build. To regenerate the snapshot, run only when the quantizer source is not being edited; an existing report remains independently reviewable through its recorded hashes and copied snapshot.
