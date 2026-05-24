//! Intel HD Audio (HDA) driver for UEFI x86_64 and aarch64.
//!
//! ## Hardware overview
//!
//! Intel HDA is a PCI multimedia controller (class 04/03).  Unlike AC97 it uses
//! a single 64-bit MMIO BAR (BAR0) for all register access; the codec verbatim
//! protocol uses CORB (host→codec) and RIRB (codec→host) ring buffers.
//!
//! DMA output uses a cyclic ring of Buffer Descriptor List (BDL) entries; each
//! entry points to one PCM sub-buffer.  We use BDL_ENTRIES = 4 sub-buffers of
//! BUF_FRAMES stereo frames each.
//!
//! ## PCI config space access
//!
//! * x86_64: CF8/CFC I/O ports (same as AC97)
//! * aarch64: PCIe ECAM MMIO at a base found from ACPI MCFG.  Call
//!   `set_ecam_base()` before `HdaDriver::new()`.

use core::sync::atomic::{AtomicU64, Ordering};
use super::AudioDriver;

// ─── ECAM base (aarch64 only) ─────────────────────────────────────────────────

/// Physical base address of the PCIe ECAM window.
/// Must be set by platform init before `create_driver()` on aarch64.
static ECAM_BASE: AtomicU64 = AtomicU64::new(0);

pub fn set_ecam_base(base: u64) {
    ECAM_BASE.store(base, Ordering::Relaxed);
    log::info!("anzu-hal HDA: ECAM base set to 0x{:016x}", base);
}

// ─── MMIO helpers ─────────────────────────────────────────────────────────────

#[inline(always)]
unsafe fn rd32(base: u64, off: u32) -> u32 {
    ((base + off as u64) as *const u32).read_volatile()
}
#[inline(always)]
unsafe fn wr32(base: u64, off: u32, v: u32) {
    ((base + off as u64) as *mut u32).write_volatile(v);
}
#[inline(always)]
unsafe fn rd16(base: u64, off: u32) -> u16 {
    ((base + off as u64) as *const u16).read_volatile()
}
#[inline(always)]
unsafe fn wr16(base: u64, off: u32, v: u16) {
    ((base + off as u64) as *mut u16).write_volatile(v);
}
#[inline(always)]
unsafe fn rd8(base: u64, off: u32) -> u8 {
    ((base + off as u64) as *const u8).read_volatile()
}
#[inline(always)]
unsafe fn wr8(base: u64, off: u32, v: u8) {
    ((base + off as u64) as *mut u8).write_volatile(v);
}

// ─── PCI config space (arch-specific) ─────────────────────────────────────────
//
// Only x86_64 (CF8/CFC I/O ports) and aarch64 (PCIe ECAM) are implemented.
// Adding a new target requires a new `pcicfg` module below.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("anzu-hal HDA: unsupported target_arch — only x86_64 and aarch64 are supported");

// ─── x86_64: legacy CF8/CFC I/O-port PCI config ──────────────────────────────
#[cfg(target_arch = "x86_64")]
mod pcicfg {
    const ADDR: u16 = 0xCF8;
    const DATA: u16 = 0xCFC;

    #[inline(always)]
    fn make_addr(bus: u8, dev: u8, func: u8, reg: u16) -> u32 {
        0x80000000
            | ((bus as u32) << 16)
            | ((dev as u32) << 11)
            | ((func as u32) << 8)
            | (reg as u32 & 0xFC)
    }

    // Intel syntax (Rust x86 inline-asm default — no att_syntax).
    pub unsafe fn read32(bus: u8, dev: u8, func: u8, reg: u16) -> u32 {
        let addr = make_addr(bus, dev, func, reg);
        // Select config register
        core::arch::asm!("out dx, eax", in("dx") ADDR, in("eax") addr, options(nostack, preserves_flags));
        let v: u32;
        // Read data
        core::arch::asm!("in eax, dx", in("dx") DATA, out("eax") v, options(nostack, preserves_flags));
        v
    }
    pub unsafe fn write32(bus: u8, dev: u8, func: u8, reg: u16, val: u32) {
        let addr = make_addr(bus, dev, func, reg);
        core::arch::asm!("out dx, eax", in("dx") ADDR, in("eax") addr, options(nostack, preserves_flags));
        core::arch::asm!("out dx, eax", in("dx") DATA, in("eax") val,  options(nostack, preserves_flags));
    }
    pub unsafe fn read16(bus: u8, dev: u8, func: u8, reg: u16) -> u16 {
        let v = read32(bus, dev, func, reg & !3);
        ((v >> ((reg & 2) * 8)) & 0xFFFF) as u16
    }
    pub unsafe fn write16(bus: u8, dev: u8, func: u8, reg: u16, val: u16) {
        let shift = ((reg & 2) * 8) as u32;
        let old = read32(bus, dev, func, reg & !3);
        write32(bus, dev, func, reg & !3, (old & !(0xFFFF << shift)) | ((val as u32) << shift));
    }
}

#[cfg(target_arch = "aarch64")]
mod pcicfg {
    use super::ECAM_BASE;
    use core::sync::atomic::Ordering;

    unsafe fn ecam_ptr(bus: u8, dev: u8, func: u8, reg: u16) -> Option<*mut u32> {
        let base = ECAM_BASE.load(Ordering::Relaxed);
        if base == 0 { return None; }
        let addr = base
            | ((bus as u64) << 20)
            | ((dev as u64) << 15)
            | ((func as u64) << 12)
            | (reg as u64 & 0xFFC);
        Some(addr as *mut u32)
    }

    pub unsafe fn read32(bus: u8, dev: u8, func: u8, reg: u16) -> u32 {
        ecam_ptr(bus, dev, func, reg)
            .map(|p| p.read_volatile())
            .unwrap_or(0xFFFF_FFFF)
    }
    pub unsafe fn write32(bus: u8, dev: u8, func: u8, reg: u16, val: u32) {
        if let Some(p) = ecam_ptr(bus, dev, func, reg) { p.write_volatile(val); }
    }
    pub unsafe fn read16(bus: u8, dev: u8, func: u8, reg: u16) -> u16 {
        let v = read32(bus, dev, func, reg & !3);
        ((v >> ((reg & 2) * 8)) & 0xFFFF) as u16
    }
    pub unsafe fn write16(bus: u8, dev: u8, func: u8, reg: u16, val: u16) {
        let shift = ((reg & 2) * 8) as u32;
        let old = read32(bus, dev, func, reg & !3);
        write32(bus, dev, func, reg & !3, (old & !(0xFFFF << shift)) | ((val as u32) << shift));
    }
}

// ─── HDA register offsets ─────────────────────────────────────────────────────

const GCAP:      u32 = 0x00;
const GCTL:      u32 = 0x08;
const STATESTS:  u32 = 0x0E;
const CORBLBASE: u32 = 0x40;
const CORBUBASE: u32 = 0x44;
const CORBWP:    u32 = 0x48;
const CORBRP:    u32 = 0x4A;
const CORBCTL:   u32 = 0x4C;
const CORBSIZE:  u32 = 0x4E;
const RIRBLBASE: u32 = 0x50;
const RIRBUBASE: u32 = 0x54;
const RIRBWP:    u32 = 0x58;
const RINTCNT:   u32 = 0x5A;
const RIRBCTL:   u32 = 0x5C;
const RIRBSIZE:  u32 = 0x5E;

// Output Stream Descriptor register offsets relative to OSD base
const SD_CTL:   u32 = 0x00; // 8-bit
const SD_STS:   u32 = 0x03; // 8-bit
const SD_LPIB:  u32 = 0x04; // 32-bit
const SD_CBL:   u32 = 0x08; // 32-bit (cyclic buffer length in bytes)
const SD_LVI:   u32 = 0x0C; // 16-bit (last valid BDL index)
const SD_FMT:   u32 = 0x12; // 16-bit
const SD_BDLPL: u32 = 0x18; // 32-bit
const SD_BDLPU: u32 = 0x1C; // 32-bit

// HDA verb constants
const VRB_GET_PARAM:       u32 = 0xF0000;
const VRB_SET_PWRSTATE:    u32 = 0x70500;
const VRB_SET_CONVERTER_FMT: u32 = 0x20000;
const VRB_SET_CONVERTER_STREAM: u32 = 0x70600;
const VRB_SET_AMP_GAIN:    u32 = 0x30000;
const VRB_SET_PIN_CTRL:    u32 = 0x70700;

const PARAM_NODE_COUNT:    u32 = 0x04;
const PARAM_WIDGET_CAP:    u32 = 0x09;

// HDA PCM format: 48 kHz, 16-bit, stereo
// bits[14:11]=0 (48 kHz base), bits[10:8]=0 (÷1), bits[7:4]=1 (16-bit), bits[3:0]=1 (2ch)
const FMT_48K_16B_STEREO: u16 = 0x0011;

// DMA ring
const BDL_ENTRIES: usize = 4;
const BUF_FRAMES:  usize = 1024;
const BUF_BYTES:   usize = BUF_FRAMES * 4; // 4 bytes per stereo frame

// ─── DMA region ───────────────────────────────────────────────────────────────

#[repr(C)]
struct BdlEntry {
    addr_lo: u32,
    addr_hi: u32,
    len:     u32,
    ioc:     u32,
}

#[repr(C, align(4096))]
struct HdaRegion {
    corb: [u32; 256],               //  1024 bytes, 128-byte aligned at offset 0
    _pad0: [u8; 128],               //   pad to keep rirb 128-aligned (1024+128=1152 → 1152%128=0 ✓)
    rirb: [u64; 256],               //  2048 bytes at offset 1152
    _pad1: [u8; 32],                //   pad: 1152+2048=3200, next 128-boundary = 3200 ✓
    bdl:  [BdlEntry; BDL_ENTRIES],  //    64 bytes at offset 3200 (128-aligned ✓)
    _pad2: [u8; 64],                //   pad to 128-align bufs: 3200+64+64=3328 ✓
    bufs: [[u8; BUF_BYTES]; BDL_ENTRIES], // 16384 bytes
}

// ─── CORB/RIRB ───────────────────────────────────────────────────────────────

struct Corb {
    bar0:    u64,
    wp:      u16,
    rirb_wp: u16,
}

impl Corb {
    /// Send one verb and return the 32-bit response.
    unsafe fn send(&mut self, region: &mut HdaRegion, verb: u32) -> u32 {
        self.wp = self.wp.wrapping_add(1) & 0xFF;
        region.corb[self.wp as usize] = verb;
        // Flush write before bumping WP
        core::sync::atomic::fence(Ordering::SeqCst);
        wr16(self.bar0, CORBWP, self.wp);

        // Poll RIRBWP until it advances
        for _ in 0..2_000_000u32 {
            let wp = rd16(self.bar0, RIRBWP) & 0xFF;
            if wp != self.rirb_wp {
                self.rirb_wp = wp;
                let resp = region.rirb[wp as usize] as u32;
                return resp;
            }
            core::hint::spin_loop();
        }
        log::warn!("anzu-hal HDA: CORB/RIRB timeout");
        0
    }

    /// Build and send an HDA verb: addr·nid·verb(12-bit)·payload(8-bit)
    unsafe fn verb(&mut self, region: &mut HdaRegion,
                   caddr: u8, nid: u8, verb: u32, payload: u16) -> u32 {
        let word = ((caddr as u32) << 28)
            | ((nid as u32) << 20)
            | (verb & 0xFF_FF0)
            | (payload as u32 & 0xFFFFF);
        self.send(region, word)
    }
}

// ─── PCI scan ─────────────────────────────────────────────────────────────────

unsafe fn find_hda() -> Option<(u8, u8, u8)> {
    for bus in 0u8..=255 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = pcicfg::read32(bus, dev, func, 0x00);
                if id == 0xFFFF_FFFF {
                    if func == 0 { break; }
                    continue;
                }
                // PCI class: 0x04 multimedia; subclass 0x03 = HDA
                let class_rev = pcicfg::read32(bus, dev, func, 0x08);
                let class = (class_rev >> 8) as u32;
                if class == 0x0403 {
                    return Some((bus, dev, func));
                }
                // Also match by known IDs (QEMU ICH6 HDA)
                let vendor = id & 0xFFFF;
                let device = id >> 16;
                if (vendor == 0x8086 && (device == 0x2668 || device == 0x293E))
                    || (vendor == 0x1B36 && device == 0x000D)  // QEMU generic HDA
                {
                    return Some((bus, dev, func));
                }
                if func == 0 {
                    let hdr = (pcicfg::read32(bus, dev, 0, 0x0C) >> 16) as u8;
                    if (hdr & 0x80) == 0 { break; }
                }
            }
        }
    }
    None
}

// ─── HDA driver ───────────────────────────────────────────────────────────────

pub struct HdaDriver {
    available:  bool,
    bar0:       u64,         // MMIO base of HDA controller
    osd_base:   u32,         // register offset of first output stream descriptor
    region:     Option<Box<HdaRegion>>,
    write_buf:  usize,       // next BDL buffer index to fill
}

impl HdaDriver {
    pub fn new() -> Self {
        let mut drv = Self {
            available: false,
            bar0: 0,
            osd_base: 0,
            region: None,
            write_buf: 0,
        };
        unsafe { drv.try_init(); }
        drv
    }

    unsafe fn try_init(&mut self) {
        let Some((bus, dev, func)) = find_hda() else { return };

        let id = pcicfg::read32(bus, dev, func, 0x00);
        let vid = id & 0xFFFF;
        let did = id >> 16;
        log::info!("anzu-hal HDA: found PCI {:04x}:{:04x} at {:02x}:{:02x}.{}", vid, did, bus, dev, func);

        // Enable bus master + memory space
        let cmd = pcicfg::read16(bus, dev, func, 0x04);
        pcicfg::write16(bus, dev, func, 0x04, cmd | 0x06);

        // BAR0 = 64-bit MMIO base
        let bar0_lo = pcicfg::read32(bus, dev, func, 0x10);
        let bar0_hi = pcicfg::read32(bus, dev, func, 0x14);
        let bar0_type = (bar0_lo >> 1) & 0x3;
        let bar0 = if bar0_type == 0x2 {
            // 64-bit BAR
            ((bar0_hi as u64) << 32) | (bar0_lo & !0xF) as u64
        } else {
            (bar0_lo & !0xF) as u64
        };

        if bar0 == 0 {
            log::warn!("anzu-hal HDA: BAR0 is 0, cannot use");
            return;
        }
        self.bar0 = bar0;
        log::info!("anzu-hal HDA: BAR0=0x{:016x}", bar0);

        // Global reset
        wr32(bar0, GCTL, 0);
        for _ in 0..100_000u32 { core::hint::spin_loop(); }
        wr32(bar0, GCTL, 1);
        for _ in 0..2_000_000u32 {
            if rd32(bar0, GCTL) & 1 != 0 { break; }
            core::hint::spin_loop();
        }
        // Wait for codec ready
        for _ in 0..2_000_000u32 {
            if rd16(bar0, STATESTS) != 0 { break; }
            core::hint::spin_loop();
        }

        // Determine OSD base: OSD starts at 0x80 + ISS×0x20
        let gcap = rd16(bar0, GCAP);
        let iss = ((gcap >> 8) & 0xF) as u32;
        let oss = ((gcap >> 12) & 0xF) as u32;
        if oss == 0 {
            log::warn!("anzu-hal HDA: no output streams");
            return;
        }
        self.osd_base = 0x80 + iss * 0x20;
        log::info!("anzu-hal HDA: ISS={} OSS={} OSD_BASE=0x{:x}", iss, oss, self.osd_base);

        // Allocate DMA region
        let mut region = Box::new(HdaRegion {
            corb:  [0u32; 256],
            _pad0: [0u8;  128],
            rirb:  [0u64; 256],
            _pad1: [0u8;   32],
            bdl:   [(); BDL_ENTRIES].map(|_| BdlEntry { addr_lo: 0, addr_hi: 0, len: 0, ioc: 0 }),
            _pad2: [0u8;   64],
            bufs:  [[0u8; BUF_BYTES]; BDL_ENTRIES],
        });

        let phys_base = region.as_ref() as *const HdaRegion as u64;
        log::info!("anzu-hal HDA: DMA region phys=0x{:016x}", phys_base);

        // CORB setup
        let corb_phys = phys_base + core::mem::offset_of!(HdaRegion, corb) as u64;
        wr32(bar0, CORBLBASE, corb_phys as u32);
        wr32(bar0, CORBUBASE, (corb_phys >> 32) as u32);
        wr8(bar0, CORBSIZE, 0x02);  // 256 entries
        // Reset CORBWP and CORBRP
        wr16(bar0, CORBWP, 0);
        wr16(bar0, CORBRP, 0x8000); // set reset bit
        for _ in 0..100_000u32 {
            if rd16(bar0, CORBRP) & 0x8000 != 0 { break; }
            core::hint::spin_loop();
        }
        wr16(bar0, CORBRP, 0); // clear reset
        wr8(bar0, CORBCTL, 0x02); // DMA run

        // RIRB setup
        let rirb_phys = phys_base
            + core::mem::offset_of!(HdaRegion, corb) as u64
            + core::mem::size_of::<[u32; 256]>() as u64
            + 128; // _pad0
        wr32(bar0, RIRBLBASE, rirb_phys as u32);
        wr32(bar0, RIRBUBASE, (rirb_phys >> 32) as u32);
        wr8(bar0, RIRBSIZE, 0x02);  // 256 entries
        wr16(bar0, RIRBWP, 0x8000); // reset write pointer
        wr16(bar0, RINTCNT, 1);
        wr8(bar0, RIRBCTL, 0x02); // DMA run

        let mut corb = Corb { bar0, wp: 0, rirb_wp: 0 };

        // ── Codec enumeration ─────────────────────────────────────────────
        // Find first codec (bit set in STATESTS)
        let statests = rd16(bar0, STATESTS);
        let caddr = (0..15u8).find(|&i| statests & (1 << i) != 0);
        let caddr = match caddr {
            Some(c) => c,
            None => {
                log::warn!("anzu-hal HDA: no codec found");
                return;
            }
        };
        log::info!("anzu-hal HDA: codec at address {}", caddr);

        // Root node: GET_PARAM(SUBORDINATE_NODE_COUNT) → find AFG
        let root_nodes = corb.verb(&mut region, caddr, 0, VRB_GET_PARAM, PARAM_NODE_COUNT as u16);
        let afg_start = ((root_nodes >> 16) & 0xFF) as u8;
        let afg_count = (root_nodes & 0xFF) as u8;
        log::info!("anzu-hal HDA: root node: {} function groups starting at {}", afg_count, afg_start);

        // Find Audio Function Group (type 0x01)
        let mut afg_nid: Option<u8> = None;
        for n in afg_start..(afg_start + afg_count) {
            let typ = corb.verb(&mut region, caddr, n, VRB_GET_PARAM, 0x05) & 0xFF;
            if typ == 0x01 {
                afg_nid = Some(n);
                break;
            }
        }
        let afg_nid = match afg_nid {
            Some(n) => n,
            None => {
                log::warn!("anzu-hal HDA: no AFG found");
                return;
            }
        };
        log::info!("anzu-hal HDA: AFG at NID {}", afg_nid);

        // Power up AFG
        corb.verb(&mut region, caddr, afg_nid, VRB_SET_PWRSTATE, 0x00);

        // Enumerate AFG widgets: find first output converter and output pin
        let widget_nodes = corb.verb(&mut region, caddr, afg_nid, VRB_GET_PARAM, PARAM_NODE_COUNT as u16);
        let wgt_start = ((widget_nodes >> 16) & 0xFF) as u8;
        let wgt_count = (widget_nodes & 0xFF) as u8;

        let mut dac_nid: Option<u8> = None;
        let mut pin_nid: Option<u8> = None;

        for n in wgt_start..(wgt_start + wgt_count) {
            let cap = corb.verb(&mut region, caddr, n, VRB_GET_PARAM, PARAM_WIDGET_CAP as u16);
            let wtype = (cap >> 20) & 0xF;
            match wtype {
                0x0 if dac_nid.is_none() => { // Audio Out
                    dac_nid = Some(n);
                    log::info!("anzu-hal HDA: DAC at NID {}", n);
                }
                0x4 => { // Pin widget
                    // Check pin default config (0xF1C) to find a physical output pin
                    // connectivity bits [31:30]: 00=no conn, 01=jack, 10=no jack, 11=fixed
                    let def_cfg = corb.verb(&mut region, caddr, n, 0xF1C00, 0);
                    let conn = (def_cfg >> 30) & 0x3;
                    let default_dev = (def_cfg >> 20) & 0xF;
                    // default_dev: 0=Line Out, 1=Speaker, 2=HP Out, 3=CD, ...
                    if conn != 0 && default_dev <= 2 && pin_nid.is_none() {
                        pin_nid = Some(n);
                        log::info!("anzu-hal HDA: output pin at NID {}", n);
                    }
                }
                _ => {}
            }
        }

        let dac_nid = match dac_nid {
            Some(n) => n,
            None => {
                log::warn!("anzu-hal HDA: no output converter found");
                return;
            }
        };

        // ── Codec configuration ────────────────────────────────────────────

        // Set DAC stream/channel: stream 1, channel 0
        corb.verb(&mut region, caddr, dac_nid, VRB_SET_CONVERTER_STREAM, 0x10);
        // Set DAC format: 48 kHz, 16-bit, stereo
        corb.verb(&mut region, caddr, dac_nid, VRB_SET_CONVERTER_FMT, FMT_48K_16B_STEREO);
        // Unmute DAC output amplifier (set=output, L+R, gain=0, mute=0)
        corb.verb(&mut region, caddr, dac_nid, VRB_SET_AMP_GAIN, 0xB000);

        if let Some(pin) = pin_nid {
            // Enable output on pin widget
            corb.verb(&mut region, caddr, pin, VRB_SET_PIN_CTRL, 0x40); // EPD=1
            // Unmute pin amplifier
            corb.verb(&mut region, caddr, pin, VRB_SET_AMP_GAIN, 0xB000);
        }

        // ── Output Stream Descriptor 0 setup ──────────────────────────────

        let osd = self.osd_base;

        // Stop stream, reset
        wr8(bar0, osd + SD_CTL, 0x00);
        for _ in 0..50_000u32 { core::hint::spin_loop(); }
        wr8(bar0, osd + SD_CTL, 0x01); // Stream Reset
        for _ in 0..50_000u32 { core::hint::spin_loop(); }
        wr8(bar0, osd + SD_CTL, 0x00); // Clear reset

        // Populate BDL
        let bufs_phys = phys_base + core::mem::offset_of!(HdaRegion, bufs) as u64;
        for i in 0..BDL_ENTRIES {
            let buf_phys = bufs_phys + (i * BUF_BYTES) as u64;
            region.bdl[i].addr_lo = buf_phys as u32;
            region.bdl[i].addr_hi = (buf_phys >> 32) as u32;
            region.bdl[i].len = BUF_BYTES as u32;
            region.bdl[i].ioc = 0; // no interrupts needed
        }

        // Configure OSD
        let bdl_phys = phys_base + core::mem::offset_of!(HdaRegion, bdl) as u64;
        wr32(bar0, osd + SD_BDLPL, bdl_phys as u32);
        wr32(bar0, osd + SD_BDLPU, (bdl_phys >> 32) as u32);
        wr32(bar0, osd + SD_CBL, (BDL_ENTRIES * BUF_BYTES) as u32);
        wr16(bar0, osd + SD_LVI, (BDL_ENTRIES - 1) as u16);
        wr16(bar0, osd + SD_FMT, FMT_48K_16B_STEREO);

        // Stream tag = 1, stripe = 0; use upper byte of CTL for stream tag
        wr8(bar0, osd + SD_CTL + 2, 0x10); // stream tag 1

        // Start the stream (SDCTL Run bit)
        wr8(bar0, osd + SD_CTL, 0x02); // Run

        self.region = Some(region);
        self.write_buf = 1; // start filling from buffer 1 (HW starts at 0)
        self.available = true;

        log::info!("anzu-hal HDA: DMA started, {} buffers × {} frames each", BDL_ENTRIES, BUF_FRAMES);
    }

    unsafe fn lpib(&self) -> u64 {
        rd32(self.bar0, self.osd_base + SD_LPIB) as u64
    }

    fn playing_buf(&self) -> usize {
        // Which BDL buffer the hardware is currently reading from
        let pos = unsafe { self.lpib() };
        ((pos / BUF_BYTES as u64) as usize) % BDL_ENTRIES
    }
}

impl AudioDriver for HdaDriver {
    fn is_available(&self) -> bool { self.available }

    fn frames_available(&self) -> usize {
        if !self.available { return 0; }
        let playing = self.playing_buf();
        // Number of buffers ahead of playing that we haven't written to yet
        // write_buf is where we'll write next; playing_buf is where HW is now
        // We can fill buffers from write_buf up to (but not including) playing_buf
        (playing + BDL_ENTRIES - self.write_buf) % BDL_ENTRIES * BUF_FRAMES
    }

    fn write_frames(&mut self, buf: &[i16], n_frames: usize) {
        if !self.available || n_frames == 0 { return; }
        // Read playing_buf before taking &mut self.region (avoids borrow conflict).
        let playing = self.playing_buf();
        let region = match self.region.as_mut() { Some(r) => r, None => return };
        let mut remaining = n_frames.min(buf.len() / 2);
        let mut src = 0usize;

        while remaining > 0 && self.write_buf != playing {
            let frames = remaining.min(BUF_FRAMES);
            let dst = &mut region.bufs[self.write_buf];
            for i in 0..frames {
                let l = buf[src + i * 2];
                let r = buf[src + i * 2 + 1];
                let lb = l.to_le_bytes();
                let rb = r.to_le_bytes();
                dst[i * 4]     = lb[0];
                dst[i * 4 + 1] = lb[1];
                dst[i * 4 + 2] = rb[0];
                dst[i * 4 + 3] = rb[1];
            }
            // Zero-pad remainder
            if frames < BUF_FRAMES {
                dst[frames * 4..].fill(0);
            }
            self.write_buf = (self.write_buf + 1) % BDL_ENTRIES;
            src += frames * 2;
            remaining -= frames;
        }
    }
}
