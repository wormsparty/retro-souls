// Matériau « PS1 » : vertex snapping, texture affine, éclairage par sommet (Gouraud),
// brouillard, couleur 15 bits avec dithering ordonné 4x4.

#import bevy_pbr::{
    mesh_functions,
    forward_io::Vertex,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals},
}
#ifdef SKINNED
#import bevy_pbr::skinning
#endif

const MAX_LIGHTS: u32 = 4u;

struct Ps1Params {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
    // rgb = couleur de teinte, a = intensité (flash d'impact, lueur furie).
    tint: vec4<f32>,
    // xyz = direction vers la lumière, w = intensité.
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient: vec4<f32>,
    fog_color: vec4<f32>,
    // x = début, y = fin du brouillard, z = finesse de la grille de snapping, w = force du dithering.
    fog: vec4<f32>,
    // x = texture présente, y = non éclairé.
    misc: vec4<f32>,
    // Triplets (position xyz + rayon, couleur rgb + intensité, vacillement a1 f1 a2 f2).
    lights: array<vec4<f32>, 12>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> m: Ps1Params;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var color_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var color_sampler: sampler;

/// Hauteur sous laquelle le décor commence à se fondre dans le noir, et sur quelle épaisseur.
const ABYSS_TOP: f32 = -6.5;
const ABYSS_DEPTH: f32 = 14.0;
/// Couleur du fond (couleur d'effacement de la caméra, en linéaire) : l'abîme s'y fond exactement.
const ABYSS_COLOR: vec3<f32> = vec3<f32>(0.0039, 0.0035, 0.0061);

struct Ps1Out {
    @builtin(position) position: vec4<f32>,
    // uv * w et w : l'interpolation perspective de ce couple redonne une interpolation affine.
    @location(0) uvw: vec3<f32>,
    @location(1) light: vec3<f32>,
    @location(2) fog: f32,
    @location(3) color: vec4<f32>,
    @location(4) abyss: f32,
};

@vertex
fn vertex(v: Vertex) -> Ps1Out {
    var out: Ps1Out;
#ifdef SKINNED
    let world_from_local = skinning::skin_model(v.joint_indices, v.joint_weights, v.instance_index);
#else
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
#endif
    let wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0)).xyz;
    var clip = position_world_to_clip(wp);

    // Vertex snapping sur une grille en espace écran.
    if clip.w > 0.0 {
        let grid = view.viewport.zw * 0.5 * m.fog.z;
        let ndc = clip.xy / clip.w;
        let snapped = floor(ndc * grid + 0.5) / grid;
        clip = vec4<f32>(snapped * clip.w, clip.z, clip.w);
    }
    out.position = clip;

#ifdef VERTEX_NORMALS
#ifdef SKINNED
    let n = normalize(skinning::skin_normals(world_from_local, v.normal));
#else
    let n = normalize(mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index));
#endif
#else
    let n = vec3<f32>(0.0, 1.0, 0.0);
#endif

    var light = m.ambient.rgb + m.sun_color.rgb * max(dot(n, m.sun_dir.xyz), 0.0) * m.sun_dir.w;
    for (var i = 0u; i < MAX_LIGHTS; i++) {
        let lp = m.lights[i * 3u];
        let lc = m.lights[i * 3u + 1u];
        let fl = m.lights[i * 3u + 2u];
        if lc.w <= 0.0 {
            continue;
        }
        let phase = lp.x * 1.3 + lp.z * 0.7;
        let intensity = lc.w + fl.x * sin(globals.time * fl.y + phase) + fl.z * sin(globals.time * fl.w + phase * 2.0);
        let d = lp.xyz - wp;
        let dist = max(length(d), 0.001);
        let att = clamp(1.0 - dist / lp.w, 0.0, 1.0);
        light += lc.rgb * intensity * att * att * (0.35 + 0.65 * max(dot(n, d / dist), 0.0));
    }
    if m.misc.y > 0.5 {
        light = vec3<f32>(1.0);
    }
    out.light = light;

    let dist = length(wp - view.world_position);
    out.fog = clamp((dist - m.fog.x) / max(m.fog.y - m.fog.x, 0.001), 0.0, 1.0);
    // L'abîme : sous les ruines, tout se fond dans le noir (seules les lueurs restent).
    out.abyss = clamp((ABYSS_TOP - wp.y) / ABYSS_DEPTH, 0.0, 1.0);

#ifdef VERTEX_UVS_A
    out.uvw = vec3<f32>(v.uv * clip.w, clip.w);
#else
    out.uvw = vec3<f32>(0.0, 0.0, clip.w);
#endif
#ifdef VERTEX_COLORS
    out.color = v.color;
#else
    out.color = vec4<f32>(1.0);
#endif
    return out;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn bayer4(p: vec2<u32>) -> f32 {
    var m4 = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return m4[(p.y % 4u) * 4u + (p.x % 4u)] / 16.0;
}

@fragment
fn fragment(in: Ps1Out) -> @location(0) vec4<f32> {
    var c = m.base_color * in.color;
    let uv = in.uvw.xy / in.uvw.z;
    let t = textureSampleLevel(color_texture, color_sampler, uv, 0.0);
    c = select(c, c * t, m.misc.x > 0.5);

    var rgb = c.rgb * in.light + m.emissive.rgb;
    rgb = mix(rgb, m.tint.rgb, m.tint.a);
    rgb = mix(rgb, m.fog_color.rgb, in.fog);
    rgb = mix(rgb, ABYSS_COLOR, in.abyss);
    // Les lueurs (feux, lanternes, réverbères) percent le brouillard et l'abîme : au loin dans
    // le noir, il ne reste qu'elles.
    rgb += m.emissive.rgb * max(in.fog, in.abyss) * 0.7;

    // Quantification 5 bits par canal (comme la PS1) en espace sRGB, avec dithering.
    let d = (bayer4(vec2<u32>(in.position.xy)) - 0.5) * m.fog.w;
    var s = to_srgb(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
    s = floor(s * 31.0 + d + 0.5) / 31.0;
    return vec4<f32>(to_linear(clamp(s, vec3<f32>(0.0), vec3<f32>(1.0))), c.a);
}
