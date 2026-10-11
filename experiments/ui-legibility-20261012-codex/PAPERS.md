# Automatic downscaling experiment — 2026-10-12

This experiment processes pixels only. It does not read annotations, recognize
text, replace fonts, run a language model, or change the target canvas size.
All work stays in this experiment directory; Claude's converter/runtime files
were not edited and no shared Cargo build was run.

## Selected method and research

- [Rapid, Detail-Preserving Image Downscaling](https://www.mergian.de/2016/siggraphasia-dpid/),
  SIGGRAPH Asia 2016. The authors provide [RGB MATLAB and CUDA code](https://github.com/mergian/dpid).
  DPID computes an area average, smooths that guide with a clipped 3×3 binomial
  kernel, then gives pixels more weight when they differ from the guide.
  Lambda controls that emphasis. It needs neither semantic recognition nor
  pretrained weights. The implementation is BSD-3-Clause.
- [VRChat SDK 3.7.1](https://creators.vrchat.com/releases/release-3-7-1/),
  September 2024, introduced opt-in DPID mipmaps, added transparency support,
  and made conservative mode the default after fixes. This is evidence of
  industry use, not evidence that its shader works unchanged on these consoles.
- [LSID](https://arxiv.org/html/2510.24334v1), October 2025 preprint,
  derives a co-occurrence profile from input pixels and uses it to guide filtering.
  It is a non-neural alternative. Its published algorithm describes grayscale
  intensity pairs, an averaging kernel of `(2s+1)²`, and a neighborhood `K≥s`.
  Arbitrary non-square scaling, RGBA handling and a converter-ready implementation
  are not established by that description. Not reproduced in this experiment.
- [ADK-Net](https://arxiv.org/abs/2511.01620), November 2025 preprint,
  predicts channel-specific RGB kernels using a trained convolutional network.
  It is not an LLM, but would introduce a model/training dependency. The stated
  natural-image benchmark gains do not establish legibility of tiny UI glyphs
  after PS1 palette encoding. Not run; no verified checkpoint was obtained.

DPID was selected for an actual experiment because its author implementation is
small, inspectable and portable. Publication recency alone is not a reason to
replace the current filter.

## Implementation and controls

`dpid.rs` is a standalone CPU Rust port with no external crates. It can be reused
in an offline converter; the consoles would load the resulting ordinary textures
without running DPID. MATLAB, CUDA, OCR, an LLM service and neural weights are not
needed for that integration. Keep `DPID-LICENSE.txt` with the port.

The original code assumes three RGB channels. This experiment explicitly extends
the shared distance and accumulation to **premultiplied RGBA**. For opaque input,
the added alpha difference is zero and distance normalization cancels, giving RGB
DPID. For transparent input this is our extension, not the original authors'
algorithm or VRChat's transparency implementation. Source RGB is clamped to alpha
to enforce the premultiplied contract and prevent hidden transparent color leaks.
Filtering uses stored byte space, matching the existing converter baseline;
linear-light filtering was not investigated here.

Comparisons:

| Name | Processing |
|---|---|
| area | Current area geometry and final rounding |
| lanczos | Premultiplied channel-wise Lanczos |
| dpid_balanced | 50% Area + 50% DPID lambda 0.5, before rounding |
| dpid_025 | DPID lambda 0.25 |
| dpid_050 | DPID lambda 0.5 |
| dpid_100 | DPID lambda 1.0 |
| dpid_budget | balanced, falling back to the already encoded Area result if its estimated texture cost increases |

The blend and budget guard are experiment additions, not paper parameters or a
reproduction of VRChat conservative mode. The guard constrains the reported cost;
it is not an automatic quality selector. Its timing is the selected filter timing,
not total candidate generation/encoding/selection latency.

## Samples, encoding and evidence

28 sources: 24 real first-frame UI assets from AKASEKA_HD, sakura and AngelWish,
and four existing synthetic label images from the earlier experiment. Seven added
real assets cover progress bars, frames and window/panel backgrounds. The control
images are reused as pixels; no label or font enters processing.

Four platforms × two encoding lanes × seven methods × 28 sources = 1,568 report
entries. There are 1,344 independently generated/encoded variants; the 224 budget
entries reuse an existing balanced or Area result.

The two lanes must be compared separately:

| Platform | Current lane | Compact lane |
|---|---|---|
| PSP | RGBA8888 | Mathematical CLUT8 model |
| PS2 | PSMCT32, GS alpha precision | Mathematical PSMT8 model, GS alpha precision |
| PS1 | Frozen project quantizer, auto CLUT4/CLUT8, dither enabled | Same quantizer, dither disabled |
| 3DS | RGBA8 | RGBA4 encoded and decoded with native tex3ds |

PS2 scale accounts for non-square pixels, but gallery views show the texel grid.
PS2 memory values are pixel payloads, not GS page allocation. PSP indexed formats
are models, not a verified loader integration. PS1 estimates include current tile
padding and both palettes, exclude allocator fragmentation, and preview normal
sprite alpha 255 (all nonzero CLUT words opaque). They do not simulate every GPU
sampling/framebuffer effect. 3DS estimates include min-8 power-of-two padding.
No emulator or hardware run has been performed.

The PS1 quantizer is the earlier frozen snapshot. Its equivalent-color ordering
can be nondeterministic; differences in a handful of palette pixels are not proof
of a filter improvement. Changing the actual converter or quantizer is deferred
to avoid colliding with concurrent work.

Validation completed:

- Three Rust tests covering constant/identity images, exact fractional area
  accumulation and mix=0, transparent/degenerate input, premultiplied bounds.
- Twelve opaque RGB comparisons against an independent scalar transcription of
  the authors' MATLAB algorithm; all output bytes matched, including non-integer
  reduction ratios and lambda 0, 0.25, 0.5, 1. MATLAB itself was not executed.
- Every source/platform Area result matched the previous Python Area reference.
- 168 unique 3DS RGBA4 variants passed native tex3ds pixel-by-pixel round trips.
- All report image paths exist; all 224 budget selections meet the Area estimate.
- Report loaded successfully from localhost. No OCR scores or fabricated
  semantic/readability ranking is used.

The author repository is pinned at
`dba82881a1797a12df123a7de4079e54da24bcc1`. Reference source URLs/hashes,
port and quantizer hashes, measurements and validation output are recorded under
`output/papers/`. Game-derived outputs remain Git-ignored and local.

## Observations and workflow direction

Visual inspection suggests low-strength DPID is useful on some thin outlines and
progress frames. In the PS1 progress-frame example, the Area preview loses much
of the right outline while balanced keeps more of it. Changes on text are modest
and asset-dependent. Strong lambda 1 can make strokes heavier and join gaps;
it does not rescue the densely packed 8-texel-high PS1 CJK prompt.

Across the 24 real sources, median pre-encoding alpha-sum changes for balanced
were +1.28% PSP, +0.70% PS2, +2.93% PS1, +1.84% 3DS. The corresponding lambda 1
changes were +5.37%, +2.86%, +11.68%, +7.80%. These describe transparency drift,
not readability or a perceptual quality score. Individual assets change more.

PSP, PS2 and 3DS costs stay constant within each encoding lane because sizes and
formats stay constant. On PS1, balanced switched three real assets per lane from
CLUT4 to CLUT8: AKASEKA_HD fc_save_help, icon_play_bar and main_window. The budget
guard falls back on those three in both lanes. In this recorded sample all 224
guarded results are within the baseline estimate. The guard should be evaluated
after actual encoding/tiling when integrated; source dimensions alone are not a
reliable memory check.

A reasonable initial converter option is **low-strength DPID with an encoding
budget guard**, processed offline at the final platform resolution before palette
or RGBA4 encoding. Preserve headers, placement, frame count and dimensions. Apply
the same operation independently to every frame, and check animated assets for
flicker before enabling it broadly. UI legibility still needs visual review;
this experiment does not justify replacing Area for all image types.

## Reproduce

Run the original `run.py --no-ocr` once if its ignored sources and frozen probes
are absent, then:

```sh
python3 experiments/ui-legibility-20261012-codex/paper_downscale.py
```

Python needs Pillow and numpy. Rustc builds only the standalone port/tests.
If tex3ds is unavailable, native RGBA4 verification is marked unavailable, not
silently claimed. Missing author reference files are fetched from the pinned
official repository commit, checked against embedded SHA-256 hashes, and cached
under `output/papers/references/` with `provenance.json`. No package installation was needed on the
current machine. Source code, comments and generated interface text are English;
localized content for the older redraw experiment is stored separately in
`label_fixtures.json` using JSON Unicode escapes. Original image pixels retain
their localized text.

Open `output/papers/report.html` locally for platform/codec/background controls,
native-size comparisons and nearest-neighbor diagnostic enlargement. The former
min12 report remains at `output/report.html`.
