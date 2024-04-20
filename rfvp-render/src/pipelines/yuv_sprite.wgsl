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
var y_texture: texture_2d<f32>;
@group(0) @binding(1)
var u_texture: texture_2d<f32>;
@group(0) @binding(2)
var v_texture: texture_2d<f32>;
@group(0) @binding(3)
var sprite_sampler: sampler;

struct YuvSpriteParams {
    transform: mat4x4<f32>,
}

var<push_constant> params: YuvSpriteParams;

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
    let conv_matrix: mat3x3<f32> = mat3x3(
            1.0, 1.0, 1.0,
            0.0, -0.1873, -1.8556,
            1.5748, -0.4681, 0.0
    );

    let y = textureSample(y_texture, sprite_sampler, input.texture_coordinate).r * 255.0;
    let u = textureSample(u_texture, sprite_sampler, input.texture_coordinate).r * 255.0;
    let v = textureSample(v_texture, sprite_sampler, input.texture_coordinate).r * 255.0;

    let rgb = vec3(
        (y + 1.402 * (v - 128.0)),
        (y - 0.344 * (u - 128.0) - 0.714 * (v - 128.0)),
        (y + 1.772 * (u - 128.0)),
    ) / 255.0;

    return vec4(rgb, 1.0);
}