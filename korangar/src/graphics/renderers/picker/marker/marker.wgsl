struct Constants {
    screen_position: vec2<f32>,
    screen_size: vec2<f32>,
    identifier_high: u32,
    identifier_low: u32,
}

var<push_constant> constants: Constants;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let vertex = vertex_data(vertex_index);
    let clip_size = constants.screen_size * 2.0;
    let position = screen_to_clip_space(constants.screen_position) + vertex.xy * clip_size;
    return vec4<f32>(position, 1.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec2<u32> {
    return vec2<u32>(constants.identifier_low, constants.identifier_high);
}

// Optimized version of the following truth table:
//
// vertex_index  x  y
// 0             0  0
// 1             1  0
// 2             1 -1
// 3             1 -1
// 4             0 -1
// 5             0  0
//
// (x,y) are the vertex position
fn vertex_data(vertex_index: u32) -> vec2<f32> {
    let index = 1u << vertex_index;
    let x = f32((index & 0xEu) != 0u);
    let y = f32((index & 0x1Cu) != 0u);
    return vec2<f32>(x, -y);
}

fn screen_to_clip_space(screen_coords: vec2<f32>) -> vec2<f32> {
    let x = (screen_coords.x * 2.0) - 1.0;
    let y = -(screen_coords.y * 2.0) + 1.0;
    return vec2<f32>(x, y);
}