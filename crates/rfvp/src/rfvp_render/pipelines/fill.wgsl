struct Pc {
  m: mat4x4<f32>,
  color: vec4<f32>,
};

struct VsIn {
  @location(0) pos: vec3<f32>,
};

struct VsOut {
  @builtin(position) pos: vec4<f32>,
};

var<push_constant> pc: Pc;

@vertex
fn vs_main(v: VsIn) -> VsOut {
  var o: VsOut;
  o.pos = pc.m * vec4<f32>(v.pos, 1.0);
  return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
  _ = i;
  return pc.color;
}
