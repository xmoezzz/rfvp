# PSV resource performance and translation overlays

Follow-up to PR #56, based on upstream `062fec82033b554b9eb3b5da069518c0c96a354e`.

## Resource loading and animation

- Vita known-length resource streams fill the final buffer in 128 KiB chunks.
- The optional `vita-zlib` feature builds libdeflate 1.26 for Vita's Cortex-A9/NEON, with zlib wrapper/checksum and exact output length checks. Other targets retain their existing decoders.
- NEON BGR/BGRA-to-RGBA conversion writes the final buffer directly, with scalar tails. No reduced color precision or alpha quantization is introduced.
- Vita native C release code is compiled with `-O2`.
- A fixed-point area downsampler handles large images. Transparent sprites can stay at native dimensions within the bounded near-cap policy, with allocation fallback.
- Graph generations and conservative dirty rectangles let capable hosts update only a changed part of a texture. Hosts can decline the optional full-source region update and use the existing upload path.
- Save snapshots skip default primitive slots, unchanged active resources can be reused on restore, and successful restore clears old queued input while filtering release events from already-held buttons.
- No additional cache of unloaded character/expression resources is introduced. Active textures and the active translation table remain owned by the running game.

The integrated code retains upstream's `AudioSilentOn` behavior, PS1 script fetch window/AOT support, overlay snapshot flag, and named diagnostic stage indices.

## Translation configuration

The existing upstream `config.toml` remains the primary configuration file. Its `[game] hcb` and `nls` keys retain their previous behavior. An existing local v17 `game.toml` is read as a fallback only when `config.toml` is absent. PS1 continues to ignore this game configuration.

A translated HCB needs only:

```toml
[game]
hcb = "MemoriaGBK.hcb"
nls = "gbk"
```

An external offset-based translation can preserve the original script encoding:

```toml
[game]
hcb = "Sakura.hcb"
nls = "sjis"
text_patch = "patch.dat"
text_nls = "gbk"
hcb_crc32 = "9974eff7"
asset_aliases = "chs-assets.tsv"
```

The CRC is specific to the matching original HCB. `text_patch` requires an explicit HCB, CRC32, and translation encoding. It does not re-encode resource filenames or alter script instructions/jump offsets.

The supported translation payload is little-endian: a `u32` record count, followed by records containing a `u32` string-payload offset, a `u16` byte length, and that many text bytes. Nonempty entries include a final NUL; zero length keeps the original text. The compressed representation has four `u32` header values (zero signature, timestamp, decompressed size, compressed size) followed by zlib data. The loader limits sizes and checks offsets against PushString operands, terminators, encoding, and duplicates.

`asset_aliases` names a UTF-8 TSV file whose two columns are original and replacement VFS paths. For example, `graph/title_logo1` can map to `patch/title_logo1` in an existing `patch.bin`. Targets are validated and aliases cannot chain. Resources are read on demand.

This preserves original HCB/BIN files. The engine has no game-name or fixed-address performance branches. It supports this offset-table format; other translation formats need their own loader or conversion. Windows patch injection, Direct3D hooks, and external movie/voice subtitle hooks are outside this change.

Example configuration files are in `platform/vita/examples/`. They contain no game data or fonts.

## Hardware coverage and evidence

Two games have been exercised on an actual PS Vita during the local diagnostic iterations:

| Game | Observed coverage |
| --- | --- |
| 星空的记忆 Wish upon a shooting star / Hoshizora no Memoria | Japanese and GBK Chinese-script operation, character voices, Chinese font rendering, and dialogue-advance stutter improvements. Earlier Chinese hardware feedback confirmed complete text rendering and normal operation. |
| 樱花、萌放 / さくら、もゆ。-as the Night's, Reincarnation- | Scene changes, sprite appearance/posture/expression changes, save/load restoration, and the load-menu input fix. The user accepted v16 stutter as within a tolerable range. The v17 Chinese deployment uses the offset table and image aliases. |

The user subsequently reported: “第17版我已在实机确认，没有问题” (v17 confirmed on hardware, no issues). This was an overall version confirmation, not a claim that every route, game/configuration combination, font, or save slot was covered.

Same-resource observations from two local Sakura runs, before/after the v16 native asset changes:

| Stage | Earlier run | Later run |
| --- | ---: | ---: |
| 1424×1853 sprite pixel conversion | 250.290 ms | 114.626 ms |
| Large expression-table inflate | 961.311 ms | 687.758 ms |

These are individual stage observations from different runs, not controlled whole-animation benchmarks.

### Integration verification

The original device-confirmed v17 snapshot is retained in the fork's `psv-v17-20261022` branch. This PR integrates that work with the newer upstream main and uses the distinct build marker `rfvp-psv-pr-20261022-17`.

Vita cross-compilation and VPK structure checks are recorded in the PR description. The integrated PR build has not been installed on the device. No new implementation tests or game regressions were run during this PR preparation, and other platforms were not compiled.

## Third-party source

`crates/rfvp/vendor/libdeflate/` is the unmodified v1.26 source distribution, commit `92e6a0db9fa848d742f9eb286c92afc60f2c3dda`. Its MIT `COPYING` is preserved. The existing vendored programs/fixtures are included with the distribution and were not executed as part of this change.
