use crate::ir::{Item, OpKind};
use crate::meta::{Meta, Nls};
use anyhow::{anyhow, bail, Result};
use encoding_rs::{GB18030, SHIFT_JIS, UTF_8};
use std::collections::HashMap;

fn enc_text(meta: &Meta, s: &str) -> Result<Vec<u8>> {
    let (cow, _, had_errors) = match meta.nls {
        Nls::Utf8 => UTF_8.encode(s),
        Nls::ShiftJis => SHIFT_JIS.encode(s),
        Nls::Gb18030 => GB18030.encode(s),
    };
    if had_errors {
        bail!("text encoding error for string: {s}");
    }
    Ok(cow.into_owned())
}

fn enc_cstr(meta: &Meta, s: &str) -> Result<Vec<u8>> {
    let mut b = enc_text(meta, s)?;
    b.push(0);
    if b.len() > 255 {
        bail!(
            "string too long for u8 length (including NUL): len={} (max=255)",
            b.len()
        );
    }
    Ok(b)
}

fn opcode(k: &OpKind) -> u8 {
    match k {
        OpKind::Nop => 0x00,
        OpKind::InitStack { .. } => 0x01,
        OpKind::CallFn { .. } => 0x02,
        OpKind::Syscall { .. } => 0x03,
        OpKind::JmpAbs { .. } | OpKind::JmpLabel { .. } => 0x04,
        OpKind::JzAbs { .. } | OpKind::JzLabel { .. } => 0x05,
        OpKind::Ret => 0x06,
        OpKind::Retv => 0x07,
        OpKind::PushNil => 0x08,
        OpKind::PushTrue => 0x09,
        OpKind::PushI8(..) => 0x0A,
        OpKind::PushI16(..) => 0x0B,
        OpKind::PushI32(..) => 0x0C,
        OpKind::PushF32(..) => 0x0D,
        OpKind::PushString(..) => 0x0E,
        OpKind::PushGlobal(..) => 0x0F,
        OpKind::PushStack(..) => 0x10,
        OpKind::PushTop => 0x11,
        OpKind::PushGlobalTable(..) => 0x12,
        OpKind::PushLocalTable(..) => 0x13,
        OpKind::PushReturn => 0x14,
        OpKind::PopGlobal(..) => 0x15,
        OpKind::PopStack(..) => 0x16,
        OpKind::PopGlobalTable(..) => 0x17,
        OpKind::PopLocalTable(..) => 0x18,
        OpKind::Neg => 0x19,
        OpKind::Add => 0x1A,
        OpKind::Sub => 0x1B,
        OpKind::Mul => 0x1C,
        OpKind::Div => 0x1D,
        OpKind::Mod => 0x1E,
        OpKind::BitTest => 0x1F,
        OpKind::And => 0x20,
        OpKind::Or => 0x21,
        OpKind::SetE => 0x22,
        OpKind::SetNe => 0x23,
        OpKind::SetG => 0x24,
        OpKind::SetLe => 0x25,
        OpKind::SetL => 0x26,
        OpKind::SetGe => 0x27,
    }
}

fn op_size(meta: &Meta, k: &OpKind) -> Result<usize> {
    let sz = match k {
        OpKind::Nop
        | OpKind::Ret
        | OpKind::Retv
        | OpKind::PushNil
        | OpKind::PushTrue
        | OpKind::PushTop
        | OpKind::PushReturn
        | OpKind::Neg
        | OpKind::Add
        | OpKind::Sub
        | OpKind::Mul
        | OpKind::Div
        | OpKind::Mod
        | OpKind::BitTest
        | OpKind::And
        | OpKind::Or
        | OpKind::SetE
        | OpKind::SetNe
        | OpKind::SetG
        | OpKind::SetLe
        | OpKind::SetL
        | OpKind::SetGe => 1,

        OpKind::InitStack { .. } => 3,
        OpKind::CallFn { .. } => 5,
        OpKind::Syscall { .. } => 3,
        OpKind::JmpAbs { .. }
        | OpKind::JzAbs { .. }
        | OpKind::JmpLabel { .. }
        | OpKind::JzLabel { .. } => 5,

        OpKind::PushI8(..) => 2,
        OpKind::PushI16(..) => 3,
        OpKind::PushI32(..) => 5,
        OpKind::PushF32(..) => 5,
        OpKind::PushString(s) => {
            let b = enc_cstr(meta, s)?;
            1 + 1 + b.len()
        }

        OpKind::PushGlobal(..)
        | OpKind::PushGlobalTable(..)
        | OpKind::PopGlobal(..)
        | OpKind::PopGlobalTable(..) => 3,

        OpKind::PushStack(..)
        | OpKind::PopStack(..)
        | OpKind::PushLocalTable(..)
        | OpKind::PopLocalTable(..) => 2,
    };
    Ok(sz)
}

pub fn assemble(meta: &Meta, items: &[Item]) -> Result<(Vec<u8>, HashMap<String, u32>)> {
    let base_addr: u32 = 4;

    // First pass: compute label addresses.
    let mut labels: HashMap<String, u32> = HashMap::new();
    let mut addr: u32 = base_addr;
    for it in items {
        match it {
            Item::Label(l) => {
                labels.insert(l.name.clone(), addr);
            }
            Item::Op(op) => {
                let sz = op_size(meta, op)? as u32;
                addr = addr
                    .checked_add(sz)
                    .ok_or_else(|| anyhow!("address overflow"))?;
            }
        }
    }

    // Second pass: encode.
    let mut out: Vec<u8> = Vec::new();
    for it in items {
        let op = match it {
            Item::Label(_) => continue,
            Item::Op(op) => op,
        };

        out.push(opcode(op));

        match op {
            OpKind::Nop
            | OpKind::Ret
            | OpKind::Retv
            | OpKind::PushNil
            | OpKind::PushTrue
            | OpKind::PushTop
            | OpKind::PushReturn
            | OpKind::Neg
            | OpKind::Add
            | OpKind::Sub
            | OpKind::Mul
            | OpKind::Div
            | OpKind::Mod
            | OpKind::BitTest
            | OpKind::And
            | OpKind::Or
            | OpKind::SetE
            | OpKind::SetNe
            | OpKind::SetG
            | OpKind::SetLe
            | OpKind::SetL
            | OpKind::SetGe => {}

            OpKind::InitStack { args, locals } => {
                out.push(*args as u8);
                out.push(*locals as u8);
            }

            OpKind::CallFn { name } => {
                let lbl = format!("fn:{name}");
                let tgt = labels
                    .get(&lbl)
                    .copied()
                    .ok_or_else(|| anyhow!("unknown function label: {lbl}"))?;
                out.extend_from_slice(&tgt.to_le_bytes());
            }

            OpKind::Syscall { id } => {
                out.extend_from_slice(&id.to_le_bytes());
            }

            OpKind::JmpAbs { target } => {
                out.extend_from_slice(&target.to_le_bytes());
            }
            OpKind::JzAbs { target } => {
                out.extend_from_slice(&target.to_le_bytes());
            }

            OpKind::JmpLabel { label } => {
                let tgt = labels
                    .get(label)
                    .copied()
                    .ok_or_else(|| anyhow!("unknown label: {label}"))?;
                out.extend_from_slice(&tgt.to_le_bytes());
            }
            OpKind::JzLabel { label } => {
                let tgt = labels
                    .get(label)
                    .copied()
                    .ok_or_else(|| anyhow!("unknown label: {label}"))?;
                out.extend_from_slice(&tgt.to_le_bytes());
            }

            OpKind::PushI8(v) => out.push(*v as u8),
            OpKind::PushI16(v) => out.extend_from_slice(&v.to_le_bytes()),
            OpKind::PushI32(v) => out.extend_from_slice(&v.to_le_bytes()),
            OpKind::PushF32(v) => out.extend_from_slice(&v.to_le_bytes()),
            OpKind::PushString(s) => {
                let b = enc_cstr(meta, s)?;
                out.push(b.len() as u8);
                out.extend_from_slice(&b);
            }

            OpKind::PushGlobal(idx)
            | OpKind::PushGlobalTable(idx)
            | OpKind::PopGlobal(idx)
            | OpKind::PopGlobalTable(idx) => {
                out.extend_from_slice(&idx.to_le_bytes());
            }

            OpKind::PushStack(idx) | OpKind::PopStack(idx) | OpKind::PushLocalTable(idx) | OpKind::PopLocalTable(idx) => {
                out.push(*idx as u8);
            }
        }
    }

    Ok((out, labels))
}

pub fn build_sysdesc(meta: &Meta, entry_point: u32) -> Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();

    buf.extend_from_slice(&entry_point.to_le_bytes());
    buf.extend_from_slice(&meta.non_volatile_global_count.to_le_bytes());
    buf.extend_from_slice(&meta.volatile_global_count.to_le_bytes());
    buf.extend_from_slice(&meta.game_mode.to_le_bytes());

    let title_b = enc_cstr(meta, &meta.game_title)?;
    buf.push(title_b.len() as u8);
    buf.extend_from_slice(&title_b);

    let sc_count = meta.syscall_count();
    buf.extend_from_slice(&sc_count.to_le_bytes());

    for sc in &meta.syscalls {
        let name_b = enc_cstr(meta, &sc.name)?;
        buf.push(sc.args);
        buf.push(name_b.len() as u8);
        buf.extend_from_slice(&name_b);
    }

    buf.extend_from_slice(&meta.custom_syscall_count.to_le_bytes());

    Ok(buf)
}
