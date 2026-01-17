use anyhow::{anyhow, bail, Context, Result};
use serde_yaml::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub enum Nls {
    Utf8,
    ShiftJis,
    Gb18030,
}

impl Nls {
    pub fn parse(s: &str) -> Result<Self> {
        let ss = s.trim().to_ascii_lowercase();
        match ss.as_str() {
            "utf8" | "utf-8" => Ok(Nls::Utf8),
            "sjis" | "shiftjis" | "shift_jis" | "shift-jis" => Ok(Nls::ShiftJis),
            "gbk" | "gb18030" => Ok(Nls::Gb18030),
            other => bail!("unsupported nls: {other}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Syscall {
    pub id: u16,
    pub args: u8,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Meta {
    pub nls: Nls,
    pub game_title: String,
    pub game_mode: u16,
    pub non_volatile_global_count: u16,
    pub volatile_global_count: u16,
    pub custom_syscall_count: u16,
    pub syscalls: Vec<Syscall>,

    name_to_id: HashMap<String, u16>,
    id_to_args: HashMap<u16, u8>,
}

impl Meta {
    pub fn syscall_id_by_name(&self, name: &str) -> Option<u16> {
        self.name_to_id.get(name).copied()
    }

    pub fn syscall_args_by_id(&self, id: u16) -> Option<u8> {
        self.id_to_args.get(&id).copied()
    }

    pub fn syscall_count(&self) -> u16 {
        self.syscalls.len() as u16
    }
}

fn as_u16(v: &Value, key: &str) -> Result<u16> {
    match v {
        Value::Number(n) => n
            .as_u64()
            .and_then(|x| u16::try_from(x).ok())
            .ok_or_else(|| anyhow!("{key} must be a u16")),
        _ => bail!("{key} must be a number"),
    }
}

fn as_u32_opt(v: Option<&Value>) -> Result<Option<u32>> {
    if let Some(Value::Number(n)) = v {
        return Ok(Some(
            n.as_u64()
                .and_then(|x| u32::try_from(x).ok())
                .ok_or_else(|| anyhow!("value must be a u32"))?,
        ));
    }
    Ok(None)
}

fn as_str(v: &Value, key: &str) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        _ => bail!("{key} must be a string"),
    }
}

pub fn load_meta(path: &Path) -> Result<Meta> {
    let txt = std::fs::read_to_string(path).with_context(|| format!("read meta: {}", path.display()))?;
    let doc: Value = serde_yaml::from_str(&txt).context("parse yaml")?;
    let map = doc
        .as_mapping()
        .ok_or_else(|| anyhow!("meta must be a mapping"))?;

    let get = |k: &str| -> Option<&Value> { map.get(&Value::String(k.to_string())) };

    let nls = if let Some(v) = get("nls") {
        Nls::parse(&as_str(v, "nls")?)?
    } else {
        Nls::ShiftJis
    };

    let game_title = if let Some(v) = get("game_title") {
        as_str(v, "game_title")?
    } else {
        String::new()
    };

    let game_mode = if let Some(v) = get("game_mode") {
        as_u16(v, "game_mode")?
    } else {
        0
    };

    let non_volatile_global_count = if let Some(v) = get("non_volatile_global_count") {
        as_u16(v, "non_volatile_global_count")?
    } else {
        0
    };

    let volatile_global_count = if let Some(v) = get("volatile_global_count") {
        as_u16(v, "volatile_global_count")?
    } else {
        0
    };

    let custom_syscall_count = if let Some(v) = get("custom_syscall_count") {
        as_u16(v, "custom_syscall_count")?
    } else {
        0
    };

    // Accept legacy fields but ignore them (must be recomputed).
    let _sys_desc_offset = as_u32_opt(get("sys_desc_offset")).unwrap_or(None);
    let _entry_point = as_u32_opt(get("entry_point")).unwrap_or(None);

    // Parse syscalls: either a sequence (legacy) or a mapping id -> {name,args} (new).
    let syscalls_v = get("syscalls").ok_or_else(|| anyhow!("meta.syscalls is required"))?;

    let mut syscalls: Vec<Syscall> = Vec::new();

    match syscalls_v {
        Value::Sequence(seq) => {
            // Legacy: list of {name, args} objects, IDs are implicit 0..N-1.
            for (i, it) in seq.iter().enumerate() {
                let m = it
                    .as_mapping()
                    .ok_or_else(|| anyhow!("meta.syscalls[{i}] must be a mapping"))?;
                let name = m
                    .get(&Value::String("name".to_string()))
                    .ok_or_else(|| anyhow!("meta.syscalls[{i}].name missing"))?;
                let args = m
                    .get(Value::String("args".to_string()))
                    .unwrap();

                let name = as_str(name, "name")?;
                let argc = as_u16(args, "args")?;
                let argc_u8 = u8::try_from(argc).map_err(|_| anyhow!("syscall args must fit u8"))?;

                syscalls.push(Syscall {
                    id: u16::try_from(i).unwrap(),
                    args: argc_u8,
                    name,
                });
            }
        }
        Value::Mapping(m) => {
            // New: id -> {name,args}
            // Use BTreeMap for deterministic ordering.
            let mut tmp: BTreeMap<u16, Syscall> = BTreeMap::new();
            for (k, v) in m.iter() {
                let id = match k {
                    Value::Number(n) => n
                        .as_u64()
                        .and_then(|x| u16::try_from(x).ok())
                        .ok_or_else(|| anyhow!("syscalls key must fit u16"))?,
                    Value::String(s) => s
                        .parse::<u16>()
                        .map_err(|_| anyhow!("syscalls key must be integer"))?,
                    _ => bail!("syscalls key must be integer"),
                };

                let vm = v
                    .as_mapping()
                    .ok_or_else(|| anyhow!("syscalls[{id}] must be a mapping"))?;

                let name_v = vm
                    .get(&Value::String("name".to_string()))
                    .ok_or_else(|| anyhow!("syscalls[{id}].name missing"))?;
                let args_v = vm
                    .get(&Value::String("args".to_string()))
                    .ok_or_else(|| anyhow!("syscalls[{id}].args missing"))?;

                let name = as_str(name_v, "name")?;
                let argc = as_u16(args_v, "args")?;
                let argc_u8 = u8::try_from(argc).map_err(|_| anyhow!("syscall args must fit u8"))?;

                if tmp.contains_key(&id) {
                    bail!("duplicate syscall id: {id}");
                }
                tmp.insert(
                    id,
                    Syscall {
                        id,
                        args: argc_u8,
                        name,
                    },
                );
            }

            // Validate contiguity if syscall_count exists.
            let declared_count = get("syscall_count")
                .and_then(|v| as_u16(v, "syscall_count").ok());
            let count = if let Some(dc) = declared_count {
                dc
            } else {
                // infer count as max_id+1
                tmp.keys().next_back().map(|x| x + 1).unwrap_or(0)
            };

            for id in 0..count {
                if !tmp.contains_key(&id) {
                    bail!("missing syscall id {id} in meta.syscalls");
                }
            }

            syscalls = tmp.into_values().collect();

            if declared_count.is_some() {
                if syscalls.len() != usize::from(count) {
                    bail!("syscall_count mismatch: declared {count}, found {}", syscalls.len());
                }
            }
        }
        _ => bail!("meta.syscalls must be a sequence or mapping"),
    }

    let mut name_to_id: HashMap<String, u16> = HashMap::new();
    let mut id_to_args: HashMap<u16, u8> = HashMap::new();
    for sc in &syscalls {
        if name_to_id.insert(sc.name.clone(), sc.id).is_some() {
            bail!("duplicate syscall name: {}", sc.name);
        }
        id_to_args.insert(sc.id, sc.args);
    }

    Ok(Meta {
        nls,
        game_title,
        game_mode,
        non_volatile_global_count,
        volatile_global_count,
        custom_syscall_count,
        syscalls,
        name_to_id,
        id_to_args,
    })
}
