# Windows Installation

Who needs this?

The original game was released on Windows anyway. But since this project is not distributed in bundle mode, the Windows build still needs a short manual setup.

## What this means

This build does not package the executable and game assets into a single ready-to-run bundle.

You must prepare the directory layout yourself.

## Requirements

Before starting, make sure you have all of the following:

- A 64-bit Windows system (Windows 8 or later, because Rust no longer supports Windows 7)
- 32-bit Windows is not supported
- The original game data files from your own installation
- Either a prebuilt `rfvp.exe`, or the Rust toolchain if you want to build the project yourself

## Supported Targets

The CI currently builds:

- x86_64 Windows (MSVC)
- arm64 Windows (MSVC)

## Directory Layout

Create a folder for the game and place the files in a layout like this:

```text
<GameRoot>/
  rfvp.exe
  <game data files or directories>