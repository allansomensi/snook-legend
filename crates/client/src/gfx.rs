//! Low-level rendering: the lighting shader, mesh building and textures.
//!
//! Everything targets OpenGL 2.1 / GLES 2.0 (GLSL 100), so the game runs on
//! practically any PC, including old integrated GPUs.

use macroquad::miniquad::{
    self as mq, BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, FilterMode,
    MipmapFilterMode, PipelineParams, TextureWrap, UniformDesc, UniformType,
};
use macroquad::prelude::*;

pub const LAMP_COUNT: usize = 3;

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;

// The texture coordinates tile across walls and floors (well past 1):
// medium precision, not low.
varying mediump vec2 uv;
varying lowp vec4 color;
varying highp vec3 v_normal;
varying highp vec3 v_world;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    vec4 world = Model * vec4(position, 1.0);
    v_world = world.xyz;
    v_normal = (Model * vec4(normal.xyz, 0.0)).xyz;
    uv = texcoord;
    color = color0 / 255.0;
    gl_Position = Projection * world;
}
"#;

const FRAGMENT: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#define HP highp
#else
// Some GLES 2 GPUs have no high precision in fragment shaders: asking for
// it there would not compile.
precision mediump float;
#define HP mediump
#endif

varying mediump vec2 uv;
varying lowp vec4 color;
varying HP vec3 v_normal;
varying HP vec3 v_world;

uniform sampler2D Texture;
uniform HP vec3 CameraPos;
uniform HP vec3 Lamp0;
uniform HP vec3 Lamp1;
uniform HP vec3 Lamp2;
uniform vec4 LampColor;      // rgb, w = intensity
uniform vec4 Ambient;        // rgb, w = number of lamps (1 or 3)
uniform vec4 Surface;        // x = specular, y = shininess, z = unlit, w = rim
uniform vec4 Extra;          // x = clear coat (glossy reflection of the room), y = cut-outs,
                             // z = haze (unlit: fades where seen edge on),
                             // w = 1 the room (see Clear), 2 a ball, 3 a cue
uniform vec4 Shadows;        // x = soft shadows on, y = occlusion on, z = cloth height,
                             // w = reflection pass (fade with height)
uniform vec4 Balls[16];      // xyz = centre, w = radius (0 = empty slot)
uniform vec4 Stage;          // x = focus: the room fades into darkness away from the table,
                             // yzw = the hall's colour in reflections
uniform vec4 Holes[6];       // pocket holes: x, z (world), radius
uniform vec4 Throats[12];    // pocket throats: 4 corners (x, z) per pocket
uniform vec4 CueA;           // the cue's tip: xyz, w = radius (0: no cue)
uniform vec4 CueB;           // the cue's butt: xyz, w = radius
uniform vec4 Spot;           // xy = half size of the pool of light on the table,
                             // z = the angle a lamp's glint on a ball must span, per
                             // metre from the camera, to cover a pixel or two
uniform vec4 Below;          // rgb = what glossy things mirror below them (the cloth, a floor)
uniform vec4 Clear;          // the room is left out of this box over the table when the
                             // camera looks down on it: xy = half size, z = bottom, w = top
                             // (w <= z: off). Extra.w marks the room's surfaces.

// The pocket cut-outs. Extra.y selects what a surface loses:
// 1 = the holes (cloth), 2 = holes and throats (rails, cushions),
// 3 = the throat side of a pocket wall above the cloth (the well),
// 4 = the holes, on upward faces only (the table body under the cloth).
bool in_quad(vec2 p, vec2 a, vec2 b, vec2 c, vec2 d) {
    vec2 e0 = b - a; vec2 e1 = c - b; vec2 e2 = d - c; vec2 e3 = a - d;
    float s0 = e0.x * (p.y - a.y) - e0.y * (p.x - a.x);
    float s1 = e1.x * (p.y - b.y) - e1.y * (p.x - b.x);
    float s2 = e2.x * (p.y - c.y) - e2.y * (p.x - c.x);
    float s3 = e3.x * (p.y - d.y) - e3.y * (p.x - d.x);
    return (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0)
        || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0);
}

// How far the cloth has dropped into the nearest pocket (0 flat … 1 at the
// hole's edge): the cloth rolls over the edge, so it darkens as it goes.
float pocket_drop(vec3 p) {
    float k = 0.0;
    for (int i = 0; i < 6; i++) {
        vec4 h = Holes[i];
        float d = length(p.xz - h.xy) - h.z;
        k = max(k, 1.0 - smoothstep(0.0, 0.028, d));
    }
    return k;
}

bool cut_away(vec3 p, float mode, bool n_up) {
    for (int i = 0; i < 6; i++) {
        vec4 h = Holes[i];
        vec2 d = p.xz - h.xy;
        bool hole = dot(d, d) < h.z * h.z;
        vec4 q0 = Throats[2 * i];
        vec4 q1 = Throats[2 * i + 1];
        bool throat = in_quad(p.xz, q0.xy, q0.zw, q1.xy, q1.zw);
        if (mode < 1.5) {
            if (hole) return true;
        } else if (mode < 2.5) {
            if (hole || throat) return true;
            // Nothing of the rail (the leather lip around a side pocket)
            // reaches in front of the cushion line, over the cloth.
            if (h.w > 0.0 && abs(p.z) < h.w - 0.008 && dot(d, d) < (h.z + 0.03) * (h.z + 0.03)) return true;
        } else if (mode < 3.5) {
            if (throat && p.y > Shadows.z + 0.0005) return true;
        } else {
            if (hole && n_up) return true;
        }
    }
    return false;
}

// How much of one lamp's light reaches `p` past one ball (soft sphere shadow:
// the penumbra grows with distance, like under a shade about 8 cm across).
float sphere_shadow(vec3 oc, float oc2, vec3 ld, float rad) {
    float ll = length(ld);
    ld /= ll;
    float t = dot(oc, ld);
    if (t <= 0.0) {
        return 1.0;
    }
    float d = sqrt(max(oc2 - t * t, 0.0));
    float pen = 0.045 * t / ll + 0.0015;
    // Soft and light: a shadow says where a ball is without hiding the
    // cloth (or a ball) under it.
    return mix(0.45, 1.0, smoothstep(rad - pen, rad + pen, d));
}

// How much of one lamp's light reaches `p` past the cue: a rod tapering
// from CueA to CueB. The closest points of the ray towards the lamp and the
// rod's axis give the distance; the penumbra widens with the distance to
// the cue, like the balls' shadows.
float cue_shadow(vec3 p, vec3 lamp) {
    vec3 d1 = lamp - p;
    vec3 d2 = CueB.xyz - CueA.xyz;
    vec3 r = p - CueA.xyz;
    float a = dot(d1, d1);
    float e = dot(d2, d2);
    float f = dot(d2, r);
    float c = dot(d1, r);
    float b = dot(d1, d2);
    float denom = a * e - b * b;
    float s = denom > 1e-9 ? clamp((b * f - c * e) / denom, 0.0, 1.0) : 0.0;
    float t = (b * s + f) / e;
    if (t < 0.0) {
        t = 0.0;
        s = clamp(-c / a, 0.0, 1.0);
    } else if (t > 1.0) {
        t = 1.0;
        s = clamp((b - c) / a, 0.0, 1.0);
    }
    if (s <= 0.0) {
        return 1.0;
    }
    float dist = length(p + d1 * s - (CueA.xyz + d2 * t));
    float rad = mix(CueA.w, CueB.w, t);
    float pen = 0.045 * s + 0.0015;
    return mix(0.6, 1.0, smoothstep(rad - pen, rad + pen, dist));
}

// Ball shadows for the three lamps (xyz) and contact occlusion (w) at `p`.
// One pass over the balls; balls more than ~22 cm away are skipped early,
// and the list ends at the first empty slot.
vec4 ball_terms(vec3 p, vec3 n) {
    vec4 r = vec4(1.0);
#ifdef NO_SHADOWS
    return r;
#else
    if (Shadows.x < 0.5 || abs(p.y - Shadows.z) > 0.12) {
        return r;
    }
    float occ = 0.0;
    bool three = Ambient.w > 1.5;
    for (int i = 0; i < 16; i++) {
        vec4 b = Balls[i];
        if (b.w <= 0.0) {
            break;
        }
        vec3 oc = b.xyz - p;
        if (dot(oc.xz, oc.xz) > 0.05) {
            continue;
        }
        float oc2 = dot(oc, oc);
        if (oc2 < b.w * b.w * 1.005) {
            continue;
        }
        if (Shadows.y > 0.5) {
            occ += max(dot(n, oc), 0.0) * inversesqrt(oc2) * (b.w * b.w) / oc2;
        }
        r.y *= sphere_shadow(oc, oc2, Lamp1 - p, b.w);
        if (three) {
            r.x *= sphere_shadow(oc, oc2, Lamp0 - p, b.w);
            r.z *= sphere_shadow(oc, oc2, Lamp2 - p, b.w);
        }
    }
    r.w = clamp(1.0 - occ * 0.9, 0.5, 1.0);
    // The cue (not on itself: points on or next to its surface are skipped).
    if (CueA.w > 0.0) {
        vec3 ab = CueB.xyz - CueA.xyz;
        float k = clamp(dot(p - CueA.xyz, ab) / dot(ab, ab), 0.0, 1.0);
        if (length(p - CueA.xyz - ab * k) > mix(CueA.w, CueB.w, k) * 1.25) {
            r.y *= cue_shadow(p, Lamp1);
            if (three) {
                r.x *= cue_shadow(p, Lamp0);
                r.z *= cue_shadow(p, Lamp2);
            }
        }
    }
    return r;
#endif
}

// What a glossy surface reflects: the dark bar, the green cloth below and the
// bright lamps above the table. Cheap and analytic, no textures needed.
vec3 lamp_reflection(vec3 pos, vec3 r) {
    vec3 l = normalize(pos - v_world);
    float d = dot(r, l);
    // A lamp seen in a polished surface: a small, crisp glint in a faint,
    // wide sheen.
    return LampColor.rgb * LampColor.w * (smoothstep(0.9935, 0.9985, d) * 1.5 + smoothstep(0.95, 0.997, d) * 0.03);
}

// A lamp seen in a ball: a crisp dot of light. Seen from far away it would
// be smaller than a pixel (a speck that flickers as the ball rolls and
// reads as a scratch): it is never drawn smaller than a clean dot.
float ball_glint(vec3 pos, vec3 r) {
    vec3 l = normalize(pos - v_world);
    // 1 - cos of the angle between the reflection and the lamp (about half
    // the angle squared).
    float e = 1.0 - dot(r, l);
    float least = Spot.z * length(CameraPos - v_world);
    float edge = max(0.0065, 0.5 * least * least);
    // A soft patch of light in a faint glow, not a hard white dot: the
    // dot sat on the numbers and hid them.
    float glow = 1.0 - smoothstep(edge, edge * 9.0, e);
    float core = 1.0 - smoothstep(edge * 0.2, edge * 2.6, e);
    return 0.55 * core * core + 0.20 * glow * glow;
}

vec3 environment(vec3 r) {
    // The hall's own colour (warm wood, a sunset, a cold loft…).
    vec3 room = Stage.yzw;
    vec3 c = mix(room, Below.rgb, smoothstep(-0.05, -0.45, r.y));
    c += vec3(0.05, 0.035, 0.02) * smoothstep(0.2, 0.9, r.y);
    c += lamp_reflection(Lamp1, r);
    if (Ambient.w > 1.5) {
        c += lamp_reflection(Lamp0, r);
        c += lamp_reflection(Lamp2, r);
    }
    return c;
}

// The highlight of what is being lit: its strength and sharpness, and its
// colour (white, or a metal's own). Set in main from Surface (a cue's
// texture changes them part by part).
float g_spec;
float g_shine;
vec3 g_tint;

vec3 lamp_at(vec3 pos, vec3 at, vec3 n, vec3 v, vec3 base, float shadow) {
    vec3 l = pos - at;
    float d = length(l);
    l /= d;
    // Downward cone of a billiard lamp.
    float cone = smoothstep(0.25, 0.75, l.y);
    float att = LampColor.w / (1.0 + 0.9 * d * d);
    float diff = max(dot(n, l), 0.0);
    vec3 h = normalize(l + v);
    float spec = pow(max(dot(n, h), 0.0), g_shine) * g_spec;
    return LampColor.rgb * (diff * base + spec * g_tint) * att * cone * shadow;
}

vec3 lamp(vec3 pos, vec3 n, vec3 v, vec3 base, float shadow) {
    return lamp_at(pos, v_world, n, v, base, shadow);
}

// A lamp's glint on a glossy surface whose light is baked (the halls):
// only the highlight, the diffuse light is already in the colour.
vec3 glint(vec3 pos, vec3 n, vec3 v) {
    vec3 l = pos - v_world;
    float d = length(l);
    l /= d;
    float cone = smoothstep(0.25, 0.75, l.y);
    vec3 h = normalize(l + v);
    float spec = pow(max(dot(n, h), 0.0), Surface.y) * Surface.x;
    return LampColor.rgb * spec * LampColor.w / (1.0 + 0.9 * d * d) * cone;
}

// The reflection pass (a glossy floor): what is seen in the floor fades
// with its height above it, like a real polished floor.
vec3 mirror_fade(vec3 c) {
    if (Shadows.w > 0.0) {
        c *= exp(-max(v_world.y, 0.0) * Shadows.w);
    }
    return c;
}

void main() {
    if (Extra.y > 0.5 && cut_away(v_world, Extra.y, v_normal.y > 0.5)) {
        discard;
    }
    // Nothing of the room (a lamp's cord, a fan, a beam) between a camera
    // looking down and the table.
    if (Extra.w > 0.5 && Extra.w < 1.5 && Clear.w > Clear.z && abs(v_world.x) < Clear.x
        && abs(v_world.z) < Clear.y && v_world.y > Clear.z && v_world.y < Clear.w) {
        discard;
    }
    // The balls are read from a sharper level of their texture: a ball a
    // few dozen pixels across keeps a crisp number instead of a smear,
    // anti-aliasing or not. (Only the balls: wood grain, floors and rugs
    // read sharper than their size on screen shimmer as the camera moves.)
    // Nothing of the room right against the lens either: a beam, a duct or
    // a lamp the camera has moved into would black out the whole view.
    if (Extra.w > 0.5 && Extra.w < 1.5 && Shadows.w <= 0.0) {
        vec3 near = v_world - CameraPos;
        if (dot(near, near) < 0.09) {
            discard;
        }
    }
    bool ball = Extra.w > 1.5 && Extra.w < 2.5;
    vec4 base = texture2D(Texture, uv, ball ? -1.0 : 0.0) * color;
    if (Surface.z > 0.5) {
        // Unlit: the light is baked into the colour (the halls) or the
        // surface shines by itself.
        vec3 c = base.rgb;
        float alpha = base.a;
        if (Extra.z > 0.0 || Surface.x > 0.0 || Extra.x > 0.0) {
            vec3 n = normalize(v_normal);
            vec3 v = normalize(CameraPos - v_world);
            // Haze (light cones, shafts): see-through where it is seen
            // edge on, so it has no hard outline.
            if (Extra.z > 0.0) {
                alpha *= pow(abs(dot(n, v)), Extra.z);
            }
            // Baked gloss: varnish, brass, glass and polished floors catch
            // the lamps and the room as the camera moves.
            if (Surface.x > 0.0 || Extra.x > 0.0) {
                if (dot(n, v) < 0.0) {
                    n = -n;
                }
                c += glint(Lamp1, n, v);
                if (Ambient.w > 1.5) {
                    c += glint(Lamp0, n, v) + glint(Lamp2, n, v);
                }
                float ndv = max(dot(n, v), 0.0);
                float fresnel = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
                c += environment(reflect(-v, n)) * Extra.x * (0.03 + 0.6 * fresnel);
            }
        }
        gl_FragColor = vec4(mirror_fade(c), alpha);
        return;
    }
    vec3 n = normalize(v_normal);
    vec3 v = normalize(CameraPos - v_world);
    g_spec = Surface.x;
    g_shine = Surface.y;
    g_tint = vec3(1.0);
    // A cue (Extra.w = 3): the texture's alpha says what each part is made
    // of: 0 polished metal, 0.33 matte (leather, linen, rubber), 0.55 satin
    // (an oiled shaft), 0.75 lacquer, 1 pearl (lacquer whose colours shift
    // with the angle).
    float metal = 0.0;
    float gloss = 1.0;
    float pearl = 0.0;
    if (Extra.w > 2.5) {
        float a = base.a;
        metal = 1.0 - smoothstep(0.08, 0.25, a);
        gloss = max(smoothstep(0.38, 0.72, a), metal);
        pearl = smoothstep(0.85, 0.97, a);
        g_spec = Surface.x * mix(0.05, 1.0, gloss) * (1.0 + 2.0 * metal);
        g_shine = mix(9.0, Surface.y, gloss) * (1.0 + 0.6 * metal);
        g_tint = mix(vec3(1.0), base.rgb * 1.3 + 0.04, metal);
        base = vec4(base.rgb * (1.0 - 0.85 * metal), 1.0);
    }
#ifdef DEBUG_NORMALS
    // Development check: faces whose normal points away from the viewer
    // (they are lit from behind and look dark) show up red.
    if (dot(n, v) < -0.02) {
        gl_FragColor = vec4(1.0, 0.0, 0.0, 1.0);
        return;
    }
#endif
    // Hemisphere ambient: a little brighter from above, a green bounce from the cloth below.
    float hemi = 0.5 + 0.5 * n.y;
    vec3 amb = Ambient.rgb * (0.55 + 0.6 * hemi) + vec3(-0.02, 0.05, 0.01) * max(-n.y, 0.0);
    vec4 bt = ball_terms(v_world, n);
    // "Focused" stage: the room falls off into darkness around the table.
    float away = smoothstep(1.3, 3.2, length(v_world.xz));
    amb *= mix(1.0, 0.12, away * Stage.x);
    vec3 c = amb * base.rgb * bt.w;
    // A pool of light that covers the whole table evenly and only fades
    // beyond the rails.
    float spot = length(v_world.xz / Spot.xy);
    float pool = mix(1.0, mix(1.0, 0.42, smoothstep(0.85, 1.6, spot)), Stage.x);
    // The cloth and the rails' tops are lit as under a wide fixture: the
    // light barely falls off from the middle to the cushions, so every
    // ball reads the same wherever it is. Only the table's own surfaces
    // (the ones the pockets are cut out of): on anything round standing on
    // the cloth this drew a pale disc with a hard edge, the cap of a ball
    // lit from another place than the rest of it.
    vec3 at = v_world;
    if (Extra.y > 0.5 && Extra.y < 2.5 && n.y > 0.9 && abs(v_world.y - Shadows.z) < 0.08) {
        at = vec3(v_world.x * 0.3, v_world.y, v_world.z * 0.3);
    }
    if (Ambient.w > 1.5) {
        c += lamp_at(Lamp1, at, n, v, base.rgb, bt.y);
        c += lamp_at(Lamp0, at, n, v, base.rgb, bt.x);
        c += lamp_at(Lamp2, at, n, v, base.rgb, bt.z);
    } else {
        // One lamp standing in for the three (Low quality): it shines from
        // the nearest point of the row they hang in, as bright as all of
        // them. (Left in the middle, what faced it from the side, the end
        // of a casting or a pocket's facing, took the light of three lamps
        // and burnt out to white.)
        vec3 row = vec3(clamp(at.x, min(Lamp0.x, Lamp2.x), max(Lamp0.x, Lamp2.x)), Lamp1.yz);
        c += lamp_at(row, at, n, v, base.rgb, bt.y);
    }
    c *= pool;
    // A little rim light keeps dark objects readable.
    float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * Surface.w;
    c += rim * vec3(0.35, 0.3, 0.25);
    if (Extra.x > 0.0) {
        float ndv = max(dot(n, v), 0.0);
        float fresnel = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
        vec3 r = reflect(-v, n);
        vec3 env = environment(r);
        if (Extra.w > 2.5) {
            // The clear coat over what is polished (none on the matte
            // parts); metal mirrors the room in its own colour, brighter
            // towards the lamps above it.
            c += env * Extra.x * (0.05 + 0.75 * fresnel) * gloss * (1.0 - metal);
            // (A broad band of light where it faces up, as under a
            // studio's softbox or a table's lamps: polished metal reads by
            // its contrast.)
            float band = smoothstep(0.3, 0.92, r.y);
            vec3 sky = env + LampColor.rgb * LampColor.w * (0.03 + 0.32 * band);
            c += sky * g_tint * metal * (0.6 + 0.4 * fresnel);
            // Mother-of-pearl: colours shifting with the angle.
            c += pearl * 0.1 * (0.5 + 0.5 * cos(6.2832 * (ndv * 1.3 + vec3(0.0, 0.33, 0.67))));
        } else {
            c += env * Extra.x * (0.05 + 0.75 * fresnel);
        }
    }
    if (ball) {
        // The light over the table, mirrored in the ball's polish: one
        // clean dot (the three lamps side by side made a ragged bar across
        // a small ball).
        c += LampColor.rgb * ball_glint(Lamp1, reflect(-v, n)) * 0.6;
    }
    // The cloth darkens where it rolls over into a pocket.
    if (Extra.y > 0.5 && Extra.y < 1.5) {
        float drop = pocket_drop(v_world);
        c *= 1.0 - 0.82 * drop * drop;
    }
    // The cloth and the cushions under the table's lamps: a deep, saturated
    // colour (lit as hard as the balls it would wash out to a pale pastel),
    // brightest in the middle and a little darker towards the cushions.
    // The balls keep all their light, so they stand out and their numbers
    // read at a glance.
    if (Extra.y > 0.5 && Extra.y < 2.5) {
        // (A dark cloth keeps most of its light: it has none to spare.)
        float luma = dot(c, vec3(0.3, 0.6, 0.1));
        float deep = mix(0.9, 0.54, smoothstep(0.08, 0.7, luma));
        c *= deep * mix(1.0, 0.76, smoothstep(0.15, 1.0, spot));
    }
    // Filmic-style exposure curve: highlights (white balls under the lamps)
    // roll off smoothly instead of burning out to flat white.
    c = vec3(1.0) - exp(-c * 1.45);
    gl_FragColor = vec4(mirror_fade(c), base.a);
}
"#;

/// How a surface reacts to light.
#[derive(Clone, Copy)]
pub struct Surface {
    pub specular: f32,
    pub shininess: f32,
    pub unlit: bool,
    pub rim: f32,
    /// Glossy clear-coat reflection (balls, varnish, glass).
    pub coat: f32,
    /// Pocket cut-out mode (see `cut_away` in the shader); 0 = none.
    pub cut: f32,
    /// Unlit haze (light cones, shafts): fades where seen edge on (the
    /// power of the fade; 0 = none).
    pub haze: f32,
    /// A ball: its lamp glints never get smaller than a clean dot and its
    /// texture is read sharp (see the shader).
    pub ball: bool,
    /// A cue: its texture's alpha says what each part is made of (see
    /// `cue_skin::Finish` and the shader).
    pub cue: bool,
}

impl Surface {
    pub const MATTE: Surface = Surface {
        specular: 0.04,
        shininess: 8.0,
        unlit: false,
        rim: 0.0,
        coat: 0.0,
        cut: 0.0,
        haze: 0.0,
        ball: false,
        cue: false,
    };
    pub const WOOD: Surface = Surface {
        specular: 0.35,
        shininess: 40.0,
        unlit: false,
        rim: 0.05,
        coat: 0.35,
        cut: 0.0,
        haze: 0.0,
        ball: false,
        cue: false,
    };
    /// Polished phenolic balls, as a tournament set looks under the lamps:
    /// each lamp is a small, crisp glint and the room a faint reflection,
    /// so the colours and the numbers always read.
    pub const BALL: Surface = Surface {
        specular: 0.07,
        shininess: 38.0,
        unlit: false,
        rim: 0.02,
        coat: 0.11,
        cut: 0.0,
        haze: 0.0,
        ball: true,
        cue: false,
    };
    pub const UNLIT: Surface = Surface {
        specular: 0.0,
        shininess: 1.0,
        unlit: true,
        rim: 0.0,
        coat: 0.0,
        cut: 0.0,
        haze: 0.0,
        ball: false,
        cue: false,
    };
    /// A cue: lacquer's gloss for its polished parts, which its texture
    /// turns down to satin and matte or up to metal and pearl part by part.
    pub const CUE: Surface = Surface {
        specular: 0.55,
        shininess: 90.0,
        unlit: false,
        rim: 0.04,
        coat: 0.42,
        cut: 0.0,
        haze: 0.0,
        ball: false,
        cue: true,
    };
    pub const fn lit(specular: f32, shininess: f32, rim: f32, coat: f32) -> Surface {
        Surface {
            specular,
            shininess,
            unlit: false,
            rim,
            coat,
            cut: 0.0,
            haze: 0.0,
            ball: false,
            cue: false,
        }
    }

    /// A surface whose light is baked into its colours (the halls) but which
    /// still catches the lamps (`specular`, `shininess`) and the room
    /// (`coat`) as the camera moves: varnish, brass, glass, polished floors.
    pub const fn baked(specular: f32, shininess: f32, coat: f32) -> Surface {
        Surface {
            specular,
            shininess,
            unlit: true,
            rim: 0.0,
            coat,
            cut: 0.0,
            haze: 0.0,
            ball: false,
            cue: false,
        }
    }

    /// See-through light (a cone under a lamp, a shaft from a window) that
    /// fades out where it is seen edge on.
    pub const fn haze(power: f32) -> Surface {
        Surface {
            specular: 0.0,
            shininess: 1.0,
            unlit: true,
            rim: 0.0,
            coat: 0.0,
            cut: 0.0,
            haze: power,
            ball: false,
            cue: false,
        }
    }

    /// The same surface with pocket cut-outs (see the shader's `cut_away`).
    pub const fn cut(mut self, mode: f32) -> Surface {
        self.cut = mode;
        self
    }
}

/// The values of the shader's uniforms, in the order they are declared in
/// [`Raw::new`] (tightly packed floats, as miniquad uploads them).
#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    projection: [f32; 16],
    model: [f32; 16],
    camera_pos: [f32; 3],
    lamp0: [f32; 3],
    lamp1: [f32; 3],
    lamp2: [f32; 3],
    lamp_color: [f32; 4],
    ambient: [f32; 4],
    surface: [f32; 4],
    extra: [f32; 4],
    shadows: [f32; 4],
    balls: [[f32; 4]; 16],
    stage: [f32; 4],
    holes: [[f32; 4]; 6],
    throats: [[f32; 4]; 12],
    cue_a: [f32; 4],
    cue_b: [f32; 4],
    spot: [f32; 4],
    below: [f32; 4],
    clear: [f32; 4],
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            projection: Mat4::IDENTITY.to_cols_array(),
            model: Mat4::IDENTITY.to_cols_array(),
            camera_pos: [0.0; 3],
            lamp0: [0.0; 3],
            lamp1: [0.0; 3],
            lamp2: [0.0; 3],
            lamp_color: [0.0; 4],
            ambient: [0.0; 4],
            surface: [0.0; 4],
            extra: [0.0; 4],
            shadows: [0.0; 4],
            balls: [[0.0; 4]; 16],
            stage: [0.0; 4],
            holes: [[0.0; 4]; 6],
            throats: [[0.0; 4]; 12],
            cue_a: [0.0; 4],
            cue_b: [0.0; 4],
            spot: [1.25, 0.72, 0.0, 0.0],
            below: [0.03, 0.17, 0.08, 0.0],
            clear: [0.0; 4],
        }
    }
}

/// The scene's own uniforms (macroquad adds Projection and Model itself).
fn uniform_descs() -> Vec<UniformDesc> {
    vec![
        UniformDesc::new("CameraPos", UniformType::Float3),
        UniformDesc::new("Lamp0", UniformType::Float3),
        UniformDesc::new("Lamp1", UniformType::Float3),
        UniformDesc::new("Lamp2", UniformType::Float3),
        UniformDesc::new("LampColor", UniformType::Float4),
        UniformDesc::new("Ambient", UniformType::Float4),
        UniformDesc::new("Surface", UniformType::Float4),
        UniformDesc::new("Extra", UniformType::Float4),
        UniformDesc::new("Shadows", UniformType::Float4),
        UniformDesc::new("Balls", UniformType::Float4).array(16),
        UniformDesc::new("Stage", UniformType::Float4),
        UniformDesc::new("Holes", UniformType::Float4).array(6),
        UniformDesc::new("Throats", UniformType::Float4).array(12),
        UniformDesc::new("CueA", UniformType::Float4),
        UniformDesc::new("CueB", UniformType::Float4),
        UniformDesc::new("Spot", UniformType::Float4),
        UniformDesc::new("Below", UniformType::Float4),
        UniformDesc::new("Clear", UniformType::Float4),
    ]
}

/// The same shader as the materials, driven straight through miniquad, for
/// meshes whose vertices live on the GPU (see [`Model`]): the halls and the
/// table are drawn every frame without sending their vertices again.
struct Raw {
    lit: mq::Pipeline,
    blended: mq::Pipeline,
    glow: mq::Pipeline,
}

impl Raw {
    fn new(
        fragment: &str,
        lit: PipelineParams,
        blended: PipelineParams,
        glow: PipelineParams,
    ) -> Option<Raw> {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let mut uniforms = vec![
            UniformDesc::new("Projection", UniformType::Mat4),
            UniformDesc::new("Model", UniformType::Mat4),
        ];
        uniforms.extend(uniform_descs());
        let meta = mq::ShaderMeta {
            images: vec!["Texture".to_string()],
            uniforms: mq::UniformBlockLayout { uniforms },
        };
        let shader = ctx
            .new_shader(mq::ShaderSource::Glsl { vertex: VERTEX, fragment }, meta)
            .map_err(|e| eprintln!("static meshes off (shader): {e:?}"))
            .ok()?;
        let layout = [mq::BufferLayout::default()];
        let attributes = [
            mq::VertexAttribute::new("position", mq::VertexFormat::Float3),
            mq::VertexAttribute::new("texcoord", mq::VertexFormat::Float2),
            mq::VertexAttribute::new("color0", mq::VertexFormat::Byte4),
            mq::VertexAttribute::new("normal", mq::VertexFormat::Float4),
        ];
        Some(Raw {
            lit: ctx.new_pipeline(&layout, &attributes, shader, lit),
            blended: ctx.new_pipeline(&layout, &attributes, shader, blended),
            glow: ctx.new_pipeline(&layout, &attributes, shader, glow),
        })
    }
}

/// The shared lit material plus a blended variant for decals and shadows,
/// and one for light in the air (see [`Materials::use_glow`]).
///
/// The uniforms are kept here too: meshes on the GPU are drawn with the same
/// shader straight through miniquad (see [`Materials::draw`]), the small
/// meshes made every frame (guides, shadows) through macroquad's materials.
pub struct Materials {
    pub lit: Material,
    pub blended: Material,
    glow: Material,
    raw: Option<Raw>,
    state: std::cell::RefCell<Uniforms>,
    blending: std::cell::Cell<Blending>,
    /// What is drawn now is the room (see [`Materials::set_room`]).
    room: std::cell::Cell<bool>,
    /// The picture drawn into instead of the window (see
    /// [`Materials::set_target`]): its pass and the size drawn on.
    target: std::cell::Cell<Option<(mq::RenderPass, i32, i32)>>,
}

impl Materials {
    pub fn new() -> Self {
        let uniforms = uniform_descs();
        // Some old drivers reject the shadow loop; fall back to a shader
        // without it rather than failing to start.
        static SIMPLE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let simple: &'static str = SIMPLE.get_or_init(|| {
            FRAGMENT.replacen("#version 100\n", "#version 100\n#define NO_SHADOWS\n", 1)
        });
        static DEBUG: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let debug: &'static str = DEBUG.get_or_init(|| {
            FRAGMENT.replacen("#version 100\n", "#version 100\n#define DEBUG_NORMALS\n", 1)
        });
        // SNOOK_LEGEND_SAFE_GRAPHICS=1 forces the basic shader (for troubleshooting).
        let safe = crate::dev::flag("SNOOK_LEGEND_SAFE_GRAPHICS");
        let first = if safe {
            simple
        } else if crate::dev::flag("SNOOK_LEGEND_DEBUG_NORMALS") {
            debug
        } else {
            FRAGMENT
        };
        let mut used = first;
        let mut load = |vertex: &'static str, pipeline_params: PipelineParams| -> Material {
            let params =
                || MaterialParams { pipeline_params, uniforms: uniforms.clone(), textures: vec![] };
            match load_material(ShaderSource::Glsl { vertex, fragment: first }, params()) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("shader fallback (no ball shadows): {e:?}");
                    used = simple;
                    load_material(ShaderSource::Glsl { vertex, fragment: simple }, params())
                        .expect("basic shader compiles")
                }
            }
        };
        let lit_params = PipelineParams {
            depth_test: Comparison::LessOrEqual,
            depth_write: true,
            cull_face: CullFace::Nothing,
            ..Default::default()
        };
        let blended_params = PipelineParams {
            // Decals and shadows write depth too (they lie flat on
            // surfaces, and what is drawn later must not show through).
            depth_test: Comparison::LessOrEqual,
            depth_write: true,
            cull_face: CullFace::Nothing,
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            ..Default::default()
        };
        // Light in the air (haze, halos): hidden by what is solid, never
        // hiding anything itself (one doesn't cut the other).
        let glow_params = PipelineParams { depth_write: false, ..blended_params };
        let lit = load(VERTEX, lit_params);
        let blended = load(VERTEX, blended_params);
        let glow = load(VERTEX, glow_params);
        // The safe mode keeps everything on macroquad's own path.
        let raw = if safe { None } else { Raw::new(used, lit_params, blended_params, glow_params) };
        Materials {
            lit,
            blended,
            glow,
            raw,
            state: Default::default(),
            blending: std::cell::Cell::new(Blending::Off),
            room: std::cell::Cell::new(false),
            target: std::cell::Cell::new(None),
        }
    }

    /// Draws the meshes on the GPU into `cam`'s picture from now on (none:
    /// the window), as macroquad does the rest once the camera is set.
    pub fn set_target(&self, cam: Option<&TargetCamera>) {
        self.target.set(cam.map(|c| (c.pass.raw_miniquad_id(), c.size.0 as i32, c.size.1 as i32)));
    }

    /// Sets a uniform on both materials and in the state of GPU meshes.
    fn set<T>(&self, name: &str, value: T, write: impl FnOnce(&mut Uniforms))
    where
        T: Copy,
    {
        self.lit.set_uniform(name, value);
        self.blended.set_uniform(name, value);
        self.glow.set_uniform(name, value);
        write(&mut self.state.borrow_mut());
    }

    /// The camera's view and projection (for the meshes on the GPU).
    pub fn set_view(&self, matrix: Mat4) {
        self.state.borrow_mut().projection = matrix.to_cols_array();
    }

    /// Sets the per-frame lighting uniforms on both materials.
    /// `lamp_count` is 1 (cheap) or 3; a single lamp is brighter to compensate.
    /// `tint`: the hall's colour, seen in reflections on balls and varnish.
    #[allow(clippy::too_many_arguments)]
    pub fn set_frame(
        &self,
        camera: Vec3,
        lamps: [Vec3; LAMP_COUNT],
        intensity: f32,
        lamp_count: u32,
        focus: f32,
        tint: Vec3,
    ) {
        let intensity = if lamp_count == 1 { intensity * 2.3 } else { intensity };
        self.set_camera_pos(camera);
        self.set("Lamp0", lamps[0], |u| u.lamp0 = lamps[0].to_array());
        self.set("Lamp1", lamps[1], |u| u.lamp1 = lamps[1].to_array());
        self.set("Lamp2", lamps[2], |u| u.lamp2 = lamps[2].to_array());
        let color = vec4(1.0, 0.88, 0.70, intensity);
        self.set("LampColor", color, |u| u.lamp_color = color.to_array());
        let ambient = vec4(0.30, 0.25, 0.21, lamp_count as f32);
        self.set("Ambient", ambient, |u| u.ambient = ambient.to_array());
        let stage = vec4(focus, tint.x, tint.y, tint.z);
        self.set("Stage", stage, |u| u.stage = stage.to_array());
    }

    /// Moves the viewpoint only (the reflection pass sees from under the floor).
    pub fn set_camera_pos(&self, camera: Vec3) {
        self.set("CameraPos", camera, |u| u.camera_pos = camera.to_array());
    }

    /// Turns the analytic ball shadows / contact occlusion on or off and sets
    /// the balls that cast them.
    /// `cue`: the cue's tip and butt (centre and radius), when it is shown.
    pub fn set_shadows(
        &self,
        soft_shadows: bool,
        occlusion: bool,
        balls: &[Vec4; 16],
        cue: Option<(Vec4, Vec4)>,
    ) {
        let (cue_a, cue_b) = cue.filter(|_| soft_shadows).unwrap_or((Vec4::ZERO, Vec4::ZERO));
        self.set("CueA", cue_a, |u| u.cue_a = cue_a.to_array());
        self.set("CueB", cue_b, |u| u.cue_b = cue_b.to_array());
        let shadows = vec4(
            soft_shadows as u8 as f32,
            occlusion as u8 as f32,
            crate::coords::TABLE_HEIGHT,
            0.0,
        );
        self.set("Shadows", shadows, |u| u.shadows = shadows.to_array());
        for m in [&self.lit, &self.blended, &self.glow] {
            m.set_uniform_array("Balls", &balls[..]);
        }
        let mut u = self.state.borrow_mut();
        for (slot, b) in u.balls.iter_mut().zip(balls) {
            *slot = b.to_array();
        }
    }

    /// The reflection pass: `fade` > 0 is how fast what the floor reflects
    /// fades with its height above it; 0 is the normal pass.
    pub fn set_mirror(&self, fade: f32) {
        let mut shadows = self.state.borrow().shadows;
        shadows[3] = fade;
        let v = Vec4::from_array(shadows);
        self.set("Shadows", v, |u| u.shadows = shadows);
    }

    pub fn use_lit(&self, s: Surface) {
        set_surface(&self.lit, s, self.room.get());
        self.set_state_surface(s);
        self.blending.set(Blending::Off);
        gl_use_material(&self.lit);
    }

    /// Marks what is drawn next as the room (left out of the box over the
    /// table, see [`Materials::set_clear`]) or not.
    pub fn set_room(&self, room: bool) {
        self.room.set(room);
    }

    /// What glossy surfaces mirror below them: the cloth at the table, the
    /// stage's floor in the showroom.
    pub fn set_below(&self, color: Vec3) {
        let v = color.extend(0.0);
        self.set("Below", v, |u| u.below = v.to_array());
    }

    /// The box over the table the room is left out of while the camera looks
    /// down on it (`None`: nothing is left out).
    pub fn set_clear(&self, clear: Option<(Vec2, f32, f32)>) {
        let v = match clear {
            Some((half, bottom, top)) => vec4(half.x, half.y, bottom, top),
            None => Vec4::ZERO,
        };
        self.set("Clear", v, |u| u.clear = v.to_array());
    }

    /// The size of the table, with its rails (`half`): the pool of light on
    /// the dark stage fits it, whatever its size. `glint`: the angle a
    /// lamp's glint on a ball spans at least, per metre from the camera
    /// (see [`glint_angle`]).
    pub fn set_table(&self, half: Vec2, glint: f32) {
        let spot = vec4(half.x * 1.08, half.y * 1.18, glint, 0.0);
        self.set("Spot", spot, |u| u.spot = spot.to_array());
    }

    /// The pocket holes and throats cut out of the table (see `cut_away`).
    pub fn set_pockets(&self, holes: &[Vec4; 6], throats: &[Vec4; 12]) {
        for m in [&self.lit, &self.blended, &self.glow] {
            m.set_uniform_array("Holes", &holes[..]);
            m.set_uniform_array("Throats", &throats[..]);
        }
        let mut u = self.state.borrow_mut();
        for (slot, h) in u.holes.iter_mut().zip(holes) {
            *slot = h.to_array();
        }
        for (slot, t) in u.throats.iter_mut().zip(throats) {
            *slot = t.to_array();
        }
    }

    pub fn use_blended(&self, s: Surface) {
        set_surface(&self.blended, s, self.room.get());
        self.set_state_surface(s);
        self.blending.set(Blending::On);
        gl_use_material(&self.blended);
    }

    /// Light in the air (the haze under a lamp, a bulb's halo): blended,
    /// behind what is solid, and leaving no depth of its own, so one never
    /// cuts a square out of another drawn after it.
    pub fn use_glow(&self, s: Surface) {
        set_surface(&self.glow, s, self.room.get());
        self.set_state_surface(s);
        self.blending.set(Blending::Glow);
        gl_use_material(&self.glow);
    }

    fn set_state_surface(&self, s: Surface) {
        let mut u = self.state.borrow_mut();
        u.surface = [s.specular, s.shininess, if s.unlit { 1.0 } else { 0.0 }, s.rim];
        u.extra = [s.coat, s.cut, s.haze, kind(s, self.room.get())];
    }

    /// Draws `model` (moved by `transform`) with the material and surface in
    /// use. Meshes on the GPU go straight to it; the rest through macroquad.
    pub fn draw(&self, model: &Model, transform: Option<Mat4>) {
        let Some(raw) = self.raw.as_ref().filter(|_| !model.gpu.is_empty()) else {
            match transform {
                Some(m) => model.draw_at(m),
                None => model.draw(),
            }
            return;
        };
        let mut gl = unsafe { get_internal_gl() };
        // Whatever macroquad has batched goes first, to keep the order.
        gl.flush();
        let ctx = gl.quad_context;
        let mut u = self.state.borrow_mut();
        u.model = transform.unwrap_or(Mat4::IDENTITY).to_cols_array();
        let target = self.target.get().or_else(|| {
            scene_target().map(|(pass, (w, h))| (pass.raw_miniquad_id(), w as i32, h as i32))
        });
        let (w, h) = match target {
            Some((pass, w, h)) => {
                ctx.begin_pass(Some(pass), mq::PassAction::Nothing);
                (w as f32, h as f32)
            }
            None => {
                ctx.begin_default_pass(mq::PassAction::Nothing);
                mq::window::screen_size()
            }
        };
        ctx.apply_pipeline(match self.blending.get() {
            Blending::Off => &raw.lit,
            Blending::On => &raw.blended,
            Blending::Glow => &raw.glow,
        });
        ctx.apply_viewport(0, 0, w as i32, h as i32);
        ctx.apply_scissor_rect(0, 0, w as i32, h as i32);
        for g in &model.gpu {
            ctx.apply_bindings(&g.bindings);
            ctx.apply_uniforms(mq::UniformsSource::table(&*u));
            ctx.draw(0, g.count, 1);
        }
        ctx.end_render_pass();
    }
}

/// How what is drawn next blends (see [`Materials`]).
#[derive(Clone, Copy, PartialEq)]
enum Blending {
    Off,
    On,
    Glow,
}

fn set_surface(m: &Material, s: Surface, room: bool) {
    m.set_uniform("Surface", vec4(s.specular, s.shininess, if s.unlit { 1.0 } else { 0.0 }, s.rim));
    m.set_uniform("Extra", vec4(s.coat, s.cut, s.haze, kind(s, room)));
}

/// What a surface is to the shader (`Extra.w`): 2 a ball, 1 the room, 0 the
/// rest.
fn kind(s: Surface, room: bool) -> f32 {
    if s.cue {
        3.0
    } else if s.ball {
        2.0
    } else if room {
        1.0
    } else {
        0.0
    }
}

/// The angle (radians, between a lamp and its reflection) a lamp's glint on
/// a ball of radius `ball_radius` must span, per metre between the ball and
/// the camera, to be a dot of about a pixel and a half across the radius
/// in a window `screen_height` pixels high seen with the field of view
/// `fovy`.
pub fn glint_angle(fovy: f32, screen_height: f32, ball_radius: f32) -> f32 {
    const PIXELS: f32 = 1.5;
    // The size of a pixel a metre away; the reflection turns twice as fast
    // as the ball's surface.
    let pixel = 2.0 * (fovy / 2.0).tan() / screen_height.max(1.0);
    2.0 * PIXELS * pixel / ball_radius.max(1e-4)
}

/// Uploads an RGBA image as a mipmapped, repeating, trilinear-filtered texture.
/// Mipmaps are what keep fine patterns (cloth, wood grain) from shimmering.
pub fn texture(width: u16, height: u16, rgba: &[u8], repeat: bool) -> Texture2D {
    let tex = Texture2D::from_rgba8(width, height, rgba);
    let id = tex.raw_miniquad_id();
    let ctx = unsafe { get_internal_gl() }.quad_context;
    // GLES 2 without the NPOT extension draws a texture whose sides aren't
    // powers of two black if it has mipmaps or repeats: such a one (none
    // of the game's own) is drawn plain and clamped instead.
    let pot = width.is_power_of_two() && height.is_power_of_two();
    if pot {
        ctx.texture_generate_mipmaps(id);
        ctx.texture_set_min_filter(id, FilterMode::Linear, MipmapFilterMode::Linear);
    } else {
        ctx.texture_set_min_filter(id, FilterMode::Linear, MipmapFilterMode::None);
    }
    ctx.texture_set_mag_filter(id, FilterMode::Linear);
    let wrap = if repeat && pot { TextureWrap::Repeat } else { TextureWrap::Clamp };
    ctx.texture_set_wrap(id, wrap, wrap);
    tex
}

/// A 1x1 white texture, for meshes coloured only through vertex colours.
pub fn white_texture() -> Texture2D {
    texture(1, 1, &[255, 255, 255, 255], true)
}

/// Builds triangle meshes vertex by vertex.
#[derive(Default)]
pub struct MeshBuilder {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

fn vtx(p: Vec3, uv: Vec2, n: Vec3, color: Color) -> Vertex {
    Vertex {
        position: p,
        uv,
        color: [
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            (color.a * 255.0) as u8,
        ],
        normal: n.extend(0.0),
    }
}

impl MeshBuilder {
    pub fn push(&mut self, p: Vec3, uv: Vec2, n: Vec3, color: Color) -> u16 {
        assert!(self.vertices.len() < u16::MAX as usize, "mesh too large");
        self.vertices.push(vtx(p, uv, n, color));
        (self.vertices.len() - 1) as u16
    }

    /// Quad `a b c d` (counter-clockwise) with a flat normal.
    pub fn quad(&mut self, p: [Vec3; 4], uv: [Vec2; 4], color: Color) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        // No allocation: guides and shadows are made of quads every frame.
        let i: [u16; 4] = std::array::from_fn(|k| self.push(p[k], uv[k], n, color));
        self.indices.extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
    }

    /// Quad with texture coordinates in metres (for tiling textures) using the
    /// two axes most aligned with the face.
    pub fn quad_world_uv(&mut self, p: [Vec3; 4], color: Color, scale: f32) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let uv = |v: Vec3| {
            if n.y.abs() > 0.7 {
                vec2(v.x, v.z) * scale
            } else if n.x.abs() > n.z.abs() {
                vec2(v.z, v.y) * scale
            } else {
                vec2(v.x, v.y) * scale
            }
        };
        self.quad(p, [uv(p[0]), uv(p[1]), uv(p[2]), uv(p[3])], color);
    }

    /// Axis-aligned box (after an optional rotation about Y) with world-space UVs.
    pub fn cuboid(&mut self, center: Vec3, size: Vec3, yaw: f32, color: Color, uv_scale: f32) {
        let h = size / 2.0;
        let rot = Quat::from_rotation_y(yaw);
        let c = |x: f32, y: f32, z: f32| center + rot * vec3(x * h.x, y * h.y, z * h.z);
        let faces = [
            [c(-1., 1., 1.), c(1., 1., 1.), c(1., 1., -1.), c(-1., 1., -1.)], // top
            [c(-1., -1., -1.), c(1., -1., -1.), c(1., -1., 1.), c(-1., -1., 1.)], // bottom
            [c(-1., -1., 1.), c(1., -1., 1.), c(1., 1., 1.), c(-1., 1., 1.)], // +z
            [c(1., -1., -1.), c(-1., -1., -1.), c(-1., 1., -1.), c(1., 1., -1.)], // -z
            [c(1., -1., 1.), c(1., -1., -1.), c(1., 1., -1.), c(1., 1., 1.)], // +x
            [c(-1., -1., -1.), c(-1., -1., 1.), c(-1., 1., 1.), c(-1., 1., -1.)], // -x
        ];
        for f in faces {
            self.quad_world_uv(f, color, uv_scale);
        }
    }

    /// Extrudes a 2D profile (in the plane spanned by `out` and +Y) along the
    /// segment `a -> b`. Used for cushions.
    pub fn extrude(&mut self, a: Vec3, b: Vec3, out: Vec3, profile: &[Vec2], color: Color) {
        // The faces' normals come from the winding, which depends on the
        // direction of `a -> b` relative to `out`: run it the way that makes
        // them face outwards (otherwise half the cushions are lit from
        // behind and look dark).
        let (a, b) = if (b - a).dot(out.cross(Vec3::Y)) < 0.0 { (b, a) } else { (a, b) };
        let n = profile.len();
        let point = |base: Vec3, p: Vec2| base + out * p.x + Vec3::Y * p.y;
        let len = (b - a).length();
        for k in 0..n {
            let p0 = profile[k];
            let p1 = profile[(k + 1) % n];
            let quad = [point(a, p0), point(b, p0), point(b, p1), point(a, p1)];
            let v0 = k as f32 * 0.05;
            self.quad(
                quad,
                [
                    vec2(0.0, v0),
                    vec2(len * 4.0, v0),
                    vec2(len * 4.0, v0 + 0.05),
                    vec2(0.0, v0 + 0.05),
                ],
                color,
            );
        }
        // End caps (fan; profiles are convex).
        for (base, flip) in [(a, true), (b, false)] {
            let dir = (b - a).normalize_or_zero() * if flip { -1.0 } else { 1.0 };
            let first = self.push(point(base, profile[0]), Vec2::ZERO, dir, color);
            let mut prev = self.push(point(base, profile[1]), Vec2::ZERO, dir, color);
            for p in &profile[2..] {
                let cur = self.push(point(base, *p), Vec2::ZERO, dir, color);
                self.indices.extend_from_slice(&[first, prev, cur]);
                prev = cur;
            }
        }
    }

    /// A turned shape (a lathe): `profile` is (height, radius) pairs from the
    /// bottom up; normals follow the curve, so it shades smoothly.
    pub fn lathe(&mut self, at: Vec3, profile: &[(f32, f32)], segments: u32, color: Color) {
        let n = profile.len();
        if n < 2 {
            return;
        }
        // The slope at each ring, averaged with its neighbours.
        let normal_2d = |i: usize| -> Vec2 {
            let a = profile[i.saturating_sub(1)];
            let b = profile[(i + 1).min(n - 1)];
            let (dy, dr) = (b.0 - a.0, b.1 - a.1);
            vec2(dy, -dr).normalize_or_zero()
        };
        let first = self.vertices.len();
        for k in 0..=segments {
            let a = k as f32 / segments as f32 * std::f32::consts::TAU;
            let (sin, cos) = a.sin_cos();
            for (i, &(y, r)) in profile.iter().enumerate() {
                let nn = normal_2d(i);
                let normal = vec3(cos * nn.x, nn.y, sin * nn.x).normalize_or_zero();
                let uv = vec2(k as f32 / segments as f32, y);
                self.push(at + vec3(cos * r, y, sin * r), uv, normal, color);
            }
        }
        let stride = n as u16;
        let base = first as u16;
        for k in 0..segments as u16 {
            for i in 0..(n as u16 - 1) {
                let a = base + k * stride + i;
                let b = base + (k + 1) * stride + i;
                self.indices.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
            }
        }
        // Closed ends where the profile meets the axis are already closed;
        // a flat top cap where it doesn't.
        let (ty, tr) = profile[n - 1];
        if tr > 1e-4 {
            self.disc(at + Vec3::Y * ty, tr, segments, color);
        }
    }

    /// A flat rectangle split into `nu` × `nv` cells (so baked lighting can
    /// vary across it), with texture coordinates in metres × `uv_scale`.
    #[allow(clippy::too_many_arguments)]
    pub fn grid(
        &mut self,
        origin: Vec3,
        u: Vec3,
        v: Vec3,
        nu: u32,
        nv: u32,
        color: Color,
        uv_scale: f32,
    ) {
        let normal = u.cross(v).normalize_or_zero();
        let (lu, lv) = (u.length(), v.length());
        let first = self.vertices.len() as u16;
        for j in 0..=nv {
            for i in 0..=nu {
                let (fu, fv) = (i as f32 / nu as f32, j as f32 / nv as f32);
                let p = origin + u * fu + v * fv;
                self.push(p, vec2(fu * lu, fv * lv) * uv_scale, normal, color);
            }
        }
        let row = (nu + 1) as u16;
        for j in 0..nv as u16 {
            for i in 0..nu as u16 {
                let a = first + j * row + i;
                self.indices.extend_from_slice(&[a, a + 1, a + row + 1, a, a + row + 1, a + row]);
            }
        }
    }

    /// Multiplies every vertex colour by `light(position, normal)` (baked
    /// lighting for the room, which is drawn unlit).
    /// Big meshes are baked on every core (shadows make each vertex dear).
    pub fn bake(&mut self, light: impl Fn(Vec3, Vec3) -> Vec3 + Sync) {
        let bake_all = |vertices: &mut [Vertex]| {
            for v in vertices {
                let l = light(v.position, v.normal.truncate());
                for (k, c) in v.color.iter_mut().take(3).enumerate() {
                    *c = (*c as f32 * l[k].clamp(0.0, 1.0)).round() as u8;
                }
            }
        };
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(8);
        if threads < 2 || self.vertices.len() < 4000 {
            bake_all(&mut self.vertices);
            return;
        }
        let chunk = self.vertices.len().div_ceil(threads);
        std::thread::scope(|s| {
            for part in self.vertices.chunks_mut(chunk) {
                s.spawn(|| bake_all(part));
            }
        });
    }

    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Vertical cylinder (open or capped on top).
    #[cfg(test)]
    pub fn cylinder(
        &mut self,
        center: Vec3,
        radius: f32,
        height: f32,
        segments: u32,
        color: Color,
        top: bool,
    ) {
        let ring = |y: f32, k: u32| {
            let a = k as f32 / segments as f32 * std::f32::consts::TAU;
            center + vec3(a.cos() * radius, y, a.sin() * radius)
        };
        for k in 0..segments {
            let quad = [ring(0.0, k + 1), ring(0.0, k), ring(height, k), ring(height, k + 1)];
            self.quad(quad, [Vec2::ZERO; 4], color);
        }
        if top {
            let c = self.push(center + Vec3::Y * height, Vec2::ZERO, Vec3::Y, color);
            for k in 0..segments {
                let a = self.push(ring(height, k), Vec2::ZERO, Vec3::Y, color);
                let b = self.push(ring(height, k + 1), Vec2::ZERO, Vec3::Y, color);
                self.indices.extend_from_slice(&[c, b, a]);
            }
        }
    }

    /// The inside of a vertical tube (a hole's wall): faces the axis.
    pub fn tube_inside(
        &mut self,
        center: Vec3,
        radius: f32,
        height: f32,
        segments: u32,
        color: Color,
    ) {
        let ring = |y: f32, k: u32| {
            let a = k as f32 / segments as f32 * std::f32::consts::TAU;
            center + vec3(a.cos() * radius, y, a.sin() * radius)
        };
        for k in 0..segments {
            let quad = [ring(0.0, k), ring(0.0, k + 1), ring(height, k + 1), ring(height, k)];
            self.quad(quad, [Vec2::ZERO; 4], color);
        }
    }

    /// Flat disc facing up.
    pub fn disc(&mut self, center: Vec3, radius: f32, segments: u32, color: Color) {
        let c = self.push(center, vec2(0.5, 0.5), Vec3::Y, color);
        for k in 0..segments {
            let a0 = k as f32 / segments as f32 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / segments as f32 * std::f32::consts::TAU;
            let p0 = center + vec3(a0.cos(), 0.0, a0.sin()) * radius;
            let p1 = center + vec3(a1.cos(), 0.0, a1.sin()) * radius;
            let i0 =
                self.push(p0, vec2(0.5 + a0.cos() * 0.5, 0.5 + a0.sin() * 0.5), Vec3::Y, color);
            let i1 =
                self.push(p1, vec2(0.5 + a1.cos() * 0.5, 0.5 + a1.sin() * 0.5), Vec3::Y, color);
            self.indices.extend_from_slice(&[c, i1, i0]);
        }
    }

    /// A flat strip from `a` to `c` on the cloth (facing up), `wa` and `wc`
    /// wide at its ends, whose long edges fade out over `fa` and `fc`: a
    /// line that is smooth on any screen, anti-aliased or not.
    #[allow(clippy::too_many_arguments)]
    pub fn soft_line(
        &mut self,
        a: Vec3,
        c: Vec3,
        wa: f32,
        wc: f32,
        fa: f32,
        fc: f32,
        color: Color,
    ) {
        let d = c - a;
        let side = vec3(-d.z, 0.0, d.x).normalize_or_zero();
        let clear = Color::new(color.r, color.g, color.b, 0.0);
        // Across the strip: clear, solid, solid, clear.
        let mut row = |p: Vec3, w: f32, f: f32| -> [u16; 4] {
            [
                self.push(p - side * (w / 2.0 + f), Vec2::ZERO, Vec3::Y, clear),
                self.push(p - side * (w / 2.0), Vec2::ZERO, Vec3::Y, color),
                self.push(p + side * (w / 2.0), Vec2::ZERO, Vec3::Y, color),
                self.push(p + side * (w / 2.0 + f), Vec2::ZERO, Vec3::Y, clear),
            ]
        };
        let (r0, r1) = (row(a, wa, fa), row(c, wc, fc));
        for k in 0..3 {
            self.indices.extend_from_slice(&[r0[k], r1[k], r1[k + 1], r0[k], r1[k + 1], r0[k + 1]]);
        }
    }

    /// A flat ring facing up, `width` wide round `radius`, whose edges fade
    /// out over `feather` (see [`MeshBuilder::soft_line`]).
    pub fn soft_ring(
        &mut self,
        center: Vec3,
        radius: f32,
        width: f32,
        feather: f32,
        segments: u32,
        color: Color,
    ) {
        let clear = Color::new(color.r, color.g, color.b, 0.0);
        let radii = [
            (radius - width / 2.0 - feather, clear),
            (radius - width / 2.0, color),
            (radius + width / 2.0, color),
            (radius + width / 2.0 + feather, clear),
        ];
        let first = self.vertices.len() as u16;
        for k in 0..=segments {
            let a = k as f32 / segments as f32 * std::f32::consts::TAU;
            let d = vec3(a.cos(), 0.0, a.sin());
            for (r, c) in radii {
                self.push(center + d * r.max(0.0), Vec2::ZERO, Vec3::Y, c);
            }
        }
        for k in 0..segments as u16 {
            for j in 0..3 {
                let (p, q) = (first + k * 4 + j, first + (k + 1) * 4 + j);
                self.indices.extend_from_slice(&[p, q, q + 1, p, q + 1, p + 1]);
            }
        }
    }

    /// The half of a sphere that faces the viewer (`toward` points at the
    /// eye, `right` and `up` span the plane across it), with the sphere's
    /// own normals: lit, it is the ball the viewer would see, and there is
    /// no far side to show through a see-through one. The ghost ball.
    #[allow(clippy::too_many_arguments)]
    pub fn dome(
        &mut self,
        center: Vec3,
        radius: f32,
        right: Vec3,
        up: Vec3,
        toward: Vec3,
        segments: u32,
        rings: u32,
        color: Color,
    ) {
        let first = self.vertices.len() as u16;
        // From the pole (nearest the eye) out to the rim, a little past the
        // half so the outline is the sphere's from close by too.
        let reach = std::f32::consts::FRAC_PI_2 * 1.04;
        for ring in 0..=rings {
            let t = ring as f32 / rings as f32 * reach;
            let (st, ct) = t.sin_cos();
            for k in 0..=segments {
                let a = k as f32 / segments as f32 * std::f32::consts::TAU;
                let n = (right * a.cos() + up * a.sin()) * st + toward * ct;
                self.push(center + n * radius, Vec2::ZERO, n, color);
            }
        }
        let stride = (segments + 1) as u16;
        for ring in 0..rings as u16 {
            for k in 0..segments as u16 {
                let a = first + ring * stride + k;
                let c = a + stride;
                self.indices.extend_from_slice(&[a, a + 1, c, a + 1, c + 1, c]);
            }
        }
    }

    /// Frustum along +Y from `y0` (radius `r0`) to `y1` (radius `r1`).
    /// A frustum along +Y like [`MeshBuilder::frustum`], placed at `at`.
    #[allow(clippy::too_many_arguments)]
    pub fn frustum_at(
        &mut self,
        at: Vec3,
        y0: f32,
        r0: f32,
        y1: f32,
        r1: f32,
        segments: u32,
        color: Color,
    ) {
        let first = self.vertices.len();
        self.frustum(y0, r0, y1, r1, segments, color);
        // Tilt the normals with the slope of the side, then move into place.
        let slope = (r0 - r1) / (y1 - y0).abs().max(1e-4);
        for v in &mut self.vertices[first..] {
            let n = vec3(v.normal.x, 0.0, v.normal.z);
            let n = (n + Vec3::Y * slope).normalize_or_zero();
            v.normal = vec4(n.x, n.y, n.z, 0.0);
            v.position += at;
        }
    }

    pub fn frustum(&mut self, y0: f32, r0: f32, y1: f32, r1: f32, segments: u32, color: Color) {
        for k in 0..segments {
            let a0 = k as f32 / segments as f32 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / segments as f32 * std::f32::consts::TAU;
            let d0 = vec3(a0.cos(), 0.0, a0.sin());
            let d1 = vec3(a1.cos(), 0.0, a1.sin());
            let p = [
                d0 * r0 + Vec3::Y * y0,
                d1 * r0 + Vec3::Y * y0,
                d1 * r1 + Vec3::Y * y1,
                d0 * r1 + Vec3::Y * y1,
            ];
            let i: Vec<u16> = [(0, d0), (1, d1), (2, d1), (3, d0)]
                .iter()
                .map(|&(k, n)| self.push(p[k], Vec2::ZERO, n, color))
                .collect();
            self.indices.extend_from_slice(&[i[0], i[2], i[1], i[0], i[3], i[2]]);
        }
    }

    /// Finishes the geometry and sends it to the GPU once: drawn through
    /// [`Materials::draw`], it costs no copying per frame.
    pub fn build(self, texture: &Texture2D) -> Model {
        let gpu = if self.indices.is_empty() {
            Vec::new()
        } else {
            let ctx = unsafe { get_internal_gl() }.quad_context;
            let vb = ctx.new_buffer(
                mq::BufferType::VertexBuffer,
                mq::BufferUsage::Immutable,
                mq::BufferSource::slice(&self.vertices),
            );
            let ib = ctx.new_buffer(
                mq::BufferType::IndexBuffer,
                mq::BufferUsage::Immutable,
                mq::BufferSource::slice(&self.indices),
            );
            let bindings = mq::Bindings {
                vertex_buffers: vec![vb],
                index_buffer: ib,
                images: vec![texture.raw_miniquad_id()],
            };
            vec![Gpu { bindings, count: self.indices.len() as i32 }]
        };
        let mut model = self.build_dynamic(texture);
        model.gpu = gpu;
        model
    }

    /// Finishes geometry made for one frame (guides, shadows): it goes
    /// through macroquad's batches. Big meshes are split into pieces that
    /// fit macroquad's default per-draw-call buffers (5000 indices, 10000
    /// vertices); keeping those buffers small matters on some drivers.
    pub fn build_dynamic(self, texture: &Texture2D) -> Model {
        const MAX_INDICES: usize = 4800;
        const MAX_VERTICES: usize = 9000;
        // A strong clone keeps the GPU texture alive as long as the mesh.
        let texture = Some(texture.clone());
        if self.indices.len() <= MAX_INDICES && self.vertices.len() <= MAX_VERTICES {
            return Model {
                parts: vec![Mesh { vertices: self.vertices, indices: self.indices, texture }],
                gpu: Vec::new(),
            };
        }
        let parts = split_mesh(&self.vertices, &self.indices, MAX_VERTICES, MAX_INDICES)
            .into_iter()
            .map(|(vertices, indices)| Mesh { vertices, indices, texture: texture.clone() })
            .collect();
        Model { parts, gpu: Vec::new() }
    }
}

/// Splits a triangle list into pieces of at most `max_v` vertices and `max_i`
/// indices, each with its own compact vertex list.
fn split_mesh(
    vertices: &[Vertex],
    indices: &[u16],
    max_v: usize,
    max_i: usize,
) -> Vec<(Vec<Vertex>, Vec<u16>)> {
    let mut parts = Vec::new();
    let mut remap = vec![u16::MAX; vertices.len()];
    let mut part_v: Vec<Vertex> = Vec::new();
    let mut part_i: Vec<u16> = Vec::new();
    for tri in indices.as_chunks::<3>().0 {
        if part_i.len() + 3 > max_i || part_v.len() + 3 > max_v {
            parts.push((std::mem::take(&mut part_v), std::mem::take(&mut part_i)));
            remap.iter_mut().for_each(|r| *r = u16::MAX);
        }
        for &i in tri {
            let slot = &mut remap[i as usize];
            if *slot == u16::MAX {
                *slot = part_v.len() as u16;
                part_v.push(vertices[i as usize]);
            }
            part_i.push(*slot);
        }
    }
    if !part_i.is_empty() {
        parts.push((part_v, part_i));
    }
    parts
}

/// Geometry kept on the GPU (see [`MeshBuilder::build`]).
struct Gpu {
    bindings: mq::Bindings,
    count: i32,
}

/// One or more meshes drawn together (see [`MeshBuilder::build`]).
pub struct Model {
    parts: Vec<Mesh>,
    gpu: Vec<Gpu>,
}

impl Drop for Model {
    fn drop(&mut self) {
        if self.gpu.is_empty() {
            return;
        }
        let ctx = unsafe { get_internal_gl() }.quad_context;
        for g in &self.gpu {
            ctx.delete_buffer(g.bindings.vertex_buffers[0]);
            ctx.delete_buffer(g.bindings.index_buffer);
        }
    }
}

impl Model {
    /// Draws with another texture from now on (the geometry stays).
    pub fn set_texture(&mut self, texture: &Texture2D) {
        for m in &mut self.parts {
            m.texture = Some(texture.clone());
        }
        for g in &mut self.gpu {
            g.bindings.images[0] = texture.raw_miniquad_id();
        }
    }

    pub fn draw(&self) {
        for m in &self.parts {
            draw_mesh(m);
        }
    }

    /// Draws transformed by `model`. The vertices are transformed on the CPU
    /// so the whole scene goes to the GPU in one batch (changing macroquad's
    /// model matrix forces a flush per object, and many flushes per frame
    /// make some drivers misbehave).
    pub fn draw_at(&self, model: Mat4) {
        let normal = Mat3::from_mat4(model);
        for m in &self.parts {
            let vertices = m
                .vertices
                .iter()
                .map(|v| {
                    let mut v = *v;
                    v.position = model.transform_point3(v.position);
                    let n = normal * v.normal.truncate();
                    v.normal = n.extend(0.0);
                    v
                })
                .collect();
            draw_mesh(&Mesh { vertices, indices: m.indices.clone(), texture: m.texture.clone() });
        }
    }
}

/// UV sphere whose texture mapping matches [`crate::textures::uv_to_dir`].
pub fn sphere_mesh(radius: f32, texture: &Texture2D, segments: u32) -> Model {
    sphere_mesh_tinted(radius, texture, segments, WHITE)
}

/// A sphere with a vertex colour (e.g. translucent).
pub fn sphere_mesh_tinted(radius: f32, texture: &Texture2D, segments: u32, tint: Color) -> Model {
    let seg = segments.max(8);
    let rings = (segments / 2).max(4);
    let mut b = MeshBuilder::default();
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        for s in 0..=seg {
            let u = s as f32 / seg as f32;
            let d = crate::textures::uv_to_dir(u, v);
            b.push(d * radius, vec2(u, v), d, tint);
        }
    }
    let stride = (seg + 1) as u16;
    for ring in 0..rings as u16 {
        for s in 0..seg as u16 {
            let a = ring * stride + s;
            let c = a + stride;
            b.indices.extend_from_slice(&[a, a + 1, c, a + 1, c + 1, c]);
        }
    }
    b.build(texture)
}

/// Perspective camera with a sensible depth range (macroquad's default far
/// plane is so distant that thin decals z-fight).
pub struct SceneCamera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fovy: f32,
}

thread_local! {
    /// Where the 3D scene is drawn this frame instead of the window (see
    /// [`crate::post`]): a render pass and its size.
    static SCENE_TARGET: std::cell::RefCell<Option<(macroquad::texture::RenderPass, (u32, u32))>> =
        const { std::cell::RefCell::new(None) };
}

/// The 3D scene is drawn into `target` from now on (none: the window); see
/// [`crate::post`].
pub fn set_scene_target(target: Option<(macroquad::texture::RenderPass, (u32, u32))>) {
    SCENE_TARGET.with(|t| *t.borrow_mut() = target);
}

fn scene_target() -> Option<(macroquad::texture::RenderPass, (u32, u32))> {
    SCENE_TARGET.with(|t| t.borrow().clone())
}

impl Camera for SceneCamera {
    fn matrix(&self) -> Mat4 {
        let aspect = screen_width() / screen_height().max(1.0);
        let (near, far) = self.depth_range();
        Mat4::perspective_rh_gl(self.fovy, aspect, near, far)
            * Mat4::look_at_rh(self.eye, self.target, self.up)
    }
    fn depth_enabled(&self) -> bool {
        true
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        scene_target().map(|(pass, _)| pass)
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        scene_target().map(|(_, (w, h))| (0, 0, w as i32, h as i32))
    }
}

/// A [`SceneCamera`] that draws into a picture (a render target with a
/// depth buffer), on its `size` from the corner (see
/// [`Materials::set_target`]).
pub struct TargetCamera {
    pub view: SceneCamera,
    pub pass: macroquad::texture::RenderPass,
    pub size: (u32, u32),
}

impl Camera for TargetCamera {
    fn matrix(&self) -> Mat4 {
        let v = &self.view;
        let aspect = self.size.0 as f32 / self.size.1.max(1) as f32;
        let (near, far) = v.depth_range();
        Mat4::perspective_rh_gl(v.fovy, aspect, near, far) * Mat4::look_at_rh(v.eye, v.target, v.up)
    }
    fn depth_enabled(&self) -> bool {
        true
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        Some(self.pass.clone())
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        Some((0, 0, self.size.0 as i32, self.size.1 as i32))
    }
}

impl SceneCamera {
    /// The near and far planes. An eye above every ceiling (the overhead
    /// view looks from high up through a long lens) sees only what is
    /// within reach of what it looks at: the hall above the table is cut
    /// away.
    fn depth_range(&self) -> (f32, f32) {
        const ABOVE_THE_HALLS: f32 = 2.8;
        if self.eye.y > ABOVE_THE_HALLS {
            let dist = self.eye.distance(self.target);
            ((dist - 1.7).max(0.03), dist + 4.0)
        } else {
            (0.03, 40.0)
        }
    }

    /// Screen position of a world point (None if behind the camera).
    pub fn project(&self, p: Vec3) -> Option<Vec2> {
        let clip = self.matrix() * p.extend(1.0);
        if clip.w <= 1e-5 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(vec2((ndc.x + 1.0) * 0.5 * screen_width(), (1.0 - ndc.y) * 0.5 * screen_height()))
    }

    /// Ray through a screen point, for picking on the table plane.
    pub fn ray(&self, screen: Vec2) -> (Vec3, Vec3) {
        let ndc =
            vec2(screen.x / screen_width() * 2.0 - 1.0, 1.0 - screen.y / screen_height() * 2.0);
        let inv = self.matrix().inverse();
        let near = inv.project_point3(vec3(ndc.x, ndc.y, -1.0));
        let far = inv.project_point3(vec3(ndc.x, ndc.y, 1.0));
        (near, (far - near).normalize())
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn splitting_keeps_every_triangle() {
        let mut b = MeshBuilder::default();
        for k in 0..2000 {
            let x = k as f32;
            b.quad(
                [
                    vec3(x, 0.0, 0.0),
                    vec3(x + 1.0, 0.0, 0.0),
                    vec3(x + 1.0, 1.0, 0.0),
                    vec3(x, 1.0, 0.0),
                ],
                [Vec2::ZERO; 4],
                WHITE,
            );
        }
        let parts = split_mesh(&b.vertices, &b.indices, 9000, 4800);
        assert!(parts.len() > 1);
        let total: usize = parts.iter().map(|(_, i)| i.len()).sum();
        assert_eq!(total, b.indices.len());
        for (v, i) in &parts {
            assert!(i.len() <= 4800 && v.len() <= 9000);
            // Same triangles, same positions.
            assert!(i.iter().all(|&k| (k as usize) < v.len()));
        }
        let first = &parts[0];
        assert_eq!(
            first.0[first.1[0] as usize].position,
            b.vertices[b.indices[0] as usize].position
        );
    }
}

/// Every face must point the way it is seen from: a face whose normal points
/// away from the viewer is lit from behind and looks dark (the bottom
/// cushion, the leather around the pockets…). These check the primitives.
#[cfg(test)]
mod orientation_tests {
    use super::*;

    /// The normal of each triangle from its winding, with its centroid.
    fn faces(b: &MeshBuilder) -> Vec<(Vec3, Vec3, Vec3)> {
        b.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [p0, p1, p2] = t.map(|i| b.vertices[i as usize].position);
                let n = b.vertices[t[0] as usize].normal.truncate();
                let geometric = (p1 - p0).cross(p2 - p0).normalize_or_zero();
                ((p0 + p1 + p2) / 3.0, n, geometric)
            })
            .collect()
    }

    #[test]
    fn tube_inside_faces_the_axis_and_cylinder_faces_out() {
        let mut b = MeshBuilder::default();
        b.tube_inside(Vec3::ZERO, 0.06, 0.1, 24, WHITE);
        for (c, n, _) in faces(&b) {
            assert!(n.dot(vec3(c.x, 0.0, c.z)) < 0.0, "tube normal {n} at {c}");
        }
        let mut b = MeshBuilder::default();
        b.cylinder(Vec3::ZERO, 0.06, 0.1, 24, WHITE, false);
        for (c, n, _) in faces(&b) {
            assert!(n.dot(vec3(c.x, 0.0, c.z)) > 0.0, "cylinder normal {n} at {c}");
        }
    }

    #[test]
    fn extruded_cushions_face_out_whichever_way_they_run() {
        // A cushion profile: nose towards -out, top, back.
        let profile = [
            vec2(0.01, 0.0),
            vec2(0.0, 0.03),
            vec2(0.008, 0.04),
            vec2(0.05, 0.04),
            vec2(0.05, 0.0),
        ];
        let out = vec3(0.0, 0.0, 1.0);
        for (a, b) in [
            (vec3(-1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0)),
            (vec3(1.0, 0.0, 0.0), vec3(-1.0, 0.0, 0.0)),
        ] {
            let mut m = MeshBuilder::default();
            m.extrude(a, b, out, &profile, WHITE);
            // The solid's middle: every face must point away from it.
            let inside = vec3(0.0, 0.02, 0.03);
            for (c, n, _) in faces(&m) {
                assert!(n.dot(c - inside) > 0.0, "cushion face {n} at {c}");
            }
        }
    }

    #[test]
    fn quads_get_the_normal_of_their_winding() {
        let mut b = MeshBuilder::default();
        b.quad(
            [vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(1.0, 0.0, -1.0), vec3(0.0, 0.0, -1.0)],
            [Vec2::ZERO; 4],
            WHITE,
        );
        for (_, n, g) in faces(&b) {
            assert!(n.dot(g) > 0.99 && n.y > 0.99);
        }
    }
}

/// Where a part of a [`Kit`] is drawn in the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Opaque, with the table.
    Solid,
    /// The inside of the pockets (after the ball shadows).
    Inside,
    /// On the rails, after the pockets: sights, leathers.
    Detail,
    /// See-through (pools of light), not on the dark stage.
    Blended,
    /// The lamp over the table (hidden from above and on the dark stage).
    Lamp,
    /// The floor of a hall: drawn first, and laid a little see-through over
    /// its own reflection where the floor is polished (Ultra). Its vertex
    /// alpha is how much of the floor shows over the reflection.
    Floor,
    /// Haze under the lamp (High, Ultra): see-through, drawn last.
    Haze,
}

/// A light of a hall that gets a soft halo facing the camera (High, Ultra).
#[derive(Clone, Copy, Debug)]
pub struct Glow {
    pub at: Vec3,
    /// Half the halo's width (metres).
    pub size: f32,
    pub color: Color,
    /// Part of the lamp over the table (hidden with it).
    pub lamp: bool,
}

/// A part of a [`Kit`]: geometry, the kit texture it uses (`None`: plain
/// white) and how it is lit.
pub struct KitPart {
    pub mesh: MeshBuilder,
    pub tex: Option<usize>,
    pub surface: Surface,
    pub layer: Layer,
}

/// Geometry and textures made away from the main thread (themed tables and
/// halls), sent to the GPU later with [`Kit::finish`].
#[derive(Default)]
pub struct Kit {
    pub textures: Vec<(crate::textures::Pixels, bool)>,
    pub parts: Vec<KitPart>,
    /// Lights that get a halo (see [`Glow`]).
    pub glows: Vec<Glow>,
}

/// A part ready to draw.
pub struct Built {
    pub mesh: Model,
    pub surface: Surface,
    pub layer: Layer,
    /// The height of its lowest point (what is high up hardly shows in a
    /// floor's reflection, so that pass skips it).
    pub low: f32,
}

impl Kit {
    /// Adds a texture (`repeat`: tiles); returns its index.
    pub fn texture(&mut self, pixels: crate::textures::Pixels, repeat: bool) -> usize {
        self.textures.push((pixels, repeat));
        self.textures.len() - 1
    }

    /// Adds a part (nothing if the geometry is empty).
    pub fn add(&mut self, mesh: MeshBuilder, tex: Option<usize>, surface: Surface, layer: Layer) {
        if !mesh.is_empty() {
            self.parts.push(KitPart { mesh, tex, surface, layer });
        }
    }

    /// A halo round a light (see [`Glow`]).
    pub fn glow(&mut self, at: Vec3, size: f32, color: Color, lamp: bool) {
        self.glows.push(Glow { at, size, color, lamp });
    }

    /// Uploads the textures and builds the meshes.
    pub fn finish(self, white: &Texture2D) -> Vec<Built> {
        let textures: Vec<Texture2D> = self
            .textures
            .into_iter()
            .map(|(p, repeat)| texture(p.width, p.height, &p.rgba, repeat))
            .collect();
        self.parts
            .into_iter()
            .map(|p| {
                let low = p.mesh.vertices.iter().map(|v| v.position.y).fold(f32::MAX, f32::min);
                Built {
                    mesh: p.mesh.build(p.tex.map_or(white, |k| &textures[k])),
                    surface: p.surface,
                    layer: p.layer,
                    low,
                }
            })
            .collect()
    }
}
