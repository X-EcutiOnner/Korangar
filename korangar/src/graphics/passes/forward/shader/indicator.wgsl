struct GlobalUniforms {
    view_projection: mat4x4<f32>,
    view: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    inverse_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    indicator_positions: mat4x4<f32>,
    indicator_color: vec4<f32>,
    ambient_color: vec4<f32>,
    camera_position: vec4<f32>,
    forward_size: vec2<u32>,
    interface_size: vec2<u32>,
    pointer_position: vec2<u32>,
    animation_timer: f32,
    day_timer: f32,
    point_light_count: u32,
    enhanced_lighting: u32,
    shadow_quality: u32,
}

struct DirectionalLightUniforms {
    view_projection: mat4x4<f32>,
    color: vec4<f32>,
    direction: vec4<f32>,
}

struct PointLight {
    position: vec4<f32>,
    color: vec4<f32>,
    range: f32,
    texture_index: i32,
}

struct TileLightIndices {
    indices: array<u32, 256>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texture_coordinates: vec2<f32>,
}

const TILE_SIZE: u32 = 16;

@group(0) @binding(0) var<uniform> global_uniforms: GlobalUniforms;
@group(0) @binding(1) var nearest_sampler: sampler;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(4) var shadow_map_sampler: sampler_comparison;
@group(1) @binding(0) var<uniform> directional_light: DirectionalLightUniforms;
@group(1) @binding(1) var shadow_map: texture_depth_2d;
@group(1) @binding(2) var<storage, read> point_lights: array<PointLight>;
@group(1) @binding(3) var light_count_texture: texture_2d<u32>;
@group(1) @binding(4) var<storage, read> tile_light_indices: array<TileLightIndices>;
@group(1) @binding(5) var point_shadow_maps: texture_depth_cube_array;
@group(2) @binding(0) var texture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let world_position = position_data(vertex_index);

    var output: VertexOutput;
    output.position = global_uniforms.view_projection * world_position;
    output.world_position = world_position;
    output.normal = normal_data(vertex_index);
    output.texture_coordinates = uv_data(vertex_index);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let diffuse_color = textureSample(texture, nearest_sampler, input.texture_coordinates);

    // Calculate which tile this fragment belongs to
    let pixel_position = vec2<u32>(floor(input.position.xy));
    let tile_x = pixel_position.x / TILE_SIZE;
    let tile_y = pixel_position.y / TILE_SIZE;
    let tile_count_x = (global_uniforms.forward_size.x + TILE_SIZE - 1u) / TILE_SIZE;
    let tile_index = tile_y * tile_count_x + tile_x;

    // Get the number of lights affecting this tile
    let light_count = textureLoad(light_count_texture, vec2<u32>(tile_x, tile_y), 0).r;

    if (diffuse_color.a < 0.1) {
        discard;
    }

    let normal = normalize(input.normal);

    // Ambient light
    var ambient_light_contribution = global_uniforms.ambient_color.rgb;

    // Directional light
    let light_direction = normalize(-directional_light.direction.xyz);
    let light_percent = max(dot(light_direction, normal), 0.0);

    // Shadow calculation
    let shadow_position = directional_light.view_projection * input.world_position;
    var shadow_coords = shadow_position.xyz / shadow_position.w;
    let bias = get_oriented_bias(normal, light_direction);
    let world_position = input.world_position.xyz / input.world_position.w;
    shadow_coords = vec3<f32>(clip_to_screen_space(shadow_coords.xy), shadow_coords.z + bias);

    var visibility: f32;

    switch (global_uniforms.shadow_quality) {
        case 1u: {
            let shadow_map_dimensions = textureDimensions(shadow_map);
            visibility = get_soft_shadow(shadow_coords, shadow_map_dimensions);
        }
        default: {
            visibility = textureSampleCompare(
                      shadow_map,
                      shadow_map_sampler,
                      shadow_coords.xy,
                      shadow_coords.z
            );
        }
    }

    let directional_light_contribution = directional_light.color.rgb * light_percent * visibility;

    // Point lights
    var point_light_contribution = vec3<f32>(0.0);
    for (var index = 0u; index < light_count; index++) {
        let light_index = tile_light_indices[tile_index].indices[index];
        let light = point_lights[light_index];
        let light_direction = normalize(input.world_position.xyz - light.position.xyz);
        let light_percent = max(dot(light_direction, input.normal), 0.0);
        let light_distance = length(light.position.xyz - input.world_position.xyz);
        var visibility = 1.0;

        if (light.texture_index != 0) {
            let bias = 1.2;
            let distance_to_light = linearToNonLinear(light_distance - bias);

            let closest_distance = textureSample(
                point_shadow_maps,
                linear_sampler,
                light_direction,
                light.texture_index - 1
            );

            visibility = f32(distance_to_light > closest_distance);
        }

        let intensity = 10.0;
        let attenuation = calculate_attenuation(light_distance, light.range);
        point_light_contribution += (light.color.rgb * intensity) * light_percent * attenuation * visibility;
    }

    let base_color = diffuse_color * global_uniforms.indicator_color;
    let light_contributions = saturate(ambient_light_contribution + directional_light_contribution + point_light_contribution);
    var color = base_color.rgb * light_contributions;

    if (global_uniforms.enhanced_lighting == 0) {
        color = color_balance(color, -0.01, 0.0, 0.0);
    }

    return vec4<f32>(color, base_color.a);
}

// Assuming inputs are in range [-1, 1] where:
// -1 = full shift towards first color (Cyan/Magenta/Yellow)
// +1 = full shift towards second color (Red/Green/Blue)
fn color_balance(color: vec3<f32>, cyan_red: f32, magenta_green: f32, yellow_blue: f32) -> vec3<f32> {
    let rgb = color;

    let adjusted = vec3<f32>(
        rgb.r + cyan_red,
        rgb.g + magenta_green,
        rgb.b + yellow_blue
    );

    return clamp(adjusted, vec3<f32>(0.0), vec3<f32>(1.0));
}

// Quadratic attenuation with smooth falloff
fn calculate_attenuation(distance: f32, range: f32) -> f32 {
    let effective_distance = min(distance, range);
    let normalized_distance = effective_distance / range;
    let attenuation = saturate(1.0 - normalized_distance * normalized_distance);
    return attenuation * attenuation;
}

fn clip_to_screen_space(ndc: vec2<f32>) -> vec2<f32> {
    let u = (ndc.x + 1.0) / 2.0;
    let v = (1.0 - ndc.y) / 2.0;
    return vec2<f32>(u, v);
}

fn position_data(vertex_index: u32) -> vec4<f32> {
    switch (vertex_index) {
        case 0u: {
            // upper_left
            return global_uniforms.indicator_positions[0u];
        }
        case 1u: {
            // upper_right
            return global_uniforms.indicator_positions[1u];
        }
        case 2u: {
            // lower_left
            return global_uniforms.indicator_positions[2u];
        }
        case 3u: {
            // lower_left
            return global_uniforms.indicator_positions[2u];
        }
        case 4u: {
            // upper_right
            return global_uniforms.indicator_positions[1u];
        }
        default: {
            // lower_right
            return global_uniforms.indicator_positions[3u];
        }
    }
}

// Optimized version of the following truth table:
//
// vertex_index  x  y
// 0             0  0
// 1             0  1
// 2             1  0
// 3             1  0
// 4             0  1
// 5             1  1
//
// (x,y) are the UV coordinates
fn uv_data(vertex_index: u32) -> vec2<f32> {
    let index = 1u << vertex_index;
    let x = f32((index & 0x2Cu) != 0u);
    let y = f32((index & 0x32u) != 0u);
    return vec2<f32>(x, y);
}

fn normal_data(vertex_index: u32) -> vec3<f32> {
    if (vertex_index < 3) {
        return normalize(cross(
            // upper_right - upper_left, lower_left - upper_left
            (global_uniforms.indicator_positions[1] - global_uniforms.indicator_positions[0]).xyz,
            (global_uniforms.indicator_positions[2] - global_uniforms.indicator_positions[0]).xyz
        ));
    } else {
        return normalize(cross(
            // upper_right - lower_left, lower_right - lower_left
            (global_uniforms.indicator_positions[1] - global_uniforms.indicator_positions[2]).xyz,
            (global_uniforms.indicator_positions[3] - global_uniforms.indicator_positions[2]).xyz
        ));
    }
}

fn get_soft_shadow(shadow_coords: vec3<f32>, shadow_map_dimensions: vec2<u32>) -> f32 {
    var gaussian_offset: i32;
    switch (shadow_map_dimensions.x) {
        case 8192u: {
            gaussian_offset = 8;
        }
        case 4096u: {
            gaussian_offset = 4;
        }
        default: {
            gaussian_offset = 2;
        }
    }

    let texel_size = vec2<f32>(1.0) / vec2<f32>(shadow_map_dimensions);
    let depth = shadow_coords.z;
    var shadow: f32 = 0.0;
    var total_weight: f32 = 0.0;

    let gaussian_offset_pow2 = f32(gaussian_offset * gaussian_offset);
    let sigma_squared = gaussian_offset_pow2 * 0.25;
    let weight_factor = 1.0 / (2.0 * sigma_squared);

    for (var y: i32 = -gaussian_offset; y <= gaussian_offset; y += 2) {
        for (var x: i32 = -gaussian_offset; x <= gaussian_offset; x += 2) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel_size;

            // Calculate Gaussian weight based on distance from center.
            let distance_squared = f32(x * x + y * y);
            let weight = exp(-distance_squared * weight_factor);

            let samples = textureGatherCompare(
                shadow_map,
                shadow_map_sampler,
                shadow_coords.xy + offset,
                depth
            );

            shadow += (samples.x + samples.y + samples.z + samples.w) * weight;
            total_weight += 4.0 * weight;
        }
    }

    return shadow / total_weight;
}

fn linearToNonLinear(linear_depth: f32) -> f32 {
    const NEAR_PLANE = 0.1;
    return NEAR_PLANE / (linear_depth + 1e-7);
}

// Based on "Shadow Techniques from Final Fantasy XVI" by Sammy Fatnassi (2023)
fn get_oriented_bias(normal: vec3<f32>, light_direction: vec3<f32>) -> f32 {
    let bias = 0.002;
    let is_facing_light = dot(normal, light_direction) > 0.0;
    // sic! We use reverse Z projection!
    return select(-bias, bias, is_facing_light);
}
