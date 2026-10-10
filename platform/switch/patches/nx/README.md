
# nx

> Userland library for Nintendo Switch homebrew (and other potential purposes), written in pure Rust and some assembly bits

> ### API docs are hosted [here](https://aarch64-switch-rs.github.io/nx/), and examples can be found [here](https://github.com/aarch64-switch-rs/examples)

> ### Setup guide to start developing Rust homebrew can be found [here](https://github.com/aarch64-switch-rs/nx/wiki/Setup-Guide)

## TODO list

- Finish implementing all IPC/SF interfaces (+ their results):

  - Finish applet services

  - Finish fatal services

  - Finish fs services

  - Finish hid services

  - Finish ldr services

  - Add lm:get

  - Finish mii services

  - Finish nv services

  - Support HTC/TMA?

  - Finish pm services

  - Finish psc services

  - Finish psm services

  - Finish settings services

  - Finish spl services

  - Finish usb services

  - Finish vi services

  - Add other unimplemented services not mentioned above

- TIPC server support

- More FS interfaces:

  - PFS0

  - Romfs

  - NCA

- NRO Romfs support

- Finish implementing all SVC wrappers.

- Actual hw-rendering? (maybe as a separate lib like [deko3d](https://github.com/devkitPro/deko3d)?)

- Finish SMC support

- Optimize IPC code to generate even better asm (like libnx or nnsdk)

- Finish documenting still-undocumented modules (`ipc`, `svc` and `service`)

## Credits

- [libnx](https://github.com/switchbrew/libnx) and its contributors for being the base of this project.

- [Atmosphère](https://github.com/Atmosphere-NX/Atmosphere) and its contributors for being another base of this project.
