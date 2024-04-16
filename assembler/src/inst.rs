use rfvp::script::parser::Nls;

pub trait Inst {
    fn address(&self) -> u32;
    fn set_address(&mut self, address: u32);
    fn serialize_to_binary(&self) -> Vec<u8>;
    fn size(&self) -> u32;
}

pub struct NopInst {
    address: u32,
}

impl NopInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for NopInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x00]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct InitStackInst {
    address: u32,
    arg_count: u8,
    locals_count: u8,
}

impl InitStackInst {
    pub fn new(arg_count: u8, locals_count: u8) -> Self {
        Self {
            address: 0,
            arg_count,
            locals_count,
        }
    }
}

impl Inst for InitStackInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x01, self.arg_count, self.locals_count]
    }

    fn size(&self) -> u32 {
        3
    }
}

pub struct CallInst {
    address: u32,
    func_address: u32,
}

impl CallInst {
    pub fn new(func_address: u32) -> Self {
        Self {
            address: 0,
            func_address,
        }
    }

    pub fn set_func_target(&mut self, target: u32) {
        self.func_address = target;
    }

    pub fn get_old_func_target(&self) -> u32 {
        self.func_address
    }
}

impl Inst for CallInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x02];
        bytes.extend_from_slice(&self.func_address.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        5
    }
}

pub struct SyscallInst {
    address: u32,
    syscall_id: u16,
}

impl SyscallInst {
    pub fn new(syscall_id: u16) -> Self {
        Self {
            address: 0,
            syscall_id,
        }
    }
}

impl Inst for SyscallInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x03];
        bytes.extend_from_slice(&self.syscall_id.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        3
    }
}

pub struct RetInst {
    address: u32,
}

impl RetInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for RetInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x04]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct RetVInst {
    address: u32,
}

impl RetVInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for RetVInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x05]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct JmpInst {
    address: u32,
    target_address: u32,
}

impl JmpInst {
    pub fn new(target_address: u32) -> Self {
        Self {
            address: 0,
            target_address,
        }
    }

    pub fn set_target(&mut self, target: u32) {
        self.target_address = target;
    }

    pub fn get_old_target(&self) -> u32 {
        self.target_address
    }
}

impl Inst for JmpInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x06];
        bytes.extend_from_slice(&self.target_address.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        5
    }
}

pub struct JzInst {
    address: u32,
    target_address: u32,
}

impl JzInst {
    pub fn new(target_address: u32) -> Self {
        Self {
            address: 0,
            target_address,
        }
    }

    pub fn set_target(&mut self, target: u32) {
        self.target_address = target;
    }

    pub fn get_old_target(&self) -> u32 {
        self.target_address
    }
}

impl Inst for JzInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x07];
        bytes.extend_from_slice(&self.target_address.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        5
    }
}

pub struct PushNilInst {
    address: u32,
}

impl PushNilInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for PushNilInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x08]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct PushTrueInst {
    address: u32,
}

impl PushTrueInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for PushTrueInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x09]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct PushI32Inst {
    address: u32,
    value: i32,
}

impl PushI32Inst {
    pub fn new(value: i32) -> Self {
        Self { address: 0, value }
    }
}

impl Inst for PushI32Inst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x0A];
        bytes.extend_from_slice(&self.value.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        5
    }
}

pub struct PushI16Inst {
    address: u32,
    value: i16,
}

impl PushI16Inst {
    pub fn new(value: i16) -> Self {
        Self { address: 0, value }
    }
}

impl Inst for PushI16Inst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x0B];
        bytes.extend_from_slice(&self.value.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        3
    }
}

pub struct PushI8Inst {
    address: u32,
    value: i8,
}

impl PushI8Inst {
    pub fn new(value: i8) -> Self {
        Self { address: 0, value }
    }
}

impl Inst for PushI8Inst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x0C, self.value as u8]
    }

    fn size(&self) -> u32 {
        2
    }
}

pub struct PushF32Inst {
    address: u32,
    value: f32,
}

impl PushF32Inst {
    pub fn new(value: f32) -> Self {
        Self { address: 0, value }
    }
}

impl Inst for PushF32Inst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x0D];
        bytes.extend_from_slice(&self.value.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        5
    }
}

pub struct PushStringInst {
    address: u32,
    content: String,
    content_blob: Vec<u8>,
    nls: Nls,
}

impl PushStringInst {
    pub fn new(content: String, nls: Nls) -> Self {
        Self {
            address: 0,
            content: content.clone(),
            content_blob: Self::string_to_blob(&content, nls.clone()),
            nls,
        }
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
}

impl Inst for PushStringInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x0E];
        if self.content_blob.len() > 0xFF {
            panic!("String too long");
        }
        bytes.push(self.content_blob.len() as u8);
        bytes.extend_from_slice(&self.content_blob);
        bytes
    }

    fn size(&self) -> u32 {
        self.content_blob.len() as u32 + 2
    }
}

pub struct PushGlobalInst {
    address: u32,
    idx: u16,
}

impl PushGlobalInst {
    pub fn new(idx: u16) -> Self {
        Self { address: 0, idx }
    }
}

impl Inst for PushGlobalInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x0F];
        bytes.extend_from_slice(&self.idx.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        3
    }
}

pub struct PushStackInst {
    address: u32,
    idx: i8,
}

impl PushStackInst {
    pub fn new(idx: i8) -> Self {
        Self { address: 0, idx }
    }
}

impl Inst for PushStackInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x10, self.idx as u8]
    }

    fn size(&self) -> u32 {
        2
    }
}

pub struct PushGlobalTableInst {
    address: u32,
    idx: u16,
}

impl PushGlobalTableInst {
    pub fn new(idx: u16) -> Self {
        Self { address: 0, idx }
    }
}

impl Inst for PushGlobalTableInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x11];
        bytes.extend_from_slice(&self.idx.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        3
    }
}

pub struct PushLocalTableInst {
    address: u32,
    idx: i8,
}

impl PushLocalTableInst {
    pub fn new(idx: i8) -> Self {
        Self { address: 0, idx }
    }
}

impl Inst for PushLocalTableInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x12, self.idx as u8]
    }

    fn size(&self) -> u32 {
        2
    }
}

pub struct PushTopInst {
    address: u32,
}

impl PushTopInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for PushTopInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x13]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct PushReturnInst {
    address: u32,
}

impl PushReturnInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for PushReturnInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x14]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct PopGlobalInst {
    address: u32,
    idx: u16,
}

impl PopGlobalInst {
    pub fn new(idx: u16) -> Self {
        Self { address: 0, idx }
    }
}

impl Inst for PopGlobalInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x15];
        bytes.extend_from_slice(&self.idx.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        3
    }
}

pub struct PopStackInst {
    address: u32,
    idx: i8,
}

impl PopStackInst {
    pub fn new(idx: i8) -> Self {
        Self { address: 0, idx }
    }
}

impl Inst for PopStackInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x16, self.idx as u8]
    }

    fn size(&self) -> u32 {
        2
    }
}

pub struct PopGlobalTableInst {
    address: u32,
    idx: u16,
}

impl PopGlobalTableInst {
    pub fn new(idx: u16) -> Self {
        Self { address: 0, idx }
    }
}

impl Inst for PopGlobalTableInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x17];
        bytes.extend_from_slice(&self.idx.to_le_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        3
    }
}

pub struct PopLocalTableInst {
    address: u32,
    idx: i8,
}

impl PopLocalTableInst {
    pub fn new(idx: i8) -> Self {
        Self { address: 0, idx }
    }
}

impl Inst for PopLocalTableInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x18, self.idx as u8]
    }

    fn size(&self) -> u32 {
        2
    }
}

pub struct NegInst {
    address: u32,
}

impl NegInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for NegInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x19]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct AddInst {
    address: u32,
}

impl AddInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for AddInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x1A]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct SubInst {
    address: u32,
}

impl SubInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for SubInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x1B]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct MulInst {
    address: u32,
}

impl MulInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for MulInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x1C]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct DivInst {
    address: u32,
}

impl DivInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for DivInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x1D]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct ModInst {
    address: u32,
}

impl ModInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for ModInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x1E]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct BitTestInst {
    address: u32,
}

impl BitTestInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for BitTestInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x1F]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct AndInst {
    address: u32,
}

impl AndInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for AndInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x20]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct OrInst {
    address: u32,
}

impl OrInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for OrInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x21]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct SetEInst {
    address: u32,
}

impl SetEInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for SetEInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x22]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct SetNEInst {
    address: u32,
}

impl SetNEInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for SetNEInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x23]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct SetGInst {
    address: u32,
}

impl SetGInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for SetGInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x24]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct SetLEInst {
    address: u32,
}

impl SetLEInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for SetLEInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x25]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct SetLInst {
    address: u32,
}

impl SetLInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for SetLInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x26]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct SetGEInst {
    address: u32,
}

impl SetGEInst {
    pub fn new() -> Self {
        Self { address: 0 }
    }
}

impl Inst for SetGEInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x27]
    }

    fn size(&self) -> u32 {
        1
    }
}
