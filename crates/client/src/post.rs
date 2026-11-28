//! Ultra's finishing touches over the 3D scene, as a camera would see it:
//! the scene is drawn into a picture of its own (with the window's
//! anti-aliasing), then laid on the window with a glow round what is
//! brightest (the lamps' glints on the balls, the bulbs, a white ball under
//! the light), a gently filmic contrast, a soft vignette and the faintest
//! grain. Nothing of the interface goes through it.

use macroquad::prelude::*;

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying mediump vec2 uv;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    uv = texcoord;
}
"#;

/// What is bright enough to glow, with a soft knee.
const BRIGHT: &str = r#"#version 100
precision mediump float;
varying mediump vec2 uv;
uniform sampler2D Texture;
void main() {
    vec3 c = texture2D(Texture, uv).rgb;
    float l = max(c.r, max(c.g, c.b));
    float k = smoothstep(0.62, 0.95, l);
    gl_FragColor = vec4(c * k, 1.0);
}
"#;

/// A Gaussian blur along `Step` (one texel, across or down).
const BLUR: &str = r#"#version 100
precision mediump float;
varying mediump vec2 uv;
uniform sampler2D Texture;
uniform vec4 Step;
void main() {
    vec2 d = Step.xy;
    vec3 c = texture2D(Texture, uv).rgb * 0.227;
    c += (texture2D(Texture, uv + d * 1.385).rgb + texture2D(Texture, uv - d * 1.385).rgb) * 0.316;
    c += (texture2D(Texture, uv + d * 3.231).rgb + texture2D(Texture, uv - d * 3.231).rgb) * 0.070;
    gl_FragColor = vec4(c, 1.0);
}
"#;

/// The scene and its glow on the window: a gently filmic contrast, a
/// little saturation, a soft vignette, the faintest grain.
const COMPOSITE: &str = r#"#version 100
precision mediump float;
varying mediump vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Glow;
uniform vec4 Params; // x: glow, y: vignette, z: grain, w: time
// A noise that keeps every number small (a sine of thousands overflows
// where the card works at half precision, and blacks out half the picture).
float hash(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}
void main() {
    vec3 c = texture2D(Texture, uv).rgb;
    vec3 g = texture2D(Glow, uv).rgb;
    c += g * Params.x;
    // A soft shoulder: what the glow pushed over white rolls off.
    c = c / (1.0 + max(c - 1.0, 0.0));
    // A gentle S-curve and a touch more colour.
    vec3 s = c * c * (3.0 - 2.0 * c);
    c = mix(c, s, 0.22);
    float l = dot(c, vec3(0.299, 0.587, 0.114));
    c = mix(vec3(l), c, 1.06);
    // The corners a little darker.
    vec2 q = uv - 0.5;
    float v = smoothstep(0.35, 0.85, length(q * vec2(1.25, 1.0)));
    c *= 1.0 - Params.y * v;
    // The faintest grain.
    c += (hash(floor(gl_FragCoord.xy) + floor(fract(Params.w) * 61.0) * 17.0) - 0.5) * Params.z;
    gl_FragColor = vec4(clamp(c, 0.0, 1.0), 1.0);
}
"#;

pub struct Post {
    size: (u32, u32),
    samples: i32,
    scene: Option<RenderTarget>,
    bright: Option<RenderTarget>,
    blur: Option<(RenderTarget, RenderTarget)>,
    bright_mat: Material,
    blur_mat: Material,
    composite: Material,
}

fn material(fragment: &str, uniforms: Vec<UniformDesc>, textures: Vec<String>) -> Option<Material> {
    load_material(
        ShaderSource::Glsl { vertex: VERTEX, fragment },
        MaterialParams { uniforms, textures, ..Default::default() },
    )
    .map_err(|e| eprintln!("post-processing off: {e:?}"))
    .ok()
}

fn target(w: u32, h: u32, samples: i32, depth: bool) -> RenderTarget {
    let rt =
        render_target_ex(w.max(1), h.max(1), RenderTargetParams { sample_count: samples, depth });
    rt.texture.set_filter(FilterMode::Linear);
    rt
}

impl Post {
    /// The finishing touches (none if the graphics card can't compile them).
    pub fn new() -> Option<Post> {
        Some(Post {
            size: (0, 0),
            samples: 1,
            scene: None,
            bright: None,
            blur: None,
            bright_mat: material(BRIGHT, vec![], vec![])?,
            blur_mat: material(BLUR, vec![UniformDesc::new("Step", UniformType::Float4)], vec![])?,
            composite: material(
                COMPOSITE,
                vec![UniformDesc::new("Params", UniformType::Float4)],
                vec!["Glow".to_string()],
            )?,
        })
    }

    /// The 3D scene goes into the picture from now on, cleared to `clear`
    /// (with the window's anti-aliasing, `samples`).
    pub fn begin(&mut self, clear: Color, samples: i32) {
        let (w, h) = (screen_width() as u32, screen_height() as u32);
        if self.scene.is_none() || self.size != (w, h) || self.samples != samples {
            self.size = (w, h);
            self.samples = samples;
            self.scene = Some(target(w, h, samples.max(1), true));
            let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
            self.bright = Some(target(hw, hh, 1, false));
            self.blur = Some((target(hw, hh, 1, false), target(hw, hh, 1, false)));
        }
        let Some(scene) = &self.scene else { return };
        crate::gfx::set_scene_target(Some((scene.render_pass.clone(), (w, h))));
        set_camera(&Camera2D {
            render_target: Some(scene.clone()),
            ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, w as f32, h as f32))
        });
        clear_background(clear);
        set_default_camera();
    }

    /// The scene, with its glow and finish, on the window.
    pub fn finish(&self) {
        crate::gfx::set_scene_target(None);
        let (Some(scene), Some(bright), Some((a, b))) = (&self.scene, &self.bright, &self.blur)
        else {
            return;
        };
        let (hw, hh) = (bright.texture.width(), bright.texture.height());
        let into = |rt: &RenderTarget, mat: &Material, src: &Texture2D, w: f32, h: f32| {
            set_camera(&Camera2D {
                render_target: Some(rt.clone()),
                ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, w, h))
            });
            clear_background(BLACK);
            gl_use_material(mat);
            // (Each copy turned back the right way up.)
            draw_texture_ex(
                src,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(w, h)),
                    flip_y: true,
                    ..Default::default()
                },
            );
            gl_use_default_material();
        };
        // What glows, at half size; blurred across and down, twice (a wide,
        // soft glow).
        into(bright, &self.bright_mat, &scene.texture, hw, hh);
        let mut src = bright.texture.clone();
        for spread in [1.0, 2.2] {
            self.blur_mat.set_uniform("Step", vec4(spread / hw, 0.0, 0.0, 0.0));
            into(a, &self.blur_mat, &src, hw, hh);
            self.blur_mat.set_uniform("Step", vec4(0.0, spread / hh, 0.0, 0.0));
            into(b, &self.blur_mat, &a.texture, hw, hh);
            src = b.texture.clone();
        }
        // On the window.
        set_default_camera();
        self.composite.set_texture("Glow", b.texture.clone());
        self.composite.set_uniform("Params", vec4(0.55, 0.16, 0.012, (get_time() % 100.0) as f32));
        gl_use_material(&self.composite);
        draw_texture_ex(
            &scene.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                flip_y: true,
                ..Default::default()
            },
        );
        gl_use_default_material();
    }
}
