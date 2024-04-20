struct VertexIn {
    @location(0)
    position: vec2<f32>,
    @location(1)
    tex_position: vec2<f32>,
    @location(2)
    color: vec3<f32>,
    @location(3)
    time: f32,
    @location(4)
    fade: f32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) tex_position: vec2<f32>,
    @location(2) fade: f32,
    @location(3) rela_time: f32,
}

@group(0) @binding(0)
var text_atlas: texture_2d<f32>;
@group(0) @binding(1)
var text_atlas_sampler: sampler;

struct TextParams {
    transform: mat4x4<f32>,
    time: f32,
}

var<push_constant> params: TextParams;

@vertex
fn vertex_main(input: VertexIn) -> VertexOutput {
    var output: VertexOutput;
    output.position = params.transform * vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;
    output.tex_position = input.tex_position;
    output.fade = input.fade;
    output.rela_time = params.time - input.time;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var sampled: f32 = textureSample(text_atlas, text_atlas_sampler, input.tex_position).x;
    var fade_alpha: f32 = clamp(input.rela_time, 0.0, 1.0);
    return vec4<f32>(input.color, sampled * fade_alpha);
}