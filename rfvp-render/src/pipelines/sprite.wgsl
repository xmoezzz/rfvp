struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) texture_coordinate: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) texture_coordinate: vec2<f32>,
}

@group(0) @binding(0)
var sprite_texture: texture_2d<f32>;
@group(0) @binding(1)
var sprite_sampler: sampler;

struct SpriteParams {
    transform: mat4x4<f32>,
}

var<push_constant> params: SpriteParams;

@vertex
fn vertex_main(input: VertexIn) -> VertexOutput {
    var output: VertexOutput;
    output.position = params.transform * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    output.texture_coordinate = input.texture_coordinate;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(sprite_texture, sprite_sampler, input.texture_coordinate) * input.color;
}