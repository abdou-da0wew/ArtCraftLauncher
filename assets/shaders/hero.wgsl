// Ported from getartcraft.com's hero scene (`hero-galaxy.tsx`) — the WebGL
// field of bowed image cards, the comet ribbon arms, and the radial reveal
// wave. Every constant below is lifted from the original GLSL sources.
//
// Original card vertex shader:
//   float d2 = dot(position.xy, position.xy);
//   world.z += uCurve * (0.25 - d2) * 2.0;
//   vec2 dpx = (position.xy - uMouse) * uSize;
//   float rr = max(uSize.x, uSize.y) * 0.6;
//   world.z += uWarp * exp(-dot(dpx, dpx) / (rr * rr));
//
// Original card fragment shader essentials:
//   12-tap Poisson disk with per-pixel rotation → 13-tap average
//   chromatic dispersion: shift = (vUv-0.5)*|vUv-0.5|²*4*uAber
//   4-slot click ripple: uvL += (q/max(1,dq)) * (w*exp(-ring²)) / uSize
//   rounded-rect SDF + 1px frame band: smoothstep(0.5, 1.5, |dEdge+1|)
//   AA: 1 - smoothstep(-0.75, 0.75, dEdge)

struct Camera {
    // projection
    cam_z: f32,
    focal: f32,
    res: vec2<f32>,
    time: f32,
    // reveal wave
    wave_r: f32,
    wave_band: f32,
    // theme + quality
    dark: f32,
    quality: f32,
    // pointer in card-local units, -1..1 over the field
    pointer: vec2<f32>,
    curve: f32,
    warp: f32,
    ripple_w: f32,
    frame_a: f32,
    pulse: f32,
    bg: vec3<f32>,
    ripples: array<vec4<f32>, 4>,
};

@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var tex_arr: texture_2d_array<f32>;
@group(0) @binding(2) var tex_smp: sampler;

// ---------------------------------------------------------------- cards

struct CardIn {
    // (A, B, TX, TY) — wx = A*px + B*py + TX ; wy = m0.w*px + m1.x*py + TY
    @location(0) m0: vec4<f32>,
    // (C, D, Z, unused) — wz = Z
    @location(1) m1: vec4<f32>,
    // (w, h, 0, 0)
    @location(2) size: vec4<f32>,
    // (alpha, blur, aberration, tex_index)
    @location(3) params: vec4<f32>,
};

struct CardOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) v_uv: vec2<f32>,
    @location(1) v_size: vec2<f32>,
    @location(2) v_alpha: f32,
    @location(3) v_blur: f32,
    @location(4) v_aber: f32,
    @location(5) v_wave: f32,
    @location(6) @interpolate(flat) tex_idx: u32,
};

@vertex
fn vs_card(
    @builtin(vertex_index) vi: u32,
    inst: CardIn,
) -> CardOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(-0.5, 0.5),
        vec2<f32>(-0.5, 0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5),
    );
    let p = corners[vi];
    var out: CardOut;

    let wx = inst.m0.x * p.x + inst.m0.y * p.y + inst.m0.z;
    let wy = inst.m0.w * p.x + inst.m1.x * p.y + inst.m1.y;
    let wz = inst.m1.z;

    // The bow: strongest at the card centre, flat at the edges, applied in
    // world space so it reads in px.
    let pn = p * inst.size.xy;
    let d2 = dot(pn, pn) / max(0.0001, inst.size.x * inst.size.y);
    var z = wz + cam.curve * (0.25 - d2) * 2.0 * max(inst.size.x, inst.size.y);

    // The cursor's weight: a local bulge toward the camera at the point of the
    // card nearest the mouse — the surface leans into the hand.
    let dpx = pn - cam.pointer * inst.size.xy;
    let rr = max(inst.size.x, inst.size.y) * 0.6;
    z = z + cam.warp * exp(-dot(dpx, dpx) / max(1.0, rr * rr));

    let depth = max(1.0, cam.cam_z - z);
    let focal = cam.focal / depth;
    let half = cam.res * 0.5;
    out.pos = vec4<f32>(vec2<f32>(wx, wy) * focal / half, 0.0, 1.0);

    out.v_uv = p + vec2<f32>(0.5, 0.5);
    out.v_size = inst.size.xy;
    out.v_alpha = inst.params.x;
    out.v_blur = inst.params.y;
    out.v_aber = inst.params.z;
    out.tex_idx = u32(inst.params.w);
    // Radial reveal wave with a trailing band, as in the ribbon shader.
    let r = length(vec2<f32>(wx, wy));
    out.v_wave = clamp((cam.wave_r - r) / max(1.0, cam.wave_band), 0.0, 1.0);
    return out;
}

struct FragIn {
    @location(0) v_uv: vec2<f32>,
    @location(1) v_size: vec2<f32>,
    @location(2) v_alpha: f32,
    @location(3) v_blur: f32,
    @location(4) v_aber: f32,
    @location(5) v_wave: f32,
    @location(6) @interpolate(flat) tex_idx: u32,
    // `position` is a builtin and must be declared on its own.
    @builtin(position) frag: vec4<f32>,
};

const TAPS = array<vec2<f32>, 12>(
    vec2<f32>(-0.326, -0.406), vec2<f32>(-0.840, -0.074), vec2<f32>(-0.696, 0.457),
    vec2<f32>(-0.203, 0.621), vec2<f32>(0.962, -0.195), vec2<f32>(0.473, -0.480),
    vec2<f32>(0.519, 0.767), vec2<f32>(0.185, -0.893), vec2<f32>(0.507, 0.064),
    vec2<f32>(0.896, 0.412), vec2<f32>(-0.322, -0.933), vec2<f32>(-0.792, -0.598),
);

fn cover_uv(uv: vec2<f32>, size: vec2<f32>, tex_aspect: f32) -> vec2<f32> {
    // Cover-fit crop so a screenshot never stretches.
    let card_aspect = size.x / max(1.0, size.y);
    var u = uv;
    if (card_aspect > tex_aspect) {
        let w = tex_aspect / card_aspect;
        u.x = (u.x - 0.5) * w + 0.5;
    } else {
        let h = card_aspect / tex_aspect;
        u.y = (u.y - 0.5) * h + 0.5;
    }
    return u;
}

@fragment
fn fs_card(built: FragIn) -> @location(0) vec4<f32> {
    let in = built;
    var uv = cover_uv(in.v_uv, in.v_size, 1.6);
    let base_a = in.v_alpha * in.v_wave;
    if (base_a <= 0.002) {
        discard;
    }

    // Local click punches: a soft gaussian ring pushes the texture radially
    // outward from the click point. Stackable across the four slots.
    for (var k = 0; k < 4; k = k + 1) {
        let c = cam.ripples[k];
        if (c.w > 0.001) {
            let q = (in.v_uv - c.xy) * in.v_size;
            let dq = length(q);
            let ring = (dq - c.z) / max(1.0, cam.ripple_w);
            uv = uv + (q / max(1.0, dq)) * (c.w * exp(-ring * ring)) / in.v_size;
        }
    }

    var col: vec3<f32>;
    if (in.v_blur > 0.0008) {
        // Poisson disk with a per-pixel rotation — a cheap wide gaussian
        // stand-in that hides its tap count at nebula sizes.
        let seed = fract(sin(dot(in.frag.xy, vec2<f32>(12.9898, 78.233))) * 43758.545);
        let ang = seed * 6.2831853;
        let rot = mat2x2<f32>(cos(ang), -sin(ang), sin(ang), cos(ang));
        var acc = textureSampleLevel(tex_arr, tex_smp, clamp(uv, vec2<f32>(0.001), vec2<f32>(0.999)), i32(in.tex_idx), 0.0).rgb;
        for (var i = 0; i < 12; i = i + 1) {
            let t = clamp(uv + rot * TAPS[i] * in.v_blur, vec2<f32>(0.001), vec2<f32>(0.999));
            acc = acc + textureSampleLevel(tex_arr, tex_smp, t, i32(in.tex_idx), 0.0).rgb;
        }
        col = acc / 13.0;
    } else {
        // Sharp path: chromatic dispersion growing toward the card edges.
        let d = in.v_uv - vec2<f32>(0.5, 0.5);
        let shift = d * dot(d, d) * 4.0 * in.v_aber;
        let r = textureSampleLevel(tex_arr, tex_smp, clamp(uv + shift, vec2<f32>(0.001), vec2<f32>(0.999)), i32(in.tex_idx), 0.0).r;
        let g = textureSampleLevel(tex_arr, tex_smp, clamp(uv, vec2<f32>(0.001), vec2<f32>(0.999)), i32(in.tex_idx), 0.0).g;
        let b = textureSampleLevel(tex_arr, tex_smp, clamp(uv - shift, vec2<f32>(0.001), vec2<f32>(0.999)), i32(in.tex_idx), 0.0).b;
        col = vec3<f32>(r, g, b);
    }

    col = mix(cam.bg.rgb, col, base_a);

    // Rounded-rect SDF in card px: antialiased corner cut, plus the hairline
    // frame drawn as a ~1px band riding the same edge.
    let q = abs((in.v_uv - vec2<f32>(0.5, 0.5)) * in.v_size) - (0.5 * in.v_size - vec2<f32>(2.0));
    let d_edge = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - 2.0;
    let frame_band = 1.0 - smoothstep(0.5, 1.5, abs(d_edge + 1.0));
    let frame = mix(vec3<f32>(0.533, 0.533, 0.533), vec3<f32>(0.20, 0.20, 0.24), cam.dark);
    col = mix(col, frame, frame_band * cam.frame_a * base_a);

    let aa = 1.0 - smoothstep(-0.75, 0.75, d_edge);
    let out_a = base_a * aa;
    if (out_a <= 0.002) {
        discard;
    }
    return vec4<f32>(col, out_a);
}

// ------------------------------------------------------------- ribbons

struct RibbonIn {
    // origin.xy, direction.xy
    @location(0) od: vec4<f32>,
    // length, arm_index, phase, alpha
    @location(1) params: vec4<f32>,
};

struct RibbonOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) v_side: f32,
    @location(1) v_g: f32,
    @location(2) v_arm: f32,
    @location(3) v_alpha: f32,
};

@vertex
fn vs_ribbon(@builtin(vertex_index) vi: u32, inst: RibbonIn) -> RibbonOut {
    // Two triangles as a unit segment: index → (t along, side across).
    var T = array<f32, 6>(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);
    var S = array<f32, 6>(-1.0, 1.0, -1.0, -1.0, 1.0, 1.0);
    let t = T[vi % 6u];
    let s = S[vi % 6u];

    let origin = inst.od.xy;
    let dir = inst.od.zw;
    let len = inst.params.x;
    let arm = inst.params.y;
    let phase = inst.params.z;

    // Band coordinate: comets travel outward (increasing t), arms dephased by
    // the golden conjugate so they never flash in lockstep. Asymmetric
    // falloff: sharp head, longer tail.
    let d = fract(phase * 12.0 - cam.time * 0.09 + arm * 0.618);
    let sd = d - 0.5;
    let sig = 0.045 * select(1.7, 0.55, sd >= 0.0);
    var g = exp(-0.5 * (sd / sig) * (sd / sig));
    // Fade pulses in over the blurred birth zone.
    let tt = t * len;
    g = g * smoothstep(0.04, 0.14, tt);
    // Intro choreography: pulses are uncovered by the same radial reveal wave
    // that uncovers the cards (trailing band).
    let r = length(origin + dir * tt);
    g = g * clamp((cam.wave_r - r) / max(1.0, cam.wave_band), 0.0, 1.0);
    // Ribbon width: resting hairline plus the pulse swell.
    let w = 1.0 + 2.5 * g;

    let mid = origin + dir * tt;
    let perp = vec2<f32>(-dir.y, dir.x);
    let p = mid + perp * s * w * 0.5;
    let focal = cam.focal / cam.cam_z;
    let half = cam.res * 0.5;

    var out: RibbonOut;
    out.pos = vec4<f32>(p * focal / half, 0.0, 1.0);
    out.v_side = s;
    out.v_g = g;
    out.v_arm = arm;
    out.v_alpha = inst.params.w;
    return out;
}

fn hue_rotate(col: vec3<f32>, ang: f32) -> vec3<f32> {
    let to_yiq = mat3x3<f32>(
        0.299, 0.587, 0.114,
        0.596, -0.274, -0.322,
        0.211, -0.523, 0.312,
    );
    let to_rgb = mat3x3<f32>(
        1.0, 0.956, 0.621,
        1.0, -0.272, -0.647,
        1.0, -1.106, 1.703,
    );
    let yiq = to_yiq * col;
    let c = cos(ang);
    let s = sin(ang);
    let y = yiq.x;
    let i = yiq.y * c - yiq.z * s;
    let q = yiq.y * s + yiq.z * c;
    return to_rgb * vec3<f32>(y, i, q);
}

@fragment
fn fs_ribbon(in: RibbonOut) -> @location(0) vec4<f32> {
    // Soft ribbon edges — the cross-fade doubles as antialiasing.
    let edge = 1.0 - smoothstep(0.45, 1.0, abs(in.v_side));
    let g = clamp(in.v_g, 0.0, 1.0);
    let pulse_col = hue_rotate(vec3<f32>(0.176, 0.506, 1.0), in.v_arm * cam.pulse * 6.2831853);
    let base = mix(vec3<f32>(0.63, 0.63, 0.63), vec3<f32>(0.10, 0.10, 0.12), cam.dark);
    let col = mix(base, pulse_col, g);
    let a = (in.v_alpha * 0.55 + 0.45 * g) * edge;
    if (a <= 0.002) {
        discard;
    }
    return vec4<f32>(col, a);
}

// ----------------------------------------------------------- ui blit

@vertex
fn vs_fullscreen(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(corners[vi], 0.0, 1.0);
}

@group(1) @binding(0) var ui_tex: texture_2d<f32>;
@group(1) @binding(1) var ui_smp: sampler;

@fragment
fn fs_ui(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = pos.xy / cam.res;
    let c = textureSampleLevel(ui_tex, ui_smp, uv, 0.0);
    // The UI canvas carries straight alpha; premultiply on the way out.
    return vec4<f32>(c.rgb * c.a, c.a);
}
