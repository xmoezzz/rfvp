//! Intel AC97 (ICH, ICH0-6) hardware audio driver for x86_64 UEFI.
//!
//! ## Hardware overview
//!
//! AC97 is a PCI multi-media audio controller with two I/O BAR regions:
//!   BAR0 (NAM):  Native Audio Mixer   – codec volume/rate registers
//!   BAR1 (NABM): Native Audio Bus Master – DMA engine and status registers
//!
//! The DMA engine (NABM) uses a Buffer Descriptor List (BDL): a ring of 32
//! entries, each pointing to a physical DMA buffer.  We double-buffer: while
//! the hardware plays one buffer, we fill the next via `tick()`.
//!
//! PCI configuration space is accessed through I/O ports 0xCF8/0xCFC
//! (Configuration Address/Data), which are always available on x86_64 before
//! ExitBootServices.

use super::AudioDriver;

// ─── PCI constants ────────────────────────────────────────────────────────────

const PCI_ADDR: u16 = 0xCF8;
const PCI_DATA: u16 = 0xCFC;

// Known AC97 PCI identities (vendor, device).
const KNOWN_AC97: &[(u16, u16)] = &[
    (0x8086, 0x2415), // Intel 82801AA (ICH)
    (0x8086, 0x2425), // Intel 82801AB (ICH0)
    (0x8086, 0x2445), // Intel 82801BA (ICH2)
    (0x8086, 0x2485), // Intel 82801CA (ICH3)
    (0x8086, 0x24C5), // Intel 82801DB (ICH4)
    (0x8086, 0x24D5), // Intel 82801EB (ICH5)
    (0x8086, 0x266E), // Intel 82801FB (ICH6)
    (0x8086, 0x27DE), // Intel 82801G  (ICH7)
    (0x1102, 0x0002), // Creative Labs ES1371
    (0x1274, 0x1371), // Ensoniq ES1371
    (0x10DE, 0x01B1), // nForce AC97
    (0x10DE, 0x006A), // nForce2 AC97
];

// NABM register offsets relative to NABM base, for the PCM-Out (PO) channel.
const PO_BDBAR:  u16 = 0x10; // Buffer Descriptor List Base Address (32-bit)
const PO_CIV:   u16 = 0x14; // Current Index Value (8-bit, read-only)
const PO_LVI:   u16 = 0x15; // Last Valid Index (8-bit, write to advance)
const PO_SR:    u16 = 0x16; // Status Register (16-bit)
const PO_CR:    u16 = 0x1B; // Control Register (8-bit)

const NABM_GLOB_CNT: u16 = 0x2C; // Global Control Register (32-bit)
const NABM_GLOB_STS: u16 = 0x30; // Global Status Register (32-bit)

// PCM-Out Control Register bits
const CR_RPBM:  u8 = 0x01; // Run/Pause Bus Master
const CR_RR:    u8 = 0x02; // Reset Registers
const CR_LVBIE: u8 = 0x04; // Last Valid Buffer Interrupt Enable
const CR_FEIE:  u8 = 0x08; // FIFO Error Interrupt Enable
const CR_IOCE:  u8 = 0x10; // Interrupt On Completion Enable

// NAM register offsets
const NAM_MASTER_VOL: u16 = 0x02;
const NAM_PCM_VOL:    u16 = 0x18;
const NAM_PCM_RATE:   u16 = 0x2C; // PCM Front DAC Rate

// BDL ring size (hardware fixed at 32).
const BDL_SIZE: usize = 32;
// Frames (stereo pairs) per DMA buffer.  85 ms @ 48 kHz.
const BUF_FRAMES: usize = 4096;
// Bytes per buffer (2 channels × 2 bytes × BUF_FRAMES).
const BUF_BYTES: usize = BUF_FRAMES * 4;

// ─── Low-level I/O port accessors ─────────────────────────────────────────────

#[inline(always)]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("outb %al, %dx", in("dx") port, in("al") val, options(att_syntax, nostack));
}
#[inline(always)]
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("outw %ax, %dx", in("dx") port, in("ax") val, options(att_syntax, nostack));
}
#[inline(always)]
unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!("outl %eax, %dx", in("dx") port, in("eax") val, options(att_syntax, nostack));
}
#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    core::arch::asm!("inb %dx, %al", in("dx") port, out("al") v, options(att_syntax, nostack));
    v
}
#[inline(always)]
unsafe fn inw(port: u16) -> u16 {
    let v: u16;
    core::arch::asm!("inw %dx, %ax", in("dx") port, out("ax") v, options(att_syntax, nostack));
    v
}
#[inline(always)]
unsafe fn inl(port: u16) -> u32 {
    let v: u32;
    core::arch::asm!("inl %dx, %eax", in("dx") port, out("eax") v, options(att_syntax, nostack));
    v
}

// ─── PCI configuration space ──────────────────────────────────────────────────

unsafe fn pci_addr(bus: u8, dev: u8, func: u8, reg: u8) -> u32 {
    0x80000000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | (reg as u32 & 0xFC)
}

unsafe fn pci_read32(bus: u8, dev: u8, func: u8, reg: u8) -> u32 {
    outl(PCI_ADDR, pci_addr(bus, dev, func, reg));
    inl(PCI_DATA)
}

unsafe fn pci_write32(bus: u8, dev: u8, func: u8, reg: u8, val: u32) {
    outl(PCI_ADDR, pci_addr(bus, dev, func, reg));
    outl(PCI_DATA, val);
}

unsafe fn pci_read16(bus: u8, dev: u8, func: u8, reg: u8) -> u16 {
    let v = pci_read32(bus, dev, func, reg & 0xFC);
    ((v >> (8 * (reg & 2))) & 0xFFFF) as u16
}

unsafe fn pci_write16(bus: u8, dev: u8, func: u8, reg: u8, val: u16) {
    let aligned = reg & 0xFC;
    let shift = 8 * (reg & 2) as u32;
    let old = pci_read32(bus, dev, func, aligned);
    let new_val = (old & !(0xFFFF << shift)) | ((val as u32) << shift);
    pci_write32(bus, dev, func, aligned, new_val);
}

/// Scan all PCI buses/devices to find an AC97 audio controller.
/// Returns (bus, dev, func, vendor_id, device_id) or None.
unsafe fn find_ac97() -> Option<(u8, u8, u8)> {
    for bus in 0u8..=255 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = pci_read32(bus, dev, func, 0x00);
                if id == 0xFFFF_FFFF {
                    // No device; if func==0, no multi-function either.
                    if func == 0 { break; }
                    continue;
                }
                let vendor = (id & 0xFFFF) as u16;
                let device = ((id >> 16) & 0xFFFF) as u16;

                for &(kv, kd) in KNOWN_AC97 {
                    if kv == vendor && kd == device {
                        return Some((bus, dev, func));
                    }
                }

                // Also accept by PCI class 0x0401 (multimedia audio)
                let class_rev = pci_read32(bus, dev, func, 0x08);
                let class = (class_rev >> 16) as u16;
                if class == 0x0401 {
                    return Some((bus, dev, func));
                }

                if func == 0 {
                    let hdr = pci_read32(bus, dev, 0, 0x0C);
                    let header_type = (hdr >> 16) as u8;
                    if (header_type & 0x80) == 0 {
                        break; // Not multi-function device
                    }
                }
            }
        }
    }
    None
}

// ─── Buffer Descriptor List ───────────────────────────────────────────────────

#[repr(C, packed)]
struct BdlEntry {
    addr: u32,   // Physical address of the buffer
    samples: u16, // Number of 16-bit samples in the buffer
    flags: u16,  // Bit 15: interrupt on completion, bit 14: last buffer
}

// ─── AC97 driver ─────────────────────────────────────────────────────────────

pub struct Ac97Driver {
    available: bool,
    nam_base: u16,
    nabm_base: u16,

    // DMA buffers: BDL_SIZE × BUF_BYTES bytes of zeroed memory.
    // SAFETY: These must be page-aligned and in physical memory ≤4 GiB.
    dma_mem: Option<Box<DmaRegion>>,

    // Tracks which BDL entry we last set LVI to.
    lvi: u8,
    // Shadow of current DMA read position (CIV from hardware).
    civ: u8,
}

/// Contiguous DMA-safe memory for the BDL and all audio buffers.
///
/// Layout:
///   [0..BDL_SIZE*8]               BDL entries  (BDL_SIZE × 8 bytes)
///   [BDL_SIZE*8..]                Audio buffers (BDL_SIZE × BUF_BYTES bytes)
///
/// Total: 32×8 + 32×BUF_BYTES bytes = 256 + 131072 = 131328 bytes ≈ 33 pages.
#[repr(C, align(4096))]
struct DmaRegion {
    bdl:  [BdlEntry; BDL_SIZE],
    bufs: [[u8; BUF_BYTES]; BDL_SIZE],
}

impl Ac97Driver {
    pub fn new() -> Self {
        let mut drv = Self {
            available: false,
            nam_base: 0,
            nabm_base: 0,
            dma_mem: None,
            lvi: (BDL_SIZE as u8) - 1,
            civ: 0,
        };
        // SAFETY: All PCI and I/O port operations are unsafe by nature.
        unsafe { drv.try_init(); }
        drv
    }

    unsafe fn try_init(&mut self) {
        let Some((bus, dev, func)) = find_ac97() else { return };

        let id = pci_read32(bus, dev, func, 0x00);
        let vendor = (id & 0xFFFF) as u16;
        let device = ((id >> 16) & 0xFFFF) as u16;
        log::info!("anzu-hal AC97: found PCI {:04x}:{:04x} at {:02x}:{:02x}.{}", vendor, device, bus, dev, func);

        // Enable bus mastering and I/O space.
        let cmd = pci_read16(bus, dev, func, 0x04);
        pci_write16(bus, dev, func, 0x04, cmd | 0x05); // I/O enable + bus master

        // BAR0 = NAM (Native Audio Mixer) I/O base.
        let bar0 = pci_read32(bus, dev, func, 0x10);
        // BAR1 = NABM (Native Audio Bus Master) I/O base.
        let bar1 = pci_read32(bus, dev, func, 0x14);

        if (bar0 & 1) == 0 || (bar1 & 1) == 0 {
            log::warn!("anzu-hal AC97: BARs are not I/O space, cannot use");
            return;
        }

        self.nam_base  = (bar0 & 0xFFFC) as u16;
        self.nabm_base = (bar1 & 0xFFF0) as u16;
        log::info!("anzu-hal AC97: NAM=0x{:04x} NABM=0x{:04x}", self.nam_base, self.nabm_base);

        // Cold reset the codec via Global Control register.
        outl(self.nabm_base + NABM_GLOB_CNT, 0x0000_0002); // GIE=0, COLD_RST=1
        // Give codec 100 µs to settle (busy-poll since we have no sleep).
        for _ in 0..1_000_000u64 { core::hint::spin_loop(); }
        outl(self.nabm_base + NABM_GLOB_CNT, 0x0000_0000); // Clear reset

        // Wait for codec ready (bit 8 of Global Status = PCM out codec ready).
        let mut ready = false;
        for _ in 0..2_000_000u64 {
            let sts = inl(self.nabm_base + NABM_GLOB_STS);
            if sts & 0x0100 != 0 { ready = true; break; }
            core::hint::spin_loop();
        }
        if !ready {
            log::warn!("anzu-hal AC97: codec not ready after reset");
        }

        // Set master volume to 0 dB (unmuted). NAM register is attenuation:
        // 0x0000 = 0 dB (maximum), bit 15 = mute.
        outw(self.nam_base + NAM_MASTER_VOL, 0x0000);
        // PCM out volume max.
        outw(self.nam_base + NAM_PCM_VOL, 0x0000);

        // Set PCM out sample rate to 48000 Hz (if VRA is supported).
        // First check if variable rate audio (VRA) is supported.
        // Extended ID register is at NAM 0x28; bit 0 = VRA.
        let ext_id = inw(self.nam_base + 0x28);
        if ext_id & 0x01 != 0 {
            // Enable VRA (write bit 0 of Extended Audio Status 0x2A).
            let ext_sta = inw(self.nam_base + 0x2A);
            outw(self.nam_base + 0x2A, ext_sta | 0x01);
            outw(self.nam_base + NAM_PCM_RATE, 48000);
            let actual = inw(self.nam_base + NAM_PCM_RATE);
            log::info!("anzu-hal AC97: VRA enabled, actual rate={}", actual);
        } else {
            log::info!("anzu-hal AC97: VRA not supported, using fixed 48 kHz");
        }

        // Allocate DMA region.  Box<DmaRegion> is page-aligned (see #[repr(C, align(4096))]).
        let mut region = Box::new(DmaRegion {
            bdl: core::array::from_fn(|_| BdlEntry { addr: 0, samples: 0, flags: 0 }),
            bufs: [[0u8; BUF_BYTES]; BDL_SIZE],
        });

        // Physical address of the DMA region = virtual address under UEFI identity map.
        let region_phys = region.as_ref() as *const DmaRegion as u64;
        assert!(region_phys + core::mem::size_of::<DmaRegion>() as u64 <= 0xFFFF_FFFF,
            "anzu-hal AC97: DMA region above 4 GiB limit");

        let buf_base_phys = region_phys + core::mem::offset_of!(DmaRegion, bufs) as u64;

        // Populate BDL entries: point each entry at its buffer, set sample count.
        for i in 0..BDL_SIZE {
            let buf_phys = (buf_base_phys + (i * BUF_BYTES) as u64) as u32;
            // AC97 BDL sample count = number of 16-bit samples (L+R interleaved).
            // BUF_BYTES bytes / 2 = BUF_FRAMES * 2 samples.
            region.bdl[i].addr    = buf_phys;
            region.bdl[i].samples = (BUF_BYTES / 2) as u16;
            region.bdl[i].flags   = 0; // No interrupts; hardware cycles automatically.
        }

        // Give NABM the BDL physical address.
        let bdl_phys = region_phys as u32;
        outl(self.nabm_base + PO_BDBAR, bdl_phys);

        // Reset the PCM-out channel registers.
        outb(self.nabm_base + PO_CR, CR_RR);
        for _ in 0..1_000_000u64 { core::hint::spin_loop(); }

        self.dma_mem = Some(region);
        self.lvi = (BDL_SIZE as u8) - 1;
        self.civ = 0;

        // Set LVI to the last entry so the hardware has all 32 buffers to play.
        outb(self.nabm_base + PO_LVI, self.lvi);

        // Start PCM out DMA (RPBM bit = run).
        outb(self.nabm_base + PO_CR, CR_RPBM);

        self.available = true;
        log::info!("anzu-hal AC97: DMA started, {} buffers × {} frames", BDL_SIZE, BUF_FRAMES);
    }

    /// Read the current hardware index (CIV) and how many buffers are "consumed"
    /// since we last refilled up to LVI.
    ///
    /// The hardware cycles CIV from 0 to LVI, then wraps back to 0.
    /// We want to fill buffers that are *behind* CIV (already played).
    fn consumed_since(&self, lvi: u8) -> u8 {
        // CIV is the buffer currently being played.
        // Buffers behind CIV (in ring order) are free for refill.
        let civ = unsafe { inb(self.nabm_base + PO_CIV) };
        // Number of buffers between (lvi+1) and (civ-1) in the ring.
        // If civ == (lvi+1)%32 no buffers are free yet.
        let next = (lvi as usize + 1) % BDL_SIZE;
        if civ as usize == next {
            0
        } else {
            // Distance from next to civ in the ring.
            ((civ as usize + BDL_SIZE - next) % BDL_SIZE) as u8
        }
    }
}

impl AudioDriver for Ac97Driver {
    fn is_available(&self) -> bool {
        self.available
    }

    fn frames_available(&self) -> usize {
        if !self.available { return 0; }
        let consumed = unsafe { self.consumed_since(self.lvi) } as usize;
        consumed * BUF_FRAMES
    }

    fn write_frames(&mut self, buf: &[i16], n_frames: usize) {
        if !self.available || n_frames == 0 { return; }

        // Copy fields we need before taking &mut self.dma_mem (avoids borrow conflict).
        let nabm_base = self.nabm_base;
        let Some(region) = self.dma_mem.as_mut() else { return };

        let mut remaining = n_frames.min(buf.len() / 2);
        let mut src_offset = 0usize;

        while remaining > 0 {
            // Inline consumed-buffer count using the local copy of nabm_base and lvi.
            let civ = unsafe { inb(nabm_base + PO_CIV) };
            let next = (self.lvi as usize + 1) % BDL_SIZE;
            let consumed = if civ as usize == next {
                0usize
            } else {
                (civ as usize + BDL_SIZE - next) % BDL_SIZE
            };
            if consumed == 0 { break; }

            let refill_idx = next;
            let frames_to_fill = remaining.min(BUF_FRAMES);

            let dst = &mut region.bufs[refill_idx];
            for i in 0..frames_to_fill {
                let l = buf[src_offset + i * 2];
                let r = buf[src_offset + i * 2 + 1];
                let li = i * 4;
                let lb = l.to_le_bytes();
                let rb = r.to_le_bytes();
                dst[li]     = lb[0];
                dst[li + 1] = lb[1];
                dst[li + 2] = rb[0];
                dst[li + 3] = rb[1];
            }
            if frames_to_fill < BUF_FRAMES {
                dst[frames_to_fill * 4..].fill(0);
            }

            self.lvi = refill_idx as u8;
            unsafe { outb(nabm_base + PO_LVI, self.lvi); }

            src_offset += frames_to_fill * 2;
            remaining -= frames_to_fill;
        }
    }
}
