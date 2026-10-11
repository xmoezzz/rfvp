# The PS2 build: the engine (platform/ps2/host, Rust, built by ../build_ps2.sh) linked with this
# runtime and ps2sdk/gsKit into one ELF.
#
#   make            -> $(OUT)/rfvp-ps2.elf
#
# Needs PS2DEV/PS2SDK (ps2dev toolchain) and Homebrew LLVM (see ../build_ps2.sh).

PS2DEV ?= $(HOME)/ps2dev
PS2SDK ?= $(PS2DEV)/ps2sdk
GSKIT ?= $(PS2DEV)/gsKit
REPO := $(abspath ../../..)
OUT ?= $(REPO)/target/ps2
CARGO_TARGET_DIR ?= $(REPO)/target

EE_PREFIX := mips64r5900el-ps2-elf-
CC := $(PS2DEV)/ee/bin/$(EE_PREFIX)gcc
BIN2C := $(PS2SDK)/bin/bin2c

CFLAGS := -D_EE -G0 -O2 -Wall \
	-I$(PS2SDK)/ee/include -I$(PS2SDK)/common/include -I$(GSKIT)/include \
	-I$(REPO)/crates/rfvp/vendor
# The engine needs more than ps2sdk's default 128 KiB of stack.
LDFLAGS := -T$(PS2SDK)/ee/startup/linkfile -L$(PS2SDK)/ee/lib -L$(GSKIT)/lib \
	-Wl,-zmax-page-size=128 -Wl,--defsym=_stack_size=0x100000
LIBS := -lgskit -ldmakit -lpad -laudsrv -lpatches -ldebug -lm -lgcc

RUST_OBJ := $(CARGO_TARGET_DIR)/ps2/rfvp_ps2.o
OBJS := $(OUT)/main.o $(OUT)/rfvp_ogg_vorbis.o $(OUT)/audsrv_irx.o

.PHONY: all rust clean
all: $(OUT)/rfvp-ps2.elf

rust:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) bash ../build_ps2.sh ../host rfvp_ps2

$(RUST_OBJ): rust

$(OUT)/main.o: main.c
	@mkdir -p $(OUT)
	$(CC) $(CFLAGS) -c $< -o $@

$(OUT)/rfvp_ogg_vorbis.o: $(REPO)/crates/rfvp/vendor/rfvp_ogg_vorbis.c $(REPO)/crates/rfvp/vendor/stb_vorbis.c
	@mkdir -p $(OUT)
	$(CC) $(CFLAGS) -w -c $< -o $@

$(OUT)/audsrv_irx.c: $(PS2SDK)/iop/irx/audsrv.irx
	@mkdir -p $(OUT)
	$(BIN2C) $< $@ audsrv_irx

$(OUT)/audsrv_irx.o: $(OUT)/audsrv_irx.c
	$(CC) $(CFLAGS) -c $< -o $@

$(OUT)/rfvp-ps2.elf: $(OBJS) rust
	$(CC) $(LDFLAGS) -o $@ $(OBJS) $(RUST_OBJ) $(LIBS)
	@echo $@

clean:
	rm -f $(OBJS) $(OUT)/audsrv_irx.c $(OUT)/rfvp-ps2.elf
