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

alias distances = array<vec2<f32>, 8>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_position0: vec2<f32>,
    @location(1) tex_position1: vec2<f32>,
    @location(2) tex_position2: vec2<f32>,
    @location(3) tex_position3: vec2<f32>,
    @location(4) tex_position4: vec2<f32>,
    @location(5) tex_position5: vec2<f32>,
    @location(6) tex_position6: vec2<f32>,
    @location(7) tex_position7: vec2<f32>,
    @location(8) fade: f32,
    @location(9) rela_time: f32,
}

@group(0) @binding(0)
var text_atlas: texture_2d<f32>;
@group(0) @binding(1)
var text_atlas_sampler: sampler;

struct TextParams {
    transform: mat4x4<f32>,
    time: f32,
    distance: f32,
}

var<push_constant> params: TextParams;

@vertex
fn vertex_main(input: VertexIn) -> VertexOutput {

    var DISTANCES = distances(
        vec2<f32>(-0.7071, -0.7071),
        vec2<f32>( 0.0,    -1.0),
        vec2<f32>( 0.7071, -0.7071),
        vec2<f32>(-1.0,     0.0),
        // NOTE: skipping the center
        vec2<f32>( 1.0,     0.0),
        vec2<f32>(-0.7071,  0.7071),
        vec2<f32>( 0.0,     1.0),
        vec2<f32>( 0.7071,  0.7071),
    );

    var output: VertexOutput;
    output.position = params.transform * vec4<f32>(input.position, 0.0, 1.0);

    output.tex_position0 = input.tex_position + DISTANCES[0] * params.distance;
    output.tex_position1 = input.tex_position + DISTANCES[1] * params.distance;
    output.tex_position2 = input.tex_position + DISTANCES[2] * params.distance;
    output.tex_position3 = input.tex_position + DISTANCES[3] * params.distance;
    output.tex_position4 = input.tex_position + DISTANCES[4] * params.distance;
    output.tex_position5 = input.tex_position + DISTANCES[5] * params.distance;
    output.tex_position6 = input.tex_position + DISTANCES[6] * params.distance;
    output.tex_position7 = input.tex_position + DISTANCES[7] * params.distance;

    output.fade = input.fade;
    output.rela_time = params.time - input.time;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var sampled: f32 = 0.0;
    sampled = max(sampled, textureSample(text_atlas, text_atlas_sampler, input.tex_position0).r);
    sampled = max(sampled, textureSample(text_atlas, text_atlas_sampler, input.tex_position1).r);
    sampled = max(sampled, textureSample(text_atlas, text_atlas_sampler, input.tex_position2).r);
    sampled = max(sampled, textureSample(text_atlas, text_atlas_sampler, input.tex_position3).r);
    sampled = max(sampled, textureSample(text_atlas, text_atlas_sampler, input.tex_position4).r);
    sampled = max(sampled, textureSample(text_atlas, text_atlas_sampler, input.tex_position5).r);
    sampled = max(sampled, textureSample(text_atlas, text_atlas_sampler, input.tex_position6).r);
    sampled = max(sampled, textureSample(text_atlas, text_atlas_sampler, input.tex_position7).r);

    var color = vec3<f32>(0.0, 0.0, 0.0);

    var fade_alpha: f32 = clamp(input.rela_time, 0.0, 1.0);
    return vec4<f32>(color, sampled * fade_alpha);
}