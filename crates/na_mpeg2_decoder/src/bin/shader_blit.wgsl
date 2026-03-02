@group(0) @binding(0)
var t_video: texture_2d<f32>;

@group(0) @binding(1)
var s_video: sampler;

struct VSOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VSOut {
  var positions = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -3.0),
    vec2<f32>( 3.0,  1.0),
    vec2<f32>(-1.0,  1.0)
  );
  var uvs = array<vec2<f32>, 3>(
    vec2<f32>(0.0, 2.0),
    vec2<f32>(2.0, 0.0),
    vec2<f32>(0.0, 0.0)
  );

  var out: VSOut;
  out.pos = vec4<f32>(positions[vid], 0.0, 1.0);
  out.uv = uvs[vid];
  return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
  return textureSample(t_video, s_video, in.uv);
}
