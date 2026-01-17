use anyhow::{bail, Result};
use clap::Parser;
use inst::Inst;
use rfvp::script::{opcode::Opcode, parser::Nls};
use serde::{de::value, Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
};

use inst::*;
use utils::*;

mod inst;
mod utils;

#[derive(Debug, Serialize, Deserialize)]
pub struct FVPProject {
    config_file: PathBuf,
    disassembly_file: PathBuf,
}

impl FVPProject {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let config_file = PathBuf::from(path.as_ref());
        let config_str = std::fs::read_to_string(config_file)?;
        let config: FVPProject = toml::from_str(&config_str)?;
        Ok(config)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyscallEntry {
    id: u32,
    name: String,
    args_count: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    entry_point: u32,
    non_volatile_global_count: u16,
    volatile_global_count: u16,
    game_mode: u16,
    game_title: String,
    syscalls: Vec<SyscallEntry>,
    custom_syscall_count: u16,
}

impl ProjectConfig {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let config_file = PathBuf::from(path.as_ref());
        let config_str = std::fs::read_to_string(config_file)?;
        let config: ProjectConfig = serde_yaml::from_str(&config_str)?;
        Ok(config)
    }

    pub fn put_u8(value: u8, buffer: &mut Vec<u8>) {
        buffer.push(value);
    }

    pub fn put_u16_le(value: u16, buffer: &mut Vec<u8>) {
        buffer.push((value & 0xff) as u8);
        buffer.push(((value >> 8) & 0xff) as u8);
    }

    pub fn put_u32_le(value: u32, buffer: &mut Vec<u8>) {
        buffer.push((value & 0xff) as u8);
        buffer.push(((value >> 8) & 0xff) as u8);
        buffer.push(((value >> 16) & 0xff) as u8);
        buffer.push(((value >> 24) & 0xff) as u8);
    }

    fn string_to_blob(content: &str, nls: Nls) -> Vec<u8> {
        // convert utf-8 string to local string via Nls
        let mut content_bytes = match nls {
            Nls::GBK => encoding_rs::GBK.encode(content).0.to_vec(),
            Nls::ShiftJIS => encoding_rs::SHIFT_JIS.encode(content).0.to_vec(),
            Nls::UTF8 => content.as_bytes().to_vec(),
        };

        if !content_bytes.ends_with(&[0]) {
            content_bytes.push(0);
        }

        content_bytes
    }

    fn serialize_to_binary(&mut self, nls: Nls) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        Self::put_u32_le(self.entry_point, &mut data);
        Self::put_u16_le(self.non_volatile_global_count, &mut data);
        Self::put_u16_le(self.volatile_global_count, &mut data);
        Self::put_u16_le(self.game_mode, &mut data);

        let game_title = Self::string_to_blob(&self.game_title, nls.clone());
        let game_title_len = game_title.len() as u8;
        Self::put_u8(game_title_len, &mut data);
        data.extend_from_slice(&game_title);

        Self::put_u16_le(self.syscalls.len() as u16, &mut data);
        self.syscalls.sort_by_key(|x| x.id);
        for syscall in &self.syscalls {
            Self::put_u8(syscall.args_count, &mut data);
            let syscall_name = Self::string_to_blob(&syscall.name, nls.clone());
            let syscall_name_len = syscall_name.len() as u8;
            Self::put_u8(syscall_name_len, &mut data);
            data.extend_from_slice(&syscall_name);
        }

        if self.custom_syscall_count > 0 {
            bail!("custom syscall not supported");
        }

        Self::put_u16_le(self.custom_syscall_count, &mut data);

        Ok(data)
    }

    pub fn link(&mut self, entry_point: u32, nls: Nls) -> Result<Vec<u8>> {
        self.entry_point = entry_point;
        self.serialize_to_binary(nls)
    }
}

pub struct Assembler {
    project: FVPProject,
    config: ProjectConfig,
    functions: Vec<Function>,
    nls: Nls,

    code_section: Vec<u8>,
}

pub enum InstSet {
    Nop(NopInst),
    InitStack(InitStackInst),
    Call(CallInst),
    Syscall(SyscallInst),
    Ret(RetInst),
    RetV(RetVInst),
    Jmp(JmpInst),
    Jz(JzInst),
    PushNil(PushNilInst),
    PushTrue(PushTrueInst),
    PushI32(PushI32Inst),
    PushI16(PushI16Inst),
    PushI8(PushI8Inst),
    PushF32(PushF32Inst),
    PushString(PushStringInst),
    PushGlobal(PushGlobalInst),
    PushStack(PushStackInst),
    PushGlobalTable(PushGlobalTableInst),
    PushLocalTable(PushLocalTableInst),
    PushTop(PushTopInst),
    PushReturn(PushReturnInst),
    PopGlobal(PopGlobalInst),
    PopStack(PopStackInst),
    PopGlobalTable(PopGlobalTableInst),
    PopLocalTable(PopLocalTableInst),
    Neg(NegInst),
    Add(AddInst),
    Sub(SubInst),
    Mul(MulInst),
    Div(DivInst),
    Mod(ModInst),
    BitTest(BitTestInst),
    And(AndInst),
    Or(OrInst),
    SetE(SetEInst),
    SetNE(SetNEInst),
    SetG(SetGInst),
    SetLE(SetLEInst),
    SetL(SetLInst),
    SetGE(SetGEInst),
}

impl InstSet {
    pub fn set_address(&mut self, address: u32) {
        match self {
            InstSet::Nop(inst) => inst.set_address(address),
            InstSet::InitStack(inst) => inst.set_address(address),
            InstSet::Call(inst) => inst.set_address(address),
            InstSet::Syscall(inst) => inst.set_address(address),
            InstSet::Ret(inst) => inst.set_address(address),
            InstSet::RetV(inst) => inst.set_address(address),
            InstSet::Jmp(inst) => inst.set_address(address),
            InstSet::Jz(inst) => inst.set_address(address),
            InstSet::PushNil(inst) => inst.set_address(address),
            InstSet::PushTrue(inst) => inst.set_address(address),
            InstSet::PushI32(inst) => inst.set_address(address),
            InstSet::PushI16(inst) => inst.set_address(address),
            InstSet::PushI8(inst) => inst.set_address(address),
            InstSet::PushF32(inst) => inst.set_address(address),
            InstSet::PushString(inst) => inst.set_address(address),
            InstSet::PushGlobal(inst) => inst.set_address(address),
            InstSet::PushStack(inst) => inst.set_address(address),
            InstSet::PushGlobalTable(inst) => inst.set_address(address),
            InstSet::PushLocalTable(inst) => inst.set_address(address),
            InstSet::PushTop(inst) => inst.set_address(address),
            InstSet::PushReturn(inst) => inst.set_address(address),
            InstSet::PopGlobal(inst) => inst.set_address(address),
            InstSet::PopStack(inst) => inst.set_address(address),
            InstSet::PopGlobalTable(inst) => inst.set_address(address),
            InstSet::PopLocalTable(inst) => inst.set_address(address),
            InstSet::Neg(inst) => inst.set_address(address),
            InstSet::Add(inst) => inst.set_address(address),
            InstSet::Sub(inst) => inst.set_address(address),
            InstSet::Mul(inst) => inst.set_address(address),
            InstSet::Div(inst) => inst.set_address(address),
            InstSet::Mod(inst) => inst.set_address(address),
            InstSet::BitTest(inst) => inst.set_address(address),
            InstSet::And(inst) => inst.set_address(address),
            InstSet::Or(inst) => inst.set_address(address),
            InstSet::SetE(inst) => inst.set_address(address),
            InstSet::SetNE(inst) => inst.set_address(address),
            InstSet::SetG(inst) => inst.set_address(address),
            InstSet::SetLE(inst) => inst.set_address(address),
            InstSet::SetL(inst) => inst.set_address(address),
            InstSet::SetGE(inst) => inst.set_address(address),
        }
    }

    pub fn get_address(&self) -> u32 {
        match self {
            InstSet::Nop(inst) => inst.address(),
            InstSet::InitStack(inst) => inst.address(),
            InstSet::Call(inst) => inst.address(),
            InstSet::Syscall(inst) => inst.address(),
            InstSet::Ret(inst) => inst.address(),
            InstSet::RetV(inst) => inst.address(),
            InstSet::Jmp(inst) => inst.address(),
            InstSet::Jz(inst) => inst.address(),
            InstSet::PushNil(inst) => inst.address(),
            InstSet::PushTrue(inst) => inst.address(),
            InstSet::PushI32(inst) => inst.address(),
            InstSet::PushI16(inst) => inst.address(),
            InstSet::PushI8(inst) => inst.address(),
            InstSet::PushF32(inst) => inst.address(),
            InstSet::PushString(inst) => inst.address(),
            InstSet::PushGlobal(inst) => inst.address(),
            InstSet::PushStack(inst) => inst.address(),
            InstSet::PushGlobalTable(inst) => inst.address(),
            InstSet::PushLocalTable(inst) => inst.address(),
            InstSet::PushTop(inst) => inst.address(),
            InstSet::PushReturn(inst) => inst.address(),
            InstSet::PopGlobal(inst) => inst.address(),
            InstSet::PopStack(inst) => inst.address(),
            InstSet::PopGlobalTable(inst) => inst.address(),
            InstSet::PopLocalTable(inst) => inst.address(),
            InstSet::Neg(inst) => inst.address(),
            InstSet::Add(inst) => inst.address(),
            InstSet::Sub(inst) => inst.address(),
            InstSet::Mul(inst) => inst.address(),
            InstSet::Div(inst) => inst.address(),
            InstSet::Mod(inst) => inst.address(),
            InstSet::BitTest(inst) => inst.address(),
            InstSet::And(inst) => inst.address(),
            InstSet::Or(inst) => inst.address(),
            InstSet::SetE(inst) => inst.address(),
            InstSet::SetNE(inst) => inst.address(),
            InstSet::SetG(inst) => inst.address(),
            InstSet::SetLE(inst) => inst.address(),
            InstSet::SetL(inst) => inst.address(),
            InstSet::SetGE(inst) => inst.address(),
        }
    }

    pub fn size(&self) -> u32 {
        match self {
            InstSet::Nop(inst) => inst.size(),
            InstSet::InitStack(inst) => inst.size(),
            InstSet::Call(inst) => inst.size(),
            InstSet::Syscall(inst) => inst.size(),
            InstSet::Ret(inst) => inst.size(),
            InstSet::RetV(inst) => inst.size(),
            InstSet::Jmp(inst) => inst.size(),
            InstSet::Jz(inst) => inst.size(),
            InstSet::PushNil(inst) => inst.size(),
            InstSet::PushTrue(inst) => inst.size(),
            InstSet::PushI32(inst) => inst.size(),
            InstSet::PushI16(inst) => inst.size(),
            InstSet::PushI8(inst) => inst.size(),
            InstSet::PushF32(inst) => inst.size(),
            InstSet::PushString(inst) => inst.size(),
            InstSet::PushGlobal(inst) => inst.size(),
            InstSet::PushStack(inst) => inst.size(),
            InstSet::PushGlobalTable(inst) => inst.size(),
            InstSet::PushLocalTable(inst) => inst.size(),
            InstSet::PushTop(inst) => inst.size(),
            InstSet::PushReturn(inst) => inst.size(),
            InstSet::PopGlobal(inst) => inst.size(),
            InstSet::PopStack(inst) => inst.size(),
            InstSet::PopGlobalTable(inst) => inst.size(),
            InstSet::PopLocalTable(inst) => inst.size(),
            InstSet::Neg(inst) => inst.size(),
            InstSet::Add(inst) => inst.size(),
            InstSet::Sub(inst) => inst.size(),
            InstSet::Mul(inst) => inst.size(),
            InstSet::Div(inst) => inst.size(),
            InstSet::Mod(inst) => inst.size(),
            InstSet::BitTest(inst) => inst.size(),
            InstSet::And(inst) => inst.size(),
            InstSet::Or(inst) => inst.size(),
            InstSet::SetE(inst) => inst.size(),
            InstSet::SetNE(inst) => inst.size(),
            InstSet::SetG(inst) => inst.size(),
            InstSet::SetLE(inst) => inst.size(),
            InstSet::SetL(inst) => inst.size(),
            InstSet::SetGE(inst) => inst.size(),
        }
    }

    pub fn serialize_to_binary(&self) -> Vec<u8> {
        match self {
            InstSet::Nop(inst) => inst.serialize_to_binary(),
            InstSet::InitStack(inst) => inst.serialize_to_binary(),
            InstSet::Call(inst) => inst.serialize_to_binary(),
            InstSet::Syscall(inst) => inst.serialize_to_binary(),
            InstSet::Ret(inst) => inst.serialize_to_binary(),
            InstSet::RetV(inst) => inst.serialize_to_binary(),
            InstSet::Jmp(inst) => inst.serialize_to_binary(),
            InstSet::Jz(inst) => inst.serialize_to_binary(),
            InstSet::PushNil(inst) => inst.serialize_to_binary(),
            InstSet::PushTrue(inst) => inst.serialize_to_binary(),
            InstSet::PushI32(inst) => inst.serialize_to_binary(),
            InstSet::PushI16(inst) => inst.serialize_to_binary(),
            InstSet::PushI8(inst) => inst.serialize_to_binary(),
            InstSet::PushF32(inst) => inst.serialize_to_binary(),
            InstSet::PushString(inst) => inst.serialize_to_binary(),
            InstSet::PushGlobal(inst) => inst.serialize_to_binary(),
            InstSet::PushStack(inst) => inst.serialize_to_binary(),
            InstSet::PushGlobalTable(inst) => inst.serialize_to_binary(),
            InstSet::PushLocalTable(inst) => inst.serialize_to_binary(),
            InstSet::PushTop(inst) => inst.serialize_to_binary(),
            InstSet::PushReturn(inst) => inst.serialize_to_binary(),
            InstSet::PopGlobal(inst) => inst.serialize_to_binary(),
            InstSet::PopStack(inst) => inst.serialize_to_binary(),
            InstSet::PopGlobalTable(inst) => inst.serialize_to_binary(),
            InstSet::PopLocalTable(inst) => inst.serialize_to_binary(),
            InstSet::Neg(inst) => inst.serialize_to_binary(),
            InstSet::Add(inst) => inst.serialize_to_binary(),
            InstSet::Sub(inst) => inst.serialize_to_binary(),
            InstSet::Mul(inst) => inst.serialize_to_binary(),
            InstSet::Div(inst) => inst.serialize_to_binary(),
            InstSet::Mod(inst) => inst.serialize_to_binary(),
            InstSet::BitTest(inst) => inst.serialize_to_binary(),
            InstSet::And(inst) => inst.serialize_to_binary(),
            InstSet::Or(inst) => inst.serialize_to_binary(),
            InstSet::SetE(inst) => inst.serialize_to_binary(),
            InstSet::SetNE(inst) => inst.serialize_to_binary(),
            InstSet::SetG(inst) => inst.serialize_to_binary(),
            InstSet::SetLE(inst) => inst.serialize_to_binary(),
            InstSet::SetL(inst) => inst.serialize_to_binary(),
            InstSet::SetGE(inst) => inst.serialize_to_binary(),
        }
    }
}

impl Assembler {
    pub fn new(project_dir: impl AsRef<Path>, nls: Nls) -> Result<Self> {
        let proj_path = project_dir.as_ref().join("project.toml");

        let project = FVPProject::new(proj_path)?;
        let disassembly_path = project_dir.as_ref().join(&project.disassembly_file);
        let config_path = project_dir.as_ref().join(&project.config_file);
        let config = ProjectConfig::new(config_path)?;
        let functions = std::fs::read_to_string(disassembly_path)?;
        let functions: Vec<Function> = serde_yaml::from_str(&functions)?;

        Ok(Self {
            project,
            config,
            functions,
            nls,

            code_section: Vec::new(),
        })
    }

    fn inst2_to_inst(
        inst: &Inst2,
        nls: &Nls,
        syscall_table: &BTreeMap<String, u32>,
    ) -> Result<InstSet> {
        let opcode = inst.get_opcode()?;
        let wrapped_inst = match opcode {
            Opcode::Nop => InstSet::Nop(to_nop(inst)?),
            Opcode::InitStack => InstSet::InitStack(to_init_stack(inst)?),
            Opcode::Call => InstSet::Call(to_call(inst)?),
            Opcode::Syscall => InstSet::Syscall(to_syscall(inst, syscall_table)?),
            Opcode::Ret => InstSet::Ret(to_ret(inst)?),
            Opcode::RetV => InstSet::RetV(to_ret_v(inst)?),
            Opcode::Jmp => InstSet::Jmp(to_jmp(inst)?),
            Opcode::Jz => InstSet::Jz(to_jz(inst)?),
            Opcode::PushNil => InstSet::PushNil(to_push_nil(inst)?),
            Opcode::PushTrue => InstSet::PushTrue(to_push_true(inst)?),
            Opcode::PushI32 => InstSet::PushI32(to_push_i32(inst)?),
            Opcode::PushI16 => InstSet::PushI16(to_push_i16(inst)?),
            Opcode::PushI8 => InstSet::PushI8(to_push_i8(inst)?),
            Opcode::PushF32 => InstSet::PushF32(to_push_f32(inst)?),
            Opcode::PushString => InstSet::PushString(to_push_string(inst, nls.clone())?),
            Opcode::PushGlobal => InstSet::PushGlobal(to_push_global(inst)?),
            Opcode::PushStack => InstSet::PushStack(to_push_stack(inst)?),
            Opcode::PushGlobalTable => InstSet::PushGlobalTable(to_push_global_table(inst)?),
            Opcode::PushLocalTable => InstSet::PushLocalTable(to_push_local_table(inst)?),
            Opcode::PushTop => InstSet::PushTop(to_push_top(inst)?),
            Opcode::PushReturn => InstSet::PushReturn(to_push_return(inst)?),
            Opcode::PopGlobal => InstSet::PopGlobal(to_pop_global(inst)?),
            Opcode::PopStack => InstSet::PopStack(to_pop_stack(inst)?),
            Opcode::PopGlobalTable => InstSet::PopGlobalTable(to_pop_global_table(inst)?),
            Opcode::PopLocalTable => InstSet::PopLocalTable(to_pop_local_table(inst)?),
            Opcode::Neg => InstSet::Neg(to_neg(inst)?),
            Opcode::Add => InstSet::Add(to_add(inst)?),
            Opcode::Sub => InstSet::Sub(to_sub(inst)?),
            Opcode::Mul => InstSet::Mul(to_mul(inst)?),
            Opcode::Div => InstSet::Div(to_div(inst)?),
            Opcode::Mod => InstSet::Mod(to_mod(inst)?),
            Opcode::BitTest => InstSet::BitTest(to_bit_test(inst)?),
            Opcode::And => InstSet::And(to_and(inst)?),
            Opcode::Or => InstSet::Or(to_or(inst)?),
            Opcode::SetE => InstSet::SetE(to_set_e(inst)?),
            Opcode::SetNE => InstSet::SetNE(to_set_ne(inst)?),
            Opcode::SetG => InstSet::SetG(to_set_g(inst)?),
            Opcode::SetLE => InstSet::SetLE(to_set_le(inst)?),
            Opcode::SetL => InstSet::SetL(to_set_l(inst)?),
            Opcode::SetGE => InstSet::SetGE(to_set_ge(inst)?),
        };

        Ok(wrapped_inst)
    }

    fn compile(&mut self, old_entry_point: u32) -> Result<u32> {
        let mut map = BTreeMap::new();
        for func in &self.functions {
            for inst in func.get_insts() {
                let addr = inst.get_address();
                map.insert(addr, inst);
            }
        }

        // phase 1: set address
        let mut syscall_table = BTreeMap::new();
        for entry in self.config.syscalls.iter() {
            syscall_table.insert(entry.name.clone(), entry.id);
        }
        let mut insts = BTreeMap::new();
        let mut cursor = 4u32;
        for (addr, inst) in map {
            let mut wrapped_inst = Self::inst2_to_inst(inst, &self.nls, &syscall_table)?;
            wrapped_inst.set_address(cursor);
            let size = wrapped_inst.size();
            let wrapped_inst = Rc::new(RefCell::new(wrapped_inst));
            insts.insert(addr, wrapped_inst);
            cursor += size;
        }
        let entry_point = insts
            .get(&old_entry_point)
            .ok_or_else(|| anyhow::anyhow!("entry point not found"))?
            .borrow()
            .get_address();

        // phase 2: set jump target
        for (_, inst) in &insts {
            let inst = &mut *inst.borrow_mut();
            match inst {
                InstSet::Jmp(inst) => {
                    let old_target = inst.get_old_target();
                    let target_inst = insts
                        .get(&old_target)
                        .ok_or_else(|| anyhow::anyhow!(format!("target not found: {}", old_target)))?;
                    inst.set_target(target_inst.borrow().get_address());
                }
                InstSet::Jz(inst) => {
                    let old_target = inst.get_old_target();
                    let target_inst = insts
                        .get(&old_target)
                        .ok_or_else(|| anyhow::anyhow!(format!("target not found: {}", old_target)))?;
                    inst.set_target(target_inst.borrow().get_address());
                }
                InstSet::Call(inst) => {
                    let old_target = inst.get_old_func_target();
                    let target_inst = insts
                        .get(&old_target)
                        .ok_or_else(|| anyhow::anyhow!(format!("target not found: {}", old_target)))?;
                    inst.set_func_target(target_inst.borrow().get_address());
                }
                _ => {}
            }
        }

        // phase 3: serialize
        self.code_section.clear();
        for (_, inst) in insts {
            let blob = inst.borrow().serialize_to_binary();
            self.code_section.extend_from_slice(&blob);
        }

        Ok(entry_point)
    }

    fn size(&self) -> u32 {
        self.code_section.len() as u32
    }

    fn link(&mut self, new_entry_point: u32) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let header_offset = 4 + self.size();

        ProjectConfig::put_u32_le(header_offset, &mut data);
        data.extend_from_slice(&self.code_section);

        let header = self.config.link(new_entry_point, self.nls.clone())?;
        data.extend_from_slice(&header);

        Ok(data)
    }
}

fn compile(project_dir: impl AsRef<Path>, output: impl AsRef<Path>, nls: Nls) -> Result<()> {
    let mut assembler = Assembler::new(project_dir, nls)?;
    let entry_point = assembler.compile(assembler.config.entry_point)?;
    let data = assembler.link(entry_point)?;
    let output_path = output.as_ref();
    std::fs::write(output_path, data)?;

    Ok(())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    project_dir: String,
    #[clap(short, long)]
    output: String,
    #[clap(short, long)]
    nls: Nls,
}

fn main() {
    env_logger::init();
    let args = Args::parse();
    if let Err(e) = compile(args.project_dir, args.output, args.nls) {
        log::error!("Error: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile() {
        let input = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../disassembler/testcase/Snow"
        ));
        let output = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/Snow_new.bin"
        ));
        let nls = Nls::ShiftJIS;
        compile(input, output, nls.clone()).unwrap();
        let _parser = rfvp::script::parser::Parser::new(output, nls).unwrap();
    }
}
