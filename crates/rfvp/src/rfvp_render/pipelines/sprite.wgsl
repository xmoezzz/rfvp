struct Pc {
  m: mat4x4<f32>,
};
@group(0) @binding(0) var t0: texture_2d<f32>;
@group(0) @binding(1) var s0: sampler;

struct VsIn {
  @location(0) pos: vec3<f32>,
  @location(1) col: vec4<f32>,
  @location(2) uv: vec2<f32>,
};

struct VsOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) col: vec4<f32>,
  @location(1) uv: vec2<f32>,
};

var<push_constant> pc: Pc;

@vertex
fn vs_main(v: VsIn) -> VsOut {
  var o: VsOut;
  o.pos = pc.m * vec4<f32>(v.pos, 1.0);
  o.col = v.col;
  o.uv = v.uv;
  return o;
}

// @fragment
// fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
//   let tex = textureSample(t0, s0, i.uv);
//   return tex * i.col;
// }

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
  let dims_i = textureDimensions(t0);        // vec2<i32>
  let dims = vec2<f32>(f32(dims_i.x), f32(dims_i.y));
  let uv = i.uv / dims;                      // pixel -> normalized
  let tex = textureSample(t0, s0, uv);
  return tex * i.col;
}

