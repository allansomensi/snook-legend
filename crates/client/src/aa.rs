//! Anti-aliased shapes: every edge gets a fringe one pixel wide that fades
//! from the shape's colour to clear (as Dear ImGui and egui draw), so
//! rounded corners, outlines and circles are smooth whatever the graphics
//! card's anti-aliasing and the interface size. Fills and fringes never
//! overlap, so translucent shapes blend evenly.

use std::cell::RefCell;

use macroquad::models::Vertex;
use macroquad::prelude::*;

/// Vertices, indices, a path and its normals.
type Buffers = (Vec<Vertex>, Vec<u16>, Vec<Vec2>, Vec<Vec2>);

thread_local! {
    /// Reused buffers (hundreds of shapes are drawn a frame).
    static BUFFERS: RefCell<Buffers> =
        const { RefCell::new((Vec::new(), Vec::new(), Vec::new(), Vec::new())) };
}

/// One screen pixel, in the units being drawn in (the current camera's).
pub fn pixel() -> f32 {
    let proj = unsafe { get_internal_gl() }.quad_gl.get_projection_matrix();
    // An orthographic camera: units to pixels is the same everywhere.
    let ortho = proj.z_axis.w == 0.0 && proj.w_axis.w == 1.0;
    let per_unit = proj.x_axis.x.abs() * screen_width() / 2.0;
    if ortho && per_unit.is_finite() && per_unit > 1e-4 {
        1.0 / per_unit
    } else {
        1.0 / crate::ui::ui_scale()
    }
}

fn vertex(p: Vec2, c: Color) -> Vertex {
    Vertex::new(p.x, p.y, 0.0, 0.0, 0.0, c)
}

fn clear(c: Color) -> Color {
    Color::new(c.r, c.g, c.b, 0.0)
}

/// `pts` without repeated points (a corner of radius 0 gives several).
fn clean(pts: &[Vec2], closed: bool, out: &mut Vec<Vec2>) {
    out.clear();
    for &p in pts {
        if out.last().is_none_or(|q| q.distance_squared(p) > 1e-8) {
            out.push(p);
        }
    }
    if closed && out.len() > 1 && out[0].distance_squared(out[out.len() - 1]) <= 1e-8 {
        out.pop();
    }
}

/// The normal at each point of a path, scaled so the offset along each
/// edge stays the same at corners (a miter, kept within bounds).
fn normals(pts: &[Vec2], closed: bool, out: &mut Vec<Vec2>) {
    out.clear();
    let n = pts.len();
    let edge = |i: usize| {
        let d = (pts[(i + 1) % n] - pts[i]).normalize_or_zero();
        vec2(d.y, -d.x)
    };
    for i in 0..n {
        let (a, b) = if closed {
            (edge((i + n - 1) % n), edge(i))
        } else if i == 0 {
            (edge(0), edge(0))
        } else if i == n - 1 {
            (edge(n - 2), edge(n - 2))
        } else {
            (edge(i - 1), edge(i))
        };
        let m = (a + b) * 0.5;
        let d2 = m.length_squared();
        out.push(if d2 > 1e-6 { m * (1.0 / d2).min(16.0) } else { a });
    }
}

fn submit(vertices: &[Vertex], indices: &[u16]) {
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.texture(None);
    gl.draw_mode(DrawMode::Triangles);
    gl.geometry(vertices, indices);
}

/// Fills the convex polygon `pts`, each point's colour from `color`
/// (already faded by the caller's opacity when wanted).
pub fn fill_convex(pts: &[Vec2], color: impl Fn(Vec2) -> Color) {
    BUFFERS.with(|b| {
        let (vertices, indices, path, norms) = &mut *b.borrow_mut();
        clean(pts, true, path);
        let n = path.len();
        if n < 3 {
            return;
        }
        // Outward normals: flip them if the path goes the other way round.
        let area: f32 = (0..n).map(|i| path[i].perp_dot(path[(i + 1) % n])).sum();
        normals(path, true, norms);
        let out = if area > 0.0 { 1.0 } else { -1.0 } * 0.5 * pixel();
        vertices.clear();
        indices.clear();
        for i in 0..n {
            let c = color(path[i]);
            vertices.push(vertex(path[i] - norms[i] * out, c));
            vertices.push(vertex(path[i] + norms[i] * out, clear(c)));
        }
        for i in 1..n - 1 {
            indices.extend([0, (2 * i) as u16, (2 * (i + 1)) as u16]);
        }
        for i in 0..n {
            let j = (i + 1) % n;
            let (ii, io, ji, jo) =
                ((2 * i) as u16, (2 * i + 1) as u16, (2 * j) as u16, (2 * j + 1) as u16);
            indices.extend([ii, io, jo, ii, jo, ji]);
        }
        submit(vertices, indices);
    });
}

/// A line along `pts` (closed: back to the first), `width` wide, centred on
/// the path. Thinner than a pixel, it is a pixel wide and fainter.
pub fn stroke(pts: &[Vec2], closed: bool, width: f32, color: Color) {
    BUFFERS.with(|b| {
        let (vertices, indices, path, norms) = &mut *b.borrow_mut();
        clean(pts, closed, path);
        let n = path.len();
        if n < 2 {
            return;
        }
        normals(path, closed, norms);
        let px = pixel();
        let (core, color) = if width >= px {
            ((width - px) * 0.5, color)
        } else {
            (0.0, Color { a: color.a * width / px, ..color })
        };
        let edge = core + px;
        vertices.clear();
        indices.clear();
        for i in 0..n {
            let (p, m) = (path[i], norms[i]);
            vertices.push(vertex(p + m * edge, clear(color)));
            vertices.push(vertex(p + m * core, color));
            vertices.push(vertex(p - m * core, color));
            vertices.push(vertex(p - m * edge, clear(color)));
        }
        let segments = if closed { n } else { n - 1 };
        for i in 0..segments {
            let j = (i + 1) % n;
            let (a, b) = ((4 * i) as u16, (4 * j) as u16);
            for k in 0..3 {
                indices.extend([a + k, a + k + 1, b + k + 1, a + k, b + k + 1, b + k]);
            }
        }
        submit(vertices, indices);
    });
}

/// How many sides a circle of radius `r` needs to look round at this size.
fn sides(r: f32) -> usize {
    ((r / pixel()).max(0.0).sqrt() * 5.0).clamp(10.0, 160.0) as usize
}

/// The points of an ellipse (a circle when `rx` = `ry`).
fn ellipse_points(c: Vec2, rx: f32, ry: f32, out: &mut Vec<Vec2>) {
    let n = sides(rx.max(ry));
    out.clear();
    out.extend((0..n).map(|k| {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        c + vec2(a.cos() * rx, a.sin() * ry)
    }));
}

thread_local! {
    static POINTS: RefCell<Vec<Vec2>> = const { RefCell::new(Vec::new()) };
}

pub fn circle(c: Vec2, r: f32, color: Color) {
    ellipse(c, r, r, color);
}

pub fn ellipse(c: Vec2, rx: f32, ry: f32, color: Color) {
    if rx <= 0.0 || ry <= 0.0 {
        return;
    }
    POINTS.with(|p| {
        let pts = &mut *p.borrow_mut();
        ellipse_points(c, rx, ry, pts);
        fill_convex(pts, |_| color);
    });
}

pub fn circle_lines(c: Vec2, r: f32, width: f32, color: Color) {
    if r <= 0.0 {
        return;
    }
    POINTS.with(|p| {
        let pts = &mut *p.borrow_mut();
        ellipse_points(c, r, r, pts);
        stroke(pts, true, width, color);
    });
}

pub fn line(a: Vec2, b: Vec2, width: f32, color: Color) {
    stroke(&[a, b], false, width, color);
}
