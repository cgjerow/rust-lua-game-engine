// Vertex shader
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    //Multiplication order is important when it comes to matrices. The vector goes on the right, and the matrices go on the left in order of importance.
    out.clip_position = vec4<f32>(model.position, 1.0);

    return out;
}

// Fragment shader
@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
let color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
   if (color.a < .01) {
        discard;
    }
    return color;
}
