use crate::vertex::wgpu;

const F16: usize = 2;

#[derive(Copy, Clone, Debug)]
pub struct TypeToWGPU {
    pub offset: u64,
    pub ty: wgpu::VertexFormat,
}

#[derive(Copy, Clone)]
pub struct WGPUData {
    pub wgpu_type: TypeToWGPU,
    pub shader_location: u32,
}

const TYPE_MAPPER: [(&str, TypeToWGPU); 34] = [
    (
        "u32",
        TypeToWGPU {
            offset: std::mem::size_of::<u32>() as u64,
            ty: wgpu::VertexFormat::Uint32,
        },
    ),
    (
        "f32",
        TypeToWGPU {
            offset: std::mem::size_of::<f32>() as u64,
            ty: wgpu::VertexFormat::Float32,
        },
    ),
    (
        "s32",
        TypeToWGPU {
            offset: std::mem::size_of::<i32>() as u64,
            ty: wgpu::VertexFormat::Sint32,
        },
    ),
    (
        "f64",
        TypeToWGPU {
            offset: std::mem::size_of::<f64>() as u64,
            ty: wgpu::VertexFormat::Float64,
        },
    ),
    (
        "u8x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[u8; 2]>() as u64,
            ty: wgpu::VertexFormat::Uint8x2,
        },
    ),
    (
        "u8x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[u8; 4]>() as u64,
            ty: wgpu::VertexFormat::Uint8x4,
        },
    ),
    (
        "s8x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[i8; 2]>() as u64,
            ty: wgpu::VertexFormat::Sint8x2,
        },
    ),
    (
        "s8x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[i8; 4]>() as u64,
            ty: wgpu::VertexFormat::Sint8x4,
        },
    ),
    (
        "un8x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[u8; 2]>() as u64,
            ty: wgpu::VertexFormat::Unorm8x2,
        },
    ),
    (
        "un8x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[u8; 4]>() as u64,
            ty: wgpu::VertexFormat::Unorm8x4,
        },
    ),
    (
        "sn8x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[i8; 2]>() as u64,
            ty: wgpu::VertexFormat::Snorm8x2,
        },
    ),
    (
        "sn8x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[i8; 4]>() as u64,
            ty: wgpu::VertexFormat::Snorm8x4,
        },
    ),
    (
        "u16x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[u16; 2]>() as u64,
            ty: wgpu::VertexFormat::Uint16x2,
        },
    ),
    (
        "u16x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[u16; 4]>() as u64,
            ty: wgpu::VertexFormat::Uint16x4,
        },
    ),
    (
        "s16x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[i16; 2]>() as u64,
            ty: wgpu::VertexFormat::Sint16x2,
        },
    ),
    (
        "s16x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[i16; 4]>() as u64,
            ty: wgpu::VertexFormat::Sint16x4,
        },
    ),
    (
        "un16x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[u16; 2]>() as u64,
            ty: wgpu::VertexFormat::Unorm16x2,
        },
    ),
    (
        "un16x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[u16; 4]>() as u64,
            ty: wgpu::VertexFormat::Unorm16x4,
        },
    ),
    (
        "sn16x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[i16; 2]>() as u64,
            ty: wgpu::VertexFormat::Snorm16x2,
        },
    ),
    (
        "sn16x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[i16; 4]>() as u64,
            ty: wgpu::VertexFormat::Snorm16x4,
        },
    ),
    (
        "f16x2",
        TypeToWGPU {
            offset: (F16 * 2) as u64,
            ty: wgpu::VertexFormat::Float16x2,
        },
    ),
    (
        "f16x4",
        TypeToWGPU {
            offset: (F16 * 4) as u64,
            ty: wgpu::VertexFormat::Float16x4,
        },
    ),
    (
        "f32x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[f32; 2]>() as u64,
            ty: wgpu::VertexFormat::Float32x2,
        },
    ),
    (
        "f32x3",
        TypeToWGPU {
            offset: std::mem::size_of::<[f32; 3]>() as u64,
            ty: wgpu::VertexFormat::Float32x3,
        },
    ),
    (
        "f32x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[f32; 4]>() as u64,
            ty: wgpu::VertexFormat::Float32x4,
        },
    ),
    (
        "u32x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[u32; 2]>() as u64,
            ty: wgpu::VertexFormat::Uint32x2,
        },
    ),
    (
        "u32x3",
        TypeToWGPU {
            offset: std::mem::size_of::<[u32; 3]>() as u64,
            ty: wgpu::VertexFormat::Uint32x3,
        },
    ),
    (
        "u32x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[u32; 4]>() as u64,
            ty: wgpu::VertexFormat::Uint32x4,
        },
    ),
    (
        "s32x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[i32; 2]>() as u64,
            ty: wgpu::VertexFormat::Sint32x2,
        },
    ),
    (
        "s32x3",
        TypeToWGPU {
            offset: std::mem::size_of::<[i32; 3]>() as u64,
            ty: wgpu::VertexFormat::Sint32x3,
        },
    ),
    (
        "s32x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[i32; 4]>() as u64,
            ty: wgpu::VertexFormat::Sint32x4,
        },
    ),
    (
        "f64x2",
        TypeToWGPU {
            offset: std::mem::size_of::<[f64; 2]>() as u64,
            ty: wgpu::VertexFormat::Float64x2,
        },
    ),
    (
        "f64x3",
        TypeToWGPU {
            offset: std::mem::size_of::<[f32; 3]>() as u64,
            ty: wgpu::VertexFormat::Float64x3,
        },
    ),
    (
        "f64x4",
        TypeToWGPU {
            offset: std::mem::size_of::<[f64; 4]>() as u64,
            ty: wgpu::VertexFormat::Float64x4,
        },
    ),
];

fn get_type(name: &str) -> Result<TypeToWGPU, String> {
    for i in TYPE_MAPPER {
        if i.0 == name {
            return Ok(i.1);
        }
    }

    Err(format!("Cannot get type for {:?}", name))
}

pub fn get_allowed_type(name: &str) -> Vec<&str> {
    let mut vec: Vec<&str> = Vec::new();
    for i in TYPE_MAPPER {
        if i.0.contains(name) {
            vec.push(i.0);
        }
    }
    vec
}

pub fn convert_type_to_wgpu(name: &String, shader_location: u32) -> Result<WGPUData, String> {
    let wgpu_type = get_type(name.as_str())?;

    Ok(WGPUData {
        wgpu_type,
        shader_location,
    })
}

pub fn has_type(name: &str) -> bool {
    for i in TYPE_MAPPER {
        if i.0 == name {
            return true;
        }
    }
    false
}
