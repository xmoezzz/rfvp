# Version 0.6.1

This release adds support for getting raw opus audio from nxa files to `sdu`. This allows preserving audio data without
additional re-encoding losses.

It also updates various dependencies of the engine, but this shouldn't change the visible functionality.

# Version 0.6.0

This is just an update of the changes accumulated over they year. This doesn't improve compat, mostly small internal
changes

- Add a very basic linear scenario disassembler. The output format is not stable yet, probably should work on making it
  compatible with what `shin-asm` expects.
- Implement & expose in sdu `shin-asm`: a way-too-much over-engineered assembler for SNR files. It is still largely
  unfinished and can assembly only very basic files.
- Add an optional gstreamer backend to `shin-video`, allowing for hardware-accelerated decoding. Not build by default,
  unsure on how to distribute on windows yet.
- Use typed `NumberSpec` values, which will get lowered to a more concrete type than `i32` when computed.
- Various dependency updates.

# Before that

Before this, I did not keep any changelog, sorry. Looking at the commits is the best you will get
