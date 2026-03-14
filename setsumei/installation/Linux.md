# Linux Installation

On Linux, you should build the project yourself.

Unlike Windows, Linux environments are not uniform. The glibc version, system libraries, and distribution packaging can differ across Oses, so a prebuilt binary from one environment may not run correctly on another.

This project is also not distributed in bundle mode.

## What this means

You are expected to:

- build `rfvp` on your own Linux system
- provide the original game data files yourself
- prepare the directory layout manually

## Supported Build Environment

The current CI builds Linux on:

- `ubuntu-22.04`
- `x86_64`

This project should be compatible with other Linux distributions and architectures, but they are not tested in CI. 

## Requirements

Before starting, make sure you have all of the following:

- A 64-bit Linux system
- The Rust toolchain
- The original game data files from your own installation

Based on the current CI configuration (Ubuntu 22.04), the build environment also installs these packages:

- `pkg-config`
- `libunwind-dev`
- `libasound2-dev`
- `libgstreamer1.0-dev`
- `libgstreamer-plugins-base1.0-dev`
- `gstreamer1.0-plugins-base`
- `gstreamer1.0-plugins-good`
- `gstreamer1.0-plugins-bad`
- `gstreamer1.0-plugins-ugly`
- `gstreamer1.0-libav`
- `libgstrtspserver-1.0-dev`
- `libges-1.0-dev`

## Build Instructions

On Ubuntu 22.04, the CI uses the following commands:

```bash
sudo apt-get update
sudo apt-get remove -y libunwind-14-dev libc++-14-dev libc++-dev || true
sudo apt-get install -y libunwind-dev
sudo apt-get install -y \
  pkg-config \
  libasound2-dev \
  libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly \
  gstreamer1.0-libav \
  libgstrtspserver-1.0-dev \
  libges-1.0-dev
```

Then build the project with:

```bash
cargo build --release -p rfvp
```

The resulting binary will be located at:

```text
target/release/rfvp
```

## Directory Layout

Create a folder for the game and place the files in a layout like this:

```text
<GameRoot>/
  rfvp
  <game data files or directories>
```

## Running

1. Place `rfvp` in your game directory.
2. Make the executable runnable:

```bash
chmod +x rfvp
```

4. Launch the game:

```bash
./rfvp
```

