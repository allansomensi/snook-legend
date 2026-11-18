//! The pictures a player can choose for their profile: friendly faces
//! drawn with the same pen as the computer's opponents.

use super::*;

#[derive(Clone, Copy)]
enum Hair {
    Short,
    Spiky,
    Long,
    Afro,
    Bun,
    Bald,
    /// Long hair tied back, the tail over one shoulder.
    Ponytail,
    /// Short tight curls.
    Curly,
    /// Shaved sides, a crest on top.
    Mohawk,
    /// Shoulder-length hair with a straight fringe.
    Fringe,
    /// A baseball cap of this colour.
    Cap([u8; 3]),
    /// A knitted beanie of this colour.
    Beanie([u8; 3]),
}

#[derive(Clone, Copy)]
struct Look {
    bg: [u8; 3],
    skin: [u8; 3],
    hair: Hair,
    hair_color: [u8; 3],
    top: [u8; 3],
    iris: [u8; 3],
    glasses: bool,
    beard: bool,
    lashes: bool,
    grin: bool,
}

const fn look(bg: [u8; 3], skin: [u8; 3], hair: Hair, hair_color: [u8; 3], top: [u8; 3]) -> Look {
    Look {
        bg,
        skin,
        hair,
        hair_color,
        top,
        iris: [70, 50, 34],
        glasses: false,
        beard: false,
        lashes: false,
        grin: false,
    }
}

/// The look behind each code of [`snook_legend_proto::AVATARS`].
fn look_of(code: &str) -> Option<Look> {
    Some(match code {
        "sunny" => Look {
            lashes: true,
            grin: true,
            iris: [60, 120, 150],
            ..look([40, 130, 130], [246, 210, 180], Hair::Long, [226, 182, 90], [70, 170, 90])
        },
        "marco" => Look {
            beard: true,
            ..look([200, 110, 40], [214, 160, 118], Hair::Short, [48, 34, 26], [40, 60, 110])
        },
        "zuri" => Look {
            lashes: true,
            ..look([110, 70, 150], [118, 76, 52], Hair::Afro, [28, 22, 20], [235, 190, 50])
        },
        "kai" => Look {
            grin: true,
            ..look(
                [60, 130, 190],
                [232, 186, 146],
                Hair::Cap([200, 50, 44]),
                [60, 40, 28],
                [40, 100, 200],
            )
        },
        "mei" => Look {
            glasses: true,
            lashes: true,
            ..look([170, 80, 110], [244, 214, 186], Hair::Bun, [26, 22, 24], [200, 50, 60])
        },
        "omar" => Look {
            beard: true,
            grin: true,
            ..look([190, 150, 50], [150, 100, 70], Hair::Bald, [36, 26, 22], [40, 130, 80])
        },
        "lena" => Look {
            lashes: true,
            iris: [70, 130, 80],
            ..look([40, 50, 100], [248, 216, 196], Hair::Long, [176, 70, 36], [120, 70, 170])
        },
        "theo" => Look {
            glasses: true,
            grin: true,
            iris: [70, 110, 160],
            ..look([50, 120, 70], [240, 200, 168], Hair::Spiky, [120, 76, 40], [230, 120, 40])
        },
        "nia" => Look {
            lashes: true,
            grin: true,
            ..look([200, 90, 80], [96, 62, 44], Hair::Long, [22, 18, 18], [30, 140, 140])
        },
        "raul" => Look {
            beard: true,
            ..look(
                [80, 90, 110],
                [210, 156, 112],
                Hair::Beanie([120, 124, 130]),
                [52, 36, 26],
                [170, 40, 40],
            )
        },
        "ana" => Look {
            lashes: true,
            grin: true,
            ..look([60, 150, 120], [222, 176, 136], Hair::Bun, [110, 64, 36], [236, 236, 230])
        },
        "joe" => Look {
            glasses: true,
            ..look([40, 80, 160], [104, 68, 48], Hair::Short, [22, 18, 18], [230, 190, 60])
        },
        "iris" => Look {
            glasses: true,
            lashes: true,
            iris: [80, 110, 150],
            ..look([120, 40, 70], [240, 206, 186], Hair::Short, [200, 200, 205], [170, 140, 200])
        },
        "sam" => Look {
            grin: true,
            ..look(
                [170, 50, 50],
                [196, 140, 100],
                Hair::Beanie([220, 170, 40]),
                [40, 30, 24],
                [36, 36, 40],
            )
        },
        "hugo" => Look {
            beard: true,
            iris: [70, 120, 170],
            ..look([40, 100, 60], [246, 212, 184], Hair::Short, [222, 186, 110], [236, 236, 232])
        },
        "lua" => Look {
            lashes: true,
            grin: true,
            ..look([70, 60, 150], [140, 94, 66], Hair::Afro, [80, 48, 30], [230, 110, 160])
        },
        "duda" => Look {
            lashes: true,
            grin: true,
            ..look([230, 120, 60], [236, 196, 160], Hair::Ponytail, [90, 52, 30], [60, 90, 170])
        },
        "bento" => {
            Look { ..look([90, 60, 40], [200, 146, 104], Hair::Curly, [40, 28, 22], [60, 140, 90]) }
        }
        "rex" => Look {
            grin: true,
            ..look([30, 30, 40], [236, 200, 170], Hair::Mohawk, [40, 160, 200], [30, 30, 34])
        },
        "yuki" => Look {
            lashes: true,
            ..look([90, 150, 200], [246, 220, 196], Hair::Fringe, [24, 22, 26], [240, 160, 180])
        },
        "malik" => Look {
            beard: true,
            grin: true,
            ..look([30, 110, 90], [92, 60, 42], Hair::Curly, [20, 16, 16], [220, 120, 40])
        },
        "clara" => Look {
            lashes: true,
            glasses: true,
            iris: [80, 120, 90],
            ..look([150, 120, 60], [242, 208, 182], Hair::Ponytail, [210, 170, 90], [70, 80, 90])
        },
        "dante" => {
            Look { ..look([120, 30, 40], [222, 172, 128], Hair::Spiky, [24, 20, 20], [40, 40, 44]) }
        }
        "flor" => Look {
            lashes: true,
            grin: true,
            ..look([200, 80, 130], [170, 116, 80], Hair::Fringe, [60, 34, 22], [250, 210, 70])
        },
        "gus" => Look {
            beard: true,
            glasses: true,
            ..look([100, 110, 60], [240, 204, 176], Hair::Bald, [150, 150, 150], [110, 70, 50])
        },
        "tati" => Look {
            lashes: true,
            ..look([60, 40, 90], [124, 82, 58], Hair::Ponytail, [20, 16, 16], [200, 60, 90])
        },
        "nico" => Look {
            grin: true,
            iris: [80, 120, 160],
            ..look([210, 170, 60], [248, 220, 196], Hair::Curly, [200, 120, 50], [50, 120, 200])
        },
        "jade" => Look {
            lashes: true,
            ..look([30, 120, 100], [228, 190, 150], Hair::Long, [40, 30, 28], [236, 236, 230])
        },
        "beto" => Look {
            grin: true,
            beard: true,
            ..look(
                [160, 90, 50],
                [190, 132, 92],
                Hair::Cap([30, 90, 60]),
                [30, 24, 20],
                [240, 200, 60],
            )
        },
        "vera" => Look {
            lashes: true,
            glasses: true,
            ..look([80, 70, 120], [236, 200, 178], Hair::Bun, [190, 190, 196], [30, 110, 120])
        },
        "leon" => Look {
            iris: [60, 110, 70],
            ..look([50, 70, 50], [244, 214, 190], Hair::Mohawk, [180, 60, 30], [90, 90, 96])
        },
        "kiara" => Look {
            lashes: true,
            grin: true,
            ..look([240, 150, 60], [100, 66, 46], Hair::Fringe, [30, 22, 20], [120, 60, 170])
        },
        _ => return None,
    })
}

/// Draws the profile picture `code` in a disc of radius `r` at `c`;
/// false (nothing drawn) for a code that isn't one of the pictures.
pub fn draw_avatar(code: &str, c: Vec2, r: f32) -> bool {
    let Some(l) = look_of(code) else {
        return draw_object(code, c, r);
    };
    let p = Pen::new(c, r);
    let bg = rgb(l.bg);
    backdrop(&p, bg);
    figure(&p.figure(), &l);
    rim(&p, bg);
    true
}

/// A ball seen from the front: `c` with its shade, a soft light at the
/// upper left.
fn ball(p: &Pen, x: f32, y: f32, rad: f32, c: Color) {
    p.shaded(p.oval_poly(x, y, rad, rad), (-0.07 * rad / 0.5, -0.07 * rad / 0.5), 0.3, c);
    p.oval(x - rad * 0.38, y - rad * 0.42, rad * 0.2, rad * 0.13, with_alpha(WHITE, 0.35));
}

/// Pictures that are not people: things of the game, for anyone.
fn draw_object(code: &str, c: Vec2, r: f32) -> bool {
    let p = Pen::new(c, r);
    let cloth = rgb([34, 110, 62]);
    let wood = rgb([150, 96, 52]);
    let bg = match code {
        "pic_eight" | "pic_cue_ball" | "pic_stripe" | "pic_rack" | "pic_table" => cloth,
        "pic_cues" | "pic_chalk" => rgb([92, 58, 34]),
        "pic_trophy" | "pic_crown" => rgb([58, 46, 110]),
        "pic_star" => rgb([30, 62, 120]),
        "pic_flame" => rgb([52, 34, 40]),
        "pic_bolt" => rgb([36, 44, 78]),
        _ => return false,
    };
    backdrop(&p, bg);
    let black = rgb([24, 22, 24]);
    match code {
        "pic_eight" => {
            ball(&p, 0.0, 0.0, 0.62, black);
            p.oval(0.0, -0.02, 0.3, 0.3, WHITE);
            // The 8: two rings.
            p.ring(0.0, -0.115, 0.085, 0.055, black);
            p.ring(0.0, 0.085, 0.105, 0.06, black);
        }
        "pic_cue_ball" => {
            // The cue coming in from the lower left.
            p.fill(
                p.poly(&[(-1.0, 0.62), (-1.0, 0.74), (-0.5, 0.42), (-0.52, 0.36)]),
                rgb([214, 176, 118]),
            );
            p.fill(
                p.poly(&[(-0.52, 0.36), (-0.5, 0.42), (-0.4, 0.355), (-0.42, 0.3)]),
                rgb([70, 120, 200]),
            );
            ball(&p, 0.08, -0.04, 0.5, rgb([240, 236, 224]));
            p.oval(0.02, 0.0, 0.035, 0.035, rgb([200, 40, 40]));
        }
        "pic_stripe" => {
            ball(&p, 0.0, 0.0, 0.62, rgb([240, 236, 224]));
            // The band, bowed like a ball's.
            let band =
                clip_convex(p.oval_poly(0.0, 0.0, 0.62, 0.36), &p.oval_poly(0.0, 0.0, 0.62, 0.62));
            fill(&clip_convex(band, &p.disc), p.col(rgb([232, 176, 30])));
            p.oval(0.0, 0.0, 0.2, 0.2, WHITE);
            // The 9: a ring and its tail.
            p.ring(0.0, -0.03, 0.07, 0.045, black);
            p.stroke(&p.quad((0.07, -0.02), (0.07, 0.1), (-0.03, 0.13)), 0.045, black);
        }
        "pic_rack" => {
            // The wooden triangle and six balls in it.
            let tri = [(0.0, -0.66), (0.7, 0.52), (-0.7, 0.52)];
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                p.line(a, b, 0.09, wood);
            }
            let colours = [
                [232, 176, 30],
                [40, 84, 190],
                [200, 44, 40],
                [24, 22, 24],
                [110, 50, 150],
                [226, 110, 36],
            ];
            let spots =
                [(0.0, -0.3), (-0.17, 0.0), (0.17, 0.0), (-0.34, 0.3), (0.0, 0.3), (0.34, 0.3)];
            for (k, (x, y)) in spots.into_iter().enumerate() {
                ball(&p, x, y, 0.155, rgb(colours[k]));
            }
        }
        "pic_cues" => {
            // Two cues crossed, a ball where they meet.
            for s in [-1.0f32, 1.0] {
                let (a, b) = ((s * -0.62, -0.62), (s * 0.62, 0.62));
                p.line(a, b, 0.085, rgb([92, 50, 26]));
                let mid = (s * 0.1, 0.1);
                p.line(a, mid, 0.07, rgb([218, 180, 122]));
                p.line(a, (a.0 + s * 0.07, a.1 + 0.07), 0.075, rgb([70, 120, 200]));
            }
            ball(&p, 0.0, 0.22, 0.2, black);
            p.oval(0.0, 0.215, 0.09, 0.09, WHITE);
        }
        "pic_chalk" => {
            let blue = rgb([52, 112, 204]);
            // A cube seen from above a corner: a top and two sides.
            p.fill(
                p.poly(&[(-0.5, -0.12), (0.0, 0.14), (0.0, 0.66), (-0.5, 0.4)]),
                shade(blue, -0.25),
            );
            p.fill(
                p.poly(&[(0.5, -0.12), (0.0, 0.14), (0.0, 0.66), (0.5, 0.4)]),
                shade(blue, -0.42),
            );
            p.fill(p.poly(&[(0.0, -0.4), (0.5, -0.12), (0.0, 0.14), (-0.5, -0.12)]), blue);
            // The hollow the tips have worn.
            p.oval(0.0, -0.13, 0.2, 0.1, shade(blue, -0.3));
            p.oval(-0.02, -0.14, 0.12, 0.055, shade(blue, 0.25));
        }
        "pic_trophy" => {
            let gold = GOLD_METAL;
            p.fill(p.round_rect(-0.34, 0.5, 0.34, 0.66, 0.04), rgb([70, 44, 28]));
            p.fill(p.rect(-0.07, 0.2, 0.07, 0.5), shade(gold, -0.2));
            p.fill(p.round_rect(-0.2, 0.42, 0.2, 0.52, 0.03), shade(gold, -0.1));
            for s in [-1.0f32, 1.0] {
                p.ring(s * 0.4, -0.2, 0.16, 0.06, shade(gold, -0.15));
            }
            let cup = p.poly(&[
                (-0.4, -0.5),
                (0.4, -0.5),
                (0.3, -0.02),
                (0.12, 0.24),
                (-0.12, 0.24),
                (-0.3, -0.02),
            ]);
            p.shaded(cup, (-0.06, -0.02), 0.22, gold);
            p.star((0.0, -0.2), 0.14, 0.06, 5, 0.0, shade(gold, 0.45));
        }
        "pic_star" => {
            p.star((0.02, 0.05), 0.68, 0.29, 5, 0.0, shade(GOLD_METAL, -0.3));
            p.star((0.0, 0.0), 0.66, 0.28, 5, 0.0, GOLD_METAL);
            p.star((-0.02, -0.03), 0.3, 0.13, 5, 0.0, shade(GOLD_METAL, 0.4));
        }
        "pic_crown" => {
            let gold = GOLD_METAL;
            // Three points on a band.
            for (x, h) in [(-0.38, 0.42), (0.0, 0.6), (0.38, 0.42)] {
                p.fill(p.poly(&[(x - 0.2, 0.22), (x, 0.22 - h), (x + 0.2, 0.22)]), gold);
                p.oval(x, 0.2 - h, 0.07, 0.07, rgb([220, 60, 70]));
            }
            p.fill(p.poly(&[(-0.58, -0.02), (-0.4, 0.22), (-0.58, 0.22)]), gold);
            p.fill(p.poly(&[(0.58, -0.02), (0.4, 0.22), (0.58, 0.22)]), gold);
            p.shaded(p.round_rect(-0.58, 0.2, 0.58, 0.46, 0.04), (0.0, -0.05), 0.25, gold);
            for x in [-0.3f32, 0.0, 0.3] {
                p.oval(x, 0.33, 0.055, 0.055, rgb([70, 140, 220]));
            }
        }
        "pic_table" => {
            // A table from above: rails, cloth, pockets, three balls.
            p.fill(p.round_rect(-0.74, -0.46, 0.74, 0.46, 0.1), wood);
            p.fill(p.round_rect(-0.62, -0.34, 0.62, 0.34, 0.04), rgb([44, 136, 78]));
            for (x, y) in [
                (-0.62, -0.34),
                (0.0, -0.37),
                (0.62, -0.34),
                (-0.62, 0.34),
                (0.0, 0.37),
                (0.62, 0.34),
            ] {
                p.oval(x, y, 0.075, 0.075, black);
            }
            ball(&p, -0.3, 0.05, 0.075, rgb([240, 236, 224]));
            ball(&p, 0.22, -0.1, 0.075, rgb([200, 44, 40]));
            ball(&p, 0.36, 0.12, 0.075, black);
        }
        "pic_flame" => {
            // A flame: a drop with a point, a lighter one inside.
            for (k, col) in [rgb([226, 82, 30]), rgb([246, 160, 40]), rgb([252, 226, 120])]
                .into_iter()
                .enumerate()
            {
                let s = 1.0 - 0.27 * k as f32;
                let y0 = 0.14 + 0.1 * k as f32;
                p.oval(0.0, y0, 0.4 * s, 0.42 * s, col);
                p.fill(
                    p.poly(&[
                        (-0.36 * s, y0 - 0.14 * s),
                        (0.08 * s, y0 - 0.92 * s),
                        (0.36 * s, y0 - 0.1 * s),
                    ]),
                    col,
                );
            }
        }
        "pic_bolt" => {
            let gold = rgb([250, 206, 60]);
            for (dx, dy, col) in [(0.03, 0.04, shade(gold, -0.4)), (0.0, 0.0, gold)] {
                p.fill(
                    p.poly(&[
                        (0.1 + dx, -0.7 + dy),
                        (-0.34 + dx, 0.08 + dy),
                        (0.0 + dx, 0.08 + dy),
                    ]),
                    col,
                );
                p.fill(
                    p.poly(&[
                        (0.0 + dx, -0.08 + dy),
                        (0.34 + dx, -0.08 + dy),
                        (-0.1 + dx, 0.7 + dy),
                    ]),
                    col,
                );
            }
        }
        _ => {}
    }
    rim(&p, bg);
    true
}

fn figure(p: &Pen, l: &Look) {
    let hidden_ears =
        matches!(l.hair, Hair::Long | Hair::Afro | Hair::Beanie(_) | Hair::Fringe | Hair::Ponytail);
    let f = Face {
        ears: if hidden_ears { 0.0 } else { 1.0 },
        jaw: if l.lashes { 0.26 } else { 0.2 },
        ..Face::new(l.skin)
    };
    let hair = rgb(l.hair_color);
    let top = rgb(l.top);
    // Behind the head.
    match l.hair {
        Hair::Long => {
            p.oval(0.0, 0.12, 0.42, 0.5, shade(hair, -0.12));
            for (x, y) in [(-0.38, 0.36), (0.38, 0.36), (-0.32, 0.56), (0.32, 0.56)] {
                p.oval(x, y, 0.11, 0.13, shade(hair, -0.12));
            }
            p.oval(0.0, 0.25, 0.3, 0.38, shade(hair, -0.35));
        }
        Hair::Fringe => {
            // A bob down to the jaw.
            p.oval(0.0, 0.0, 0.4, 0.42, shade(hair, -0.12));
            p.fill(p.rect(-0.4, 0.0, 0.4, 0.3), shade(hair, -0.12));
        }
        Hair::Ponytail => {
            // The tail falls over the right shoulder.
            p.fill(p.oval_rot(0.3, 0.32, 0.1, 0.3, -0.25), shade(hair, -0.1));
        }
        Hair::Afro => {
            p.oval(0.0, -0.17, 0.5, 0.44, shade(hair, -0.08));
            if p.shows(22.0) {
                for (x, y) in
                    [(-0.38, -0.38), (0.0, -0.55), (0.38, -0.38), (-0.46, -0.1), (0.46, -0.1)]
                {
                    p.oval(x, y, 0.13, 0.12, shade(hair, -0.04));
                }
            }
        }
        _ => {}
    }
    torso(p, 0.74, top);
    neck(p, &f, 0.1);
    // A crew neck in a lighter shade, or a collar on a white shirt.
    if l.top.iter().all(|&v| v > 220) {
        collar(p, SHIRT_WHITE, 0.02);
    } else {
        let rib = p.arc(0.0, 0.53, 0.14, 0.075, 0.0, PI);
        p.stroke(&rib, 0.04, shade(top, 0.25));
    }
    head(p, &f);
    // The hair (or hat) over the head.
    match l.hair {
        Hair::Short => {
            short_hair(p, &f, hair, f.y - 0.2, 0.0);
            shine(p, &p.arc(0.0, -0.14, 0.26, 0.24, 1.15 * PI, 1.4 * PI), 0.025, hair);
        }
        Hair::Spiky => {
            short_hair(p, &f, hair, f.y - 0.19, 0.0);
            for (x, h) in [(-0.2, 0.12), (-0.08, 0.17), (0.05, 0.16), (0.18, 0.13)] {
                p.fill(
                    p.poly(&[(x - 0.08, -0.34), (x + 0.02, -0.38 - h), (x + 0.08, -0.33)]),
                    hair,
                );
            }
        }
        Hair::Long => {
            for s in [-1.0f32, 1.0] {
                p.fill(p.oval_rot(s * 0.29, 0.05, 0.075, 0.28, s * -0.12), hair);
            }
            let cap = p.oval_poly(0.0, -0.13, 0.35, 0.33);
            let cap = above(p, cap, (-0.4, -0.1), (0.4, -0.2));
            p.shaded(cap, (-0.04, -0.03), 0.15, hair);
            p.fill(p.poly(&[(0.3, -0.22), (0.33, -0.03), (0.04, -0.2)]), hair);
            shine(p, &p.arc(0.0, -0.12, 0.28, 0.25, 1.15 * PI, 1.45 * PI), 0.03, hair);
        }
        Hair::Afro => {
            let cap = p.oval_poly(0.0, -0.16, 0.36, 0.3);
            let cap = above(p, cap, (-0.4, -0.13), (0.4, -0.13));
            p.shaded(cap, (-0.03, -0.03), 0.12, hair);
        }
        Hair::Bun => {
            p.shaded(p.oval_poly(0.0, -0.5, 0.15, 0.12), (-0.03, -0.03), 0.15, hair);
            short_hair(p, &f, hair, f.y - 0.18, 0.06);
            shine(p, &p.arc(0.0, -0.14, 0.26, 0.24, 1.15 * PI, 1.4 * PI), 0.025, hair);
        }
        Hair::Ponytail => {
            short_hair(p, &f, hair, f.y - 0.2, 0.08);
            for s in [-1.0f32, 1.0] {
                p.fill(p.oval_rot(s * 0.29, -0.02, 0.06, 0.16, s * -0.1), hair);
            }
            shine(p, &p.arc(0.0, -0.14, 0.26, 0.24, 1.15 * PI, 1.4 * PI), 0.025, hair);
        }
        Hair::Curly => {
            short_hair(p, &f, hair, f.y - 0.2, 0.0);
            for (x, y) in [
                (-0.26, -0.22),
                (-0.15, -0.33),
                (0.0, -0.37),
                (0.15, -0.33),
                (0.26, -0.22),
                (-0.07, -0.27),
                (0.08, -0.27),
            ] {
                p.oval(x, y, 0.085, 0.075, shade(hair, if x < 0.0 { 0.06 } else { -0.04 }));
            }
        }
        Hair::Mohawk => {
            // Shaved sides: a shadow of hair, and the crest.
            let cap = f.poly(p, 0.0, -0.012, 0.01);
            let cap = above(p, cap, (-0.5, f.y - 0.05), (0.5, f.y - 0.05));
            fill(&cap, p.col(with_alpha(shade(f.skin, -0.35), 0.6)));
            let crest =
                p.poly(&[(-0.07, -0.3), (-0.05, -0.56), (0.0, -0.62), (0.06, -0.55), (0.07, -0.3)]);
            p.shaded(crest, (-0.02, -0.02), 0.15, hair);
        }
        Hair::Fringe => {
            for s in [-1.0f32, 1.0] {
                p.fill(p.oval_rot(s * 0.3, 0.08, 0.07, 0.26, s * -0.06), hair);
            }
            let cap = p.oval_poly(0.0, -0.14, 0.36, 0.32);
            let cap = above(p, cap, (-0.4, -0.13), (0.4, -0.13));
            p.shaded(cap, (-0.04, -0.03), 0.14, hair);
            if p.shows(22.0) {
                for k in 0..6 {
                    let x = -0.25 + 0.1 * k as f32;
                    p.line((x, -0.2), (x + 0.01, -0.13), 0.01, shade(hair, -0.3));
                }
            }
            shine(p, &p.arc(0.0, -0.16, 0.26, 0.24, 1.15 * PI, 1.4 * PI), 0.025, hair);
        }
        Hair::Bald => {
            if p.shows(22.0) {
                p.oval(-0.1, f.y - 0.24, 0.09, 0.04, with_alpha(shade(f.skin, 0.35), 0.6));
            }
        }
        Hair::Cap(c) => {
            let cap = rgb(c);
            sideburns(p, &f, hair, -0.15, 0.2, 0.05);
            let crown = p.oval_poly(0.0, -0.2, 0.34, 0.3);
            let crown = above(p, crown, (-0.4, -0.17), (0.4, -0.2));
            p.shaded(crown, (-0.05, -0.03), 0.18, cap);
            p.oval(0.0, -0.49, 0.035, 0.022, shade(cap, -0.25));
            let brim_l = (-0.38, -0.17);
            let brim_r = (0.36, -0.2);
            let mut brim = p.quad(brim_r, (0.0, -0.08), brim_l);
            brim.extend(p.quad(brim_l, (0.0, -0.21), brim_r).into_iter().skip(1));
            brim.pop();
            p.fill(brim, shade(cap, -0.1));
        }
        Hair::Beanie(c) => {
            let knit = rgb(c);
            let hat = p.oval_poly(0.0, -0.2, 0.35, 0.33);
            let hat = above(p, hat, (-0.4, -0.14), (0.4, -0.14));
            p.shaded(hat, (-0.04, -0.03), 0.16, knit);
            p.fill(p.round_rect(-0.35, -0.22, 0.35, -0.1, 0.05), shade(knit, -0.12));
            if p.shows(22.0) {
                for k in 0..7 {
                    let x = -0.3 + 0.1 * k as f32;
                    p.line((x, -0.2), (x, -0.12), 0.012, shade(knit, -0.28));
                }
            }
            p.oval(0.0, -0.54, 0.07, 0.06, shade(knit, 0.12));
        }
    }
    // A short beard along the jaw.
    if l.beard {
        let jaw = f.poly(p, 0.0, 0.0, 0.025);
        let jaw = below(p, jaw, (-0.5, f.y + 0.1), (0.5, f.y + 0.1));
        p.shaded(jaw, (-0.03, -0.02), 0.15, shade(hair, 0.05));
        // The lips show through.
        p.oval(0.0, f.y + 0.245, 0.09, 0.045, shade(f.skin, -0.05));
    }
    let eye = Eye { lashes: l.lashes, ..Eye::new(l.iris) };
    eyes(p, &f, &eye);
    let brow_c = if matches!(l.hair, Hair::Bald) { rgb([40, 30, 26]) } else { shade(hair, -0.15) };
    brows(p, &f, -0.04, -0.03, 0.025, if l.lashes { 0.024 } else { 0.034 }, brow_c);
    nose(p, &f, if l.lashes { 0.85 } else { 1.0 });
    if l.lashes {
        blush(p, &f, 1.0);
    }
    if l.grin {
        grin(p, &f, 0.0, f.y + 0.235, 0.1, 0.05, 0.0, 0.35, false);
    } else {
        smile(p, 0.0, f.y + 0.245, 0.08, 0.025, 0.0, 0.028, rgb([120, 55, 50]));
    }
    if l.glasses {
        let frame = rgb([30, 28, 30]);
        for s in [-1.0f32, 1.0] {
            p.ring(s * 0.125, f.y + 0.06, 0.085, 0.02, frame);
        }
        p.line((-0.045, f.y + 0.05), (0.045, f.y + 0.05), 0.02, frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_avatar_has_a_picture() {
        for code in snook_legend_proto::AVATARS {
            // A person, or a thing of the game.
            assert!(look_of(code).is_some() != code.starts_with("pic_"), "{code}");
        }
        assert!(look_of("").is_none());
    }
}
