use std::collections::{BTreeMap, HashMap};

use crate::inst::*;
use anyhow::Result;
use rfvp::script::{opcode::Opcode, parser::Nls};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Function {
    address: u32,
    args_count: u8,
    locals_count: u8,
    insts: Vec<Inst2>,
}

impl Function {
    pub fn get_insts(&self) -> &Vec<Inst2> {
        &self.insts
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Inst2 {
    address: u32,
    mnemonic: String,
    operands: Vec<String>,
}

impl Inst2 {
    pub fn get_address(&self) -> u32 {
        self.address
    }

    pub fn get_opcode(&self) -> Result<Opcode> {
        match Opcode::try_from(self.mnemonic.as_str()) {
            Ok(opcode) => Ok(opcode),
            Err(_) => Err(anyhow::anyhow!("invalid opcode")),
        }
    }
}

pub fn to_nop(inst: &Inst2) -> Result<NopInst> {
    Ok(NopInst::new())
}

pub fn to_init_stack(inst: &Inst2) -> Result<InitStackInst> {
    Ok(InitStackInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
        inst.operands
            .get(1)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_call(inst: &Inst2) -> Result<CallInst> {
    Ok(CallInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_syscall(inst: &Inst2, syscalls: &BTreeMap<String, u32>) -> Result<SyscallInst> {
    let syscall_name = inst
        .operands
        .get(0)
        .ok_or(anyhow::anyhow!("missing operand"))?;
    let id = syscalls
        .get(syscall_name)
        .ok_or(anyhow::anyhow!("invalid syscall"))?
        .to_owned();
    Ok(SyscallInst::new(id as u16))
}

pub fn to_ret(inst: &Inst2) -> Result<RetInst> {
    Ok(RetInst::new())
}

pub fn to_ret_v(inst: &Inst2) -> Result<RetVInst> {
    Ok(RetVInst::new())
}

pub fn to_jmp(inst: &Inst2) -> Result<JmpInst> {
    Ok(JmpInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_jz(inst: &Inst2) -> Result<JzInst> {
    Ok(JzInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_nil(inst: &Inst2) -> Result<PushNilInst> {
    Ok(PushNilInst::new())
}

pub fn to_push_true(inst: &Inst2) -> Result<PushTrueInst> {
    Ok(PushTrueInst::new())
}

pub fn to_push_i32(inst: &Inst2) -> Result<PushI32Inst> {
    Ok(PushI32Inst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_i16(inst: &Inst2) -> Result<PushI16Inst> {
    Ok(PushI16Inst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_i8(inst: &Inst2) -> Result<PushI8Inst> {
    Ok(PushI8Inst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_f32(inst: &Inst2) -> Result<PushF32Inst> {
    Ok(PushF32Inst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_string(inst: &Inst2, nls: Nls) -> Result<PushStringInst> {
    Ok(PushStringInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .to_owned(),
        nls,
    ))
}

pub fn to_push_global(inst: &Inst2) -> Result<PushGlobalInst> {
    Ok(PushGlobalInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_stack(inst: &Inst2) -> Result<PushStackInst> {
    Ok(PushStackInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_global_table(inst: &Inst2) -> Result<PushGlobalTableInst> {
    Ok(PushGlobalTableInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_local_table(inst: &Inst2) -> Result<PushLocalTableInst> {
    Ok(PushLocalTableInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_push_top(inst: &Inst2) -> Result<PushTopInst> {
    Ok(PushTopInst::new())
}

pub fn to_push_return(inst: &Inst2) -> Result<PushReturnInst> {
    Ok(PushReturnInst::new())
}

pub fn to_pop_global(inst: &Inst2) -> Result<PopGlobalInst> {
    Ok(PopGlobalInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_pop_stack(inst: &Inst2) -> Result<PopStackInst> {
    Ok(PopStackInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_pop_global_table(inst: &Inst2) -> Result<PopGlobalTableInst> {
    Ok(PopGlobalTableInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_pop_local_table(inst: &Inst2) -> Result<PopLocalTableInst> {
    Ok(PopLocalTableInst::new(
        inst.operands
            .get(0)
            .ok_or(anyhow::anyhow!("missing operand"))?
            .parse()?,
    ))
}

pub fn to_neg(inst: &Inst2) -> Result<NegInst> {
    Ok(NegInst::new())
}

pub fn to_add(inst: &Inst2) -> Result<AddInst> {
    Ok(AddInst::new())
}

pub fn to_sub(inst: &Inst2) -> Result<SubInst> {
    Ok(SubInst::new())
}

pub fn to_mul(inst: &Inst2) -> Result<MulInst> {
    Ok(MulInst::new())
}

pub fn to_div(inst: &Inst2) -> Result<DivInst> {
    Ok(DivInst::new())
}

pub fn to_mod(inst: &Inst2) -> Result<ModInst> {
    Ok(ModInst::new())
}

pub fn to_bit_test(inst: &Inst2) -> Result<BitTestInst> {
    Ok(BitTestInst::new())
}

pub fn to_and(inst: &Inst2) -> Result<AndInst> {
    Ok(AndInst::new())
}

pub fn to_or(inst: &Inst2) -> Result<OrInst> {
    Ok(OrInst::new())
}

pub fn to_set_e(inst: &Inst2) -> Result<SetEInst> {
    Ok(SetEInst::new())
}

pub fn to_set_ne(inst: &Inst2) -> Result<SetNEInst> {
    Ok(SetNEInst::new())
}

pub fn to_set_g(inst: &Inst2) -> Result<SetGInst> {
    Ok(SetGInst::new())
}

pub fn to_set_le(inst: &Inst2) -> Result<SetLEInst> {
    Ok(SetLEInst::new())
}

pub fn to_set_l(inst: &Inst2) -> Result<SetLInst> {
    Ok(SetLInst::new())
}

pub fn to_set_ge(inst: &Inst2) -> Result<SetGEInst> {
    Ok(SetGEInst::new())
}

