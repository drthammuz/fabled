// Terrain: blends grass / dirt / rock by slope, on top of the standard
// PBR pipeline (extends StandardMaterial via MaterialExtension).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var grass_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var grass_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var dirt_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var dirt_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var rock_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var rock_sampler: sampler;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // World-space planar UVs: seamless tiling without mesh UV seams.
    let uv_soil = in.world_position.xz / 4.0;
    let uv_rock = in.world_position.xz / 7.0;
    let grass = textureSample(grass_texture, grass_sampler, uv_soil).rgb;
    let dirt = textureSample(dirt_texture, dirt_sampler, uv_soil).rgb;
    let rock = textureSample(rock_texture, rock_sampler, uv_rock).rgb;

    // Flat ground is grass; gentle slopes blend to dirt; steep gets rock.
    let slope = 1.0 - normalize(in.world_normal).y;
    let dirt_w = smoothstep(0.04, 0.12, slope);
    let rock_w = smoothstep(0.16, 0.30, slope);
    var ground = mix(grass, dirt, dirt_w);
    ground = mix(ground, rock, rock_w);

    pbr_input.material.base_color = vec4<f32>(ground, 1.0)
        * pbr_input.material.base_color;
    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
