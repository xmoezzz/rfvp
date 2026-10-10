# Experimental ports

Host crates for 3DS, Wii, Wii U, PS2 and PS3. They exist so the `no_std` core has a place to plug
into on these consoles, but **none of them has been run**, and they are not built on every change.
Expect a game to need repacking at a lower resolution before it can fit.

Each directory has its build script (`build-<name>.sh`) and, where needed, a custom target spec.
`.github/workflows/build-homebrew-artifacts.yml` can build some of them by hand.
