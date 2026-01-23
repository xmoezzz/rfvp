# RFVP Save/Load Full Snapshot Format
Version: 1 (Draft for implementation)
Scope: Full restore of VM + prim/motion + parts/gaiji + audio state.
Out of scope: Video playback state, InputManager state (explicitly excluded).

## Goals
- Preserve existing SaveItem header + thumbnail layout for UI compatibility.
- Append an extensible snapshot chunk that enables full-state restoration.
- Compatibility requirements:
  - Older saves without snapshot chunk remain loadable (metadata + thumb only).
  - Future snapshot versions can be introduced without breaking old saves.
  - Unknown snapshot blocks can be skipped safely.

---

## File Layout

The save file is:

```

[SaveItemHeader + strings][ThumbnailRGBA][Optional SnapshotChunk]

```

### 1) SaveItemHeader + strings (legacy-compatible)
Binary, little-endian:

| Field | Type | Notes |
|---|---|---|
| year | u16 | Gregorian year |
| month | u8 | 1..12 |
| day | u8 | 1..31 |
| day_of_week | u8 | engine-defined (keep as captured) |
| hour | u8 | 0..23 |
| minute | u8 | 0..59 |
| title_len | u16 | bytes |
| title_bytes | [u8; title_len] | UTF-8 |
| scene_len | u16 | bytes |
| scene_bytes | [u8; scene_len] | UTF-8 |
| script_len | u16 | bytes |
| script_bytes | [u8; script_len] | UTF-8 |

### 2) ThumbnailRGBA
Immediately after strings:

```

thumb_rgba: [u8; thumb_width * thumb_height * 4]

```

- Width/height are known by runtime constants (the engine’s save thumbnail size).
- Pixel format: RGBA8, row-major.

### 3) Optional SnapshotChunk (new)
If present, it starts immediately after the thumbnail.

---

## SnapshotChunk (RVSS)
All little-endian.

### Header
```

magic: [u8;4] = b"RVSS"
ver:   u16     = snapshot container version (start at 1)
flags: u16     bitflags
raw_len:  u32  length of uncompressed payload
comp_len: u32  length of compressed payload (== raw_len if not compressed)
payload:  [u8; comp_len]

```

### flags
- bit0 (0x0001): payload is compressed (deflate/zlib)
- other bits reserved (must be 0 in v1)

### Payload (Block stream)
The payload is a sequence of TLV blocks:

```

repeat until EOF:
tag: u16
len: u32
data: [u8; len]

```

- `data` is a bincode-serialized struct using a fixed configuration:
  - little-endian
  - fixed-int encoding (no varint)
  - reject trailing bytes (within block)

Unknown tags are skipped by reading `len` and advancing.

---

## Block Tags (v1)
Each block tag corresponds to a distinct subsystem snapshot.
All structs described below are serialized independently as `data`.

| Tag (hex) | Name | Required | Notes |
|---:|---|:---:|---|
| 0x0101 | VmSnapshotV1 | Yes | ThreadManager + globals + timers/flags/history |
| 0x0201 | MotionSnapshotV1 | Yes | prim + motion containers + graph buffs + dissolve + snow |
| 0x0301 | TextSnapshotV1 | Yes | text rendering state + pixel buffers |
| 0x0401 | PartsSnapshotV1 | Yes | parts items + parts motions + allocation pool |
| 0x0501 | GaijiSnapshotV1 | No | optional, only if gaiji used |
| 0x0601 | AudioSnapshotV1 | No | optional, if audio subsystem enabled |

Rationale:
- Separate blocks allow independent evolution and partial compatibility.
- Optional blocks can be absent; loaders must handle defaults.

---

## VmSnapshotV1 (0x0101)
Captures VM-observable semantics.

### Fields
- `thread_manager`: `ThreadManagerSnapshotV1`
- `globals`: snapshot of `GLOBAL` table
- `flags`: `FlagManagerSnapshotV1`
- `history`: `HistoryManagerSnapshotV1` (if used by scripts)
- `timers`: `TimerManagerSnapshotV1`
- `rng_state`: optional (only if RNG affects script semantics)

### ThreadManagerSnapshotV1
- `current_id: u8`
- `break_flag: bool`
- `contexts: [ContextSnapshotV1; 32]`

#### ContextSnapshotV1 (per thread)
Store the minimal data required to continue execution:
- `pc`/`cursor` (instruction offset)
- `stack` values (VM value type snapshot)
- call frames / base pointers
- wait/sleep state, and any scheduling metadata
- exception/try depth, if present
- any per-context registers/flags required by bytecode execution

Notes:
- Pointers/handles must not be stored; only logical data.
- If the runtime uses indices into tables, store indices, not raw pointers.

### Restore semantics
Apply order (recommended):
1. Clear VM runtime (stop execution, discard old thread state).
2. Restore globals/flags/history/timers.
3. Restore thread contexts and select `current_id`.
4. Ensure transient input edges are reset (InputManager excluded; do not synthesize clicks).

---

## MotionSnapshotV1 (0x0201)
Captures render/scene state excluding video.

### Fields
- `prim_manager`: `PrimManagerSnapshotV1`
- `motion_containers`: state of alpha/move/rot/scale/z/v3d/anim, etc.
- `snow`: `SnowSnapshotV1`
- `graphs`: `GraphSetSnapshotV1`
- `dissolve2`: dissolve2 state (and its mask graph if applicable)
- any other deterministic render state that affects output

### PrimManagerSnapshotV1
- `custom_root_prim_id: i16`
- `prims: Vec<PrimSnapshotV1>` (size == prim_count, stable ordering)

PrimSnapshotV1 must include:
- tree links: first_child/next_sibling/parent as IDs or indices
- visibility flags, alpha, blend, clip, transform, z (even if z not used for sorting)
- prim type (sprite/text/etc.) and parameters needed to rebuild GPU submission

### GraphSetSnapshotV1
Represents graph buffers (textures) by logical ID:
- `entries: Vec<GraphEntrySnapshotV1>`

Each entry:
- `graph_id: u16`
- `payload: GraphPayloadSnapshotV1`

#### GraphPayloadSnapshotV1
Two representations to control file size while preserving semantics:

1) Path-based (rebuildable):
- `kind = Path`
- `texture_path: String`
- `color_tone: (u8 r, u8 g, u8 b)` or normalized (0..200) depending on engine
- `offset/u/v/w/h` fields if relevant

2) Pixel-based (authoritative):
- `kind = Pixels`
- `width: u32`
- `height: u32`
- `format: enum { Rgba8 }` (extendable)
- `pixels: Vec<u8>` (len = width*height*4)
- `metadata`: offset/u/v/rgb, etc.

Selection rule (recommended):
- Use Path representation only if the graph is “pure”: directly loaded from `texture_path` and has no pixel mutations (no copies, blending, tone, masks applied).
- Otherwise use Pixels representation to preserve exact result.
- Do not snapshot text-buffer graphs (e.g., 4064..4095); those are restored by TextSnapshotV1 and re-uploaded.

### SnowSnapshotV1
Because fixed-size arrays can be problematic for serde across toolchains, represent large arrays as Vec:
- all fields needed to resume snow effect deterministically
- any per-particle arrays stored as Vec with explicit length validation on load

### Restore semantics
Apply order (recommended):
1. Rebuild/restore graphs (create textures, upload pixels).
2. Restore prim tree and prim attributes.
3. Restore motion containers (so subsequent ticks continue from saved state).
4. Restore dissolve state, snow, etc.
5. Mark affected resources dirty so GPU binds are refreshed.

---

## TextSnapshotV1 (0x0301)
Goal: restore text semantics and current rendered output without depending on font caches.

### Strategy
- Store “rebuildable fields” plus a pixel buffer for each active text slot.
- On restore:
  - Re-parse content to rebuild internal items.
  - Restore reveal/progress state (elapsed/visible_chars).
  - Restore pixel buffer and set dirty=false.
  - Upload to the corresponding text graph slot.

### Fields
- `slots: Vec<TextSlotSnapshotV1>` (one entry per text slot index used)

Each TextSlotSnapshotV1:
- `slot_id: u16` (0..31 or engine-defined)
- Rebuildable fields:
  - `content_text: String`
  - `text_content: String` (if distinct)
  - `font_id / face_name` (logical identifier, not pointers)
  - `font_size`, `t_size`
  - formatting: color, outline, shadow, offsets, ruby settings, etc.
  - suspension state: `is_suspended`, `suspend_chrs`, etc.
  - reveal/progress: `elapsed`, `visible_chars`, `total_chars`, `speed`, `skip_mode`
- Pixel buffer fields:
  - `width: u32`
  - `height: u32`
  - `pixel_rgba: Vec<u8>` (len = w*h*4)
  - `dirty: bool` (should be restored as false if pixel buffer is authoritative)

Notes:
- Do not store FontItem or platform font handles.
- Font matching should be by a stable logical key (face name / id) and resolved at load.

---

## PartsSnapshotV1 (0x0401)
Restores character parts state.

### Fields
- `parts: Vec<PartsItemSnapshotV1>` size == 64
- `motions: Vec<PartsMotionSnapshotV1>` size == 8
- `allocation_pool: Vec<u8>`
- `current_id: u8`

PartsItemSnapshotV1:
- `texture_name: String` (empty if none)
- `loaded: bool`
- `running: bool`
- `rgb: (u8,u8,u8)` or engine-specific range

PartsMotionSnapshotV1:
- `running: bool`
- `parts_id: u8`
- `entry_id: u8`
- `elapsed: u32`
- `duration: u32`
- `id: u8`

Restore semantics:
- For each loaded part with a non-empty name:
  - read bytes from VFS and call the normal loader (no direct texture serialization)
- Apply rgb/tone parameters after loading.
- Restore motion scheduling fields and allocation state.

---

## GaijiSnapshotV1 (0x0501)
Restores gaiji mapping.

### Fields
- `items: Vec<GaijiItemSnapshotV1>`

GaijiItemSnapshotV1:
- `codepoint: u32`
- `size: u32`
- `filename: String`

Restore semantics:
- Load `filename` bytes via VFS and re-register gaiji (same API as normal path).

---

## AudioSnapshotV1 (0x0601)
Restores audio “state” only (not precise playback position), per requirements.

### Fields
- `bgm_tracks: Vec<AudioTrackSnapshotV1>`
- `se_tracks: Vec<AudioTrackSnapshotV1>`
- `master_volume: Option<f32>` (if applicable)

AudioTrackSnapshotV1:
- `slot: u8`
- `name: String` (path/key used by VFS)
- `kind: u8` or enum (engine-specific)
- `muted: bool`
- `volume: f32`
- `pan: f32` (optional)
- `looped: bool`
- `playing: bool`

Restore semantics:
- Stop all tracks.
- For each track snapshot with non-empty `name`:
  - VFS load bytes
  - load_named(slot, name, bytes)
  - set volume/mute/pan
  - if `playing` then play with `looped` flag
- Playback position is not restored.

---

## Compatibility and Evolution Rules

### Container version (`RVSS.ver`)
- Increment when the container header semantics change.
- v1 readers must reject higher container versions unless explicitly supported.

### Block tags
- Tag encodes subsystem + block version (low byte).
- Introduce a new tag for breaking changes rather than changing old structs.

### Non-breaking changes within a block
- Prefer adding fields with `serde(default)` and keeping old fields intact.
- Avoid changing field types.
- If a breaking change is required, publish `...V2` under a new tag.

### Unknown blocks
- Must be ignored by skipping `len` bytes.
- Load must still succeed if required blocks are present.

---

## Exclusions (Explicit)
- Video playback status is NOT saved/restored.
- InputManager state is NOT saved/restored (no key edges, no mouse button edges).
- Any transient UI hover/click edges must not be synthesized on load.

---

## Restore Order (Recommended)
1. Parse SaveItemHeader + strings + thumbnail (for UI / metadata).
2. If SnapshotChunk present:
   - Read RVSS header, decompress payload if needed.
   - Parse TLV blocks into a map by tag.
3. Apply snapshots in this order:
   - MotionSnapshotV1 (graphs first, then prim tree, then motion containers)
   - PartsSnapshotV1
   - GaijiSnapshotV1
   - TextSnapshotV1 (re-upload text buffers)
   - AudioSnapshotV1 (recreate track states)
   - VmSnapshotV1 (globals/flags/timers/history, then thread contexts)
4. Clear any transient runtime queues (e.g., pending syscalls) so execution continues cleanly.

---

## Validation and Safety Checks
- All lengths must be bounds-checked against file size.
- TLV parsing must reject blocks with `len` exceeding remaining payload.
- Snapshot application must validate indices:
  - prim_id range
  - graph_id range
  - text slot range
  - audio slot range
- If snapshot is corrupt:
  - fall back to metadata-only load, or report a recoverable error.

---

## Determinism Notes
- If snow or other effects depend on RNG, store RNG seed/state in VmSnapshotV1 (optional).
- If timers drive script semantics, TimerManagerSnapshotV1 must be included.


