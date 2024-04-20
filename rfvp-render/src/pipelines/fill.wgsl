struct VertexIn {
    @location(0) position: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

struct FillParams {
    transform: mat4x4<f32>,
    color: vec4<f32>,
}

@fragment
var<push_constant> params: FillParams;

@vertex
fn vertex_main(input: VertexIn) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = params.transform * vec4<f32>(input.position, 1.0);
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return params.color;
}