struct VertexOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) a_pos: vec2<f32>, @location(1) a_uv: vec2<f32>) -> VertexOut {
  var o: VertexOut;
  o.pos = vec4<f32>(a_pos, 0.0, 1.0);
  o.uv = a_uv;
  return o;
}

@group(0) @binding(0) var my_tex: texture_2d<f32>;
@group(0) @binding(1) var my_samp: sampler;

@fragment
fn fs_main(i: VertexOut) -> @location(0) vec4<f32> {
  return textureSample(my_tex, my_samp, i.uv);
}
