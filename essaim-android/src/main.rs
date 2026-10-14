//! ESSAIM — portage Android (Rust + macroquad) du jeu web.
//!
//! Un essaim de bestioles (boids de Craig W. Reynolds : séparation, alignement,
//! cohésion) traque la proie, c'est-à-dire ton doigt (ou la souris sur ordinateur).
//! Trace des murs de lumière qu'ils ne peuvent pas traverser, et referme une forme
//! autour d'eux pour les piéger.
//!
//! Commandes
//! - Android : un doigt déplace la proie. Pose un deuxième doigt n'importe où pour
//!   tracer avec le premier. Bouton Retour : pause.
//! - Ordinateur : la souris est la proie, clic gauche maintenu pour tracer, P / Échap : pause.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

// ---------------------------------------------------------------- palette
const ABYSS: Color = Color::new(0.016, 0.024, 0.059, 1.0); // #04060f
const BG_CENTER: Color = Color::new(0.075, 0.125, 0.298, 1.0); // #13204c
const BG_MID: Color = Color::new(0.039, 0.071, 0.188, 1.0); // #0a1230
const TEXT: Color = Color::new(0.894, 0.914, 1.0, 1.0); // #e4e9ff
const MUTED: Color = Color::new(0.553, 0.592, 0.78, 1.0); // #8d97c7
const WALL: Color = Color::new(0.435, 0.902, 1.0, 1.0); // #6fe6ff
const SWARM: Color = Color::new(1.0, 0.478, 0.302, 1.0); // #ff7a4d
const BRUTE: Color = Color::new(0.773, 0.42, 1.0, 1.0); // #c56bff
const CORE: Color = Color::new(1.0, 0.945, 0.761, 1.0); // #fff1c2
const DANGER: Color = Color::new(1.0, 0.302, 0.427, 1.0); // #ff4d6d
const PANEL: Color = Color::new(0.047, 0.07, 0.172, 0.86);
const PANEL_LINE: Color = Color::new(0.51, 0.667, 1.0, 0.18);

const STROKE_LIFE: f32 = 7.5;
const INK_COST: f32 = 0.075;
const STEP_PX: f32 = 9.0;
const MAX_CREATURES: usize = 260;
const IS_ANDROID: bool = cfg!(target_os = "android");

fn rgba(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}
fn rnd(a: f32, b: f32) -> f32 {
    rand::gen_range(a, b)
}

// ---------------------------------------------------------------- entités
#[derive(PartialEq, Eq, Clone, Copy)]
enum Kind {
    Mite,
    Brute,
}

struct Creature {
    pos: Vec2,
    vel: Vec2,
    kind: Kind,
    r: f32,
    stun: f32,
    wob: f32,
    dead: bool,
    heading: f32,
    hit_wall: f32,
    seed: f32,
}

struct Stroke {
    pts: Vec<Vec2>,
    age: f32,
    life: f32,
    closed: bool,
    flash: f32,
}

struct Particle {
    pos: Vec2,
    vel: Vec2,
    life: f32,
    max: f32,
    color: Color,
    size: f32,
}

struct Popup {
    pos: Vec2,
    txt: String,
    sub: String,
    age: f32,
    big: bool,
    dim: bool,
}

struct Fill {
    tris: Vec<Vec2>,
    age: f32,
}

struct Mote {
    x: f32,
    y: f32,
    z: f32,
    p: f32,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum State {
    Title,
    Play,
    Pause,
    Over,
}

/// Entrées de l'image courante, unifiées souris / tactile.
#[derive(Default)]
struct Input {
    pos: Vec2,
    down: bool,
    tap: Option<Vec2>,
}

struct Assets {
    title: Font,
    ui: Font,
    ui_bold: Font,
    glow: Texture2D,
    bg: Texture2D,
    vignette: Texture2D,
    additive: Material,
}

// ---------------------------------------------------------------- utilitaires géométriques
fn closest(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let l2 = ab.length_squared().max(1e-6);
    let u = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    a + ab * u
}

fn point_in_poly(p: Vec2, poly: &[Vec2]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn triangulate(poly: &[Vec2]) -> Vec<Vec2> {
    let flat: Vec<f64> = poly.iter().flat_map(|p| [p.x as f64, p.y as f64]).collect();
    match earcutr::earcut(&flat, &[], 2) {
        Ok(idx) => idx.iter().map(|&i| poly[i]).collect(),
        Err(_) => vec![],
    }
}

/// Polygone convexe rempli (éventail depuis le premier point).
fn fill_convex(pts: &[Vec2], color: Color) {
    for i in 1..pts.len().saturating_sub(1) {
        draw_triangle(pts[0], pts[i], pts[i + 1], color);
    }
}

fn rounded_rect_pts(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<Vec2> {
    let mut v = Vec::with_capacity(32);
    let corners = [(x + w - r, y + r, -PI / 2.0), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, PI / 2.0), (x + r, y + r, PI)];
    for (cx, cy, a0) in corners {
        for k in 0..=6 {
            let a = a0 + k as f32 / 6.0 * PI / 2.0;
            v.push(vec2(cx + a.cos() * r, cy + a.sin() * r));
        }
    }
    v
}

fn draw_panel(x: f32, y: f32, w: f32, h: f32, r: f32, fill: Color, line: Color) {
    let pts = rounded_rect_pts(x, y, w, h, r);
    let c = vec2(x + w / 2.0, y + h / 2.0);
    for i in 0..pts.len() {
        draw_triangle(c, pts[i], pts[(i + 1) % pts.len()], fill);
    }
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        draw_line(a.x, a.y, b.x, b.y, 1.2, line);
    }
}

// ---------------------------------------------------------------- textures générées
fn radial_texture(size: u16, stops: &[(f32, Color)]) -> Texture2D {
    let mut img = Image::gen_image_color(size, size, BLANK);
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size as u32 {
        for x in 0..size as u32 {
            let d = (((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt() / c).min(1.0);
            let mut col = stops[stops.len() - 1].1;
            for w in stops.windows(2) {
                if d >= w[0].0 && d <= w[1].0 {
                    let t = (d - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
                    col = mix(w[0].1, w[1].1, t);
                    break;
                }
            }
            img.set_pixel(x, y, col);
        }
    }
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Linear);
    t
}

const ADD_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;
const ADD_FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

async fn load_assets() -> Assets {
    let title = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/TiltWarp-Regular.ttf")).unwrap();
    let ui = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/ChakraPetch-SemiBold.ttf")).unwrap();
    let ui_bold = load_ttf_font_from_bytes(include_bytes!("../assets/fonts/ChakraPetch-Bold.ttf")).unwrap();
    let w = WHITE;
    let glow = radial_texture(64, &[(0.0, rgba(w, 0.9)), (0.25, rgba(w, 0.35)), (1.0, rgba(w, 0.0))]);
    let bg = radial_texture(256, &[(0.0, BG_CENTER), (0.55, BG_MID), (1.0, ABYSS)]);
    let vignette = radial_texture(256, &[(0.0, Color::new(0., 0., 0., 0.)), (0.56, Color::new(0., 0., 0., 0.)), (1.0, Color::new(0., 0., 0., 1.0))]);
    let additive = load_material(
        ShaderSource::Glsl { vertex: ADD_VERTEX, fragment: ADD_FRAGMENT },
        MaterialParams {
            pipeline_params: PipelineParams {
                color_blend: Some(BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::SourceAlpha), BlendFactor::One)),
                alpha_blend: Some(BlendState::new(Equation::Add, BlendFactor::Zero, BlendFactor::One)),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    Assets { title, ui, ui_bold, glow, bg, vignette, additive }
}

// ---------------------------------------------------------------- texte
#[allow(dead_code)]
#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

fn text(s: &str, x: f32, y: f32, size: f32, font: &Font, color: Color, align: Align) -> f32 {
    let fs = size.round().max(6.0) as u16;
    let dim = measure_text(s, Some(font), fs, 1.0);
    let x0 = match align {
        Align::Left => x,
        Align::Center => x - dim.width / 2.0,
        Align::Right => x - dim.width,
    };
    draw_text_ex(s, x0, y, TextParams { font: Some(font), font_size: fs, color, ..Default::default() });
    dim.width
}

/// Texte avec contour (popups, bannière), échelle animée via font_scale pour éviter
/// de rastériser une nouvelle taille de police à chaque image.
fn text_outlined(s: &str, x: f32, y: f32, size: u16, scale: f32, font: &Font, fill: Color, outline: Color, ow: f32) {
    let dim = measure_text(s, Some(font), size, scale);
    let x0 = x - dim.width / 2.0;
    let y0 = y + dim.offset_y / 2.0;
    let p = |c: Color| TextParams { font: Some(font), font_size: size, font_scale: scale, color: c, ..Default::default() };
    if ow > 0.0 {
        for k in 0..8 {
            let a = k as f32 / 8.0 * TAU;
            draw_text_ex(s, x0 + a.cos() * ow, y0 + a.sin() * ow, p(outline));
        }
    }
    draw_text_ex(s, x0, y0, p(fill));
}

fn wrap(s: &str, font: &Font, size: f32, max_w: f32) -> Vec<String> {
    let fs = size.round().max(6.0) as u16;
    let mut lines = vec![];
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let trial = if cur.is_empty() { word.to_string() } else { format!("{} {}", cur, word) };
        if measure_text(&trial, Some(font), fs, 1.0).width > max_w && !cur.is_empty() {
            lines.push(cur);
            cur = word.to_string();
        } else {
            cur = trial;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn fmt_time(t: f32) -> String {
    let s = t.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}
fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

// ---------------------------------------------------------------- record
fn best_path() -> String {
    if IS_ANDROID {
        let dir = "/data/data/fr.franchiruncap.essaim/files";
        let _ = std::fs::create_dir_all(dir);
        format!("{}/essaim_best.txt", dir)
    } else {
        "essaim_best.txt".to_string()
    }
}
fn load_best() -> u64 {
    std::fs::read_to_string(best_path()).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}
fn save_best(v: u64) {
    let _ = std::fs::write(best_path(), v.to_string());
}

// ---------------------------------------------------------------- jeu
struct Game {
    state: State,
    w: f32,
    h: f32,
    s: f32,
    t: f32,
    score: f32,
    kills: u32,
    wave: u32,
    next_wave: f32,
    best: u64,
    new_record: bool,
    shake: f32,
    hurt: f32,
    player: Vec2,
    hp: f32,
    ink: f32,
    inv: f32,
    pulse: f32,
    ptr: Vec2,
    down: bool,
    strokes: Vec<Stroke>,
    active: Option<usize>,
    creatures: Vec<Creature>,
    particles: Vec<Particle>,
    popups: Vec<Popup>,
    fills: Vec<Fill>,
    motes: Vec<Mote>,
    seg_grid: HashMap<i64, Vec<(Vec2, Vec2, usize)>>,
    demo_t: f32,
    wave_banner: Option<(String, String, f32)>,
    hint_t: f32,
    over_t: f32,
    // tactile
    primary: Option<u64>,
    secondary: Vec<u64>,
    touch_seen: bool,
}

const CELL: f32 = 48.0;
fn gkey(gx: i32, gy: i32) -> i64 {
    ((gx as i64) << 32) ^ (gy as i64 & 0xffff_ffff)
}

impl Game {
    fn new() -> Self {
        let mut g = Game {
            state: State::Title,
            w: 0.0,
            h: 0.0,
            s: 1.0,
            t: 0.0,
            score: 0.0,
            kills: 0,
            wave: 0,
            next_wave: 0.0,
            best: load_best(),
            new_record: false,
            shake: 0.0,
            hurt: 0.0,
            player: vec2(0.0, 0.0),
            hp: 100.0,
            ink: 100.0,
            inv: 0.0,
            pulse: 0.0,
            ptr: vec2(0.0, 0.0),
            down: false,
            strokes: vec![],
            active: None,
            creatures: vec![],
            particles: vec![],
            popups: vec![],
            fills: vec![],
            motes: vec![],
            seg_grid: HashMap::new(),
            demo_t: 0.0,
            wave_banner: None,
            hint_t: 0.0,
            over_t: 0.0,
            primary: None,
            secondary: vec![],
            touch_seen: false,
        };
        g.resize(screen_width(), screen_height());
        g.ptr = vec2(g.w / 2.0, g.h / 2.0);
        g.player = g.ptr;
        g.seed_demo();
        g
    }

    fn resize(&mut self, nw: f32, nh: f32) {
        if self.w > 0.0 && self.h > 0.0 {
            let (sx, sy) = (nw / self.w, nh / self.h);
            for c in &mut self.creatures {
                c.pos.x *= sx;
                c.pos.y *= sy;
            }
            self.player.x *= sx;
            self.player.y *= sy;
            self.ptr.x *= sx;
            self.ptr.y *= sy;
            self.strokes.clear();
            self.active = None;
        }
        self.w = nw;
        self.h = nh;
        let m = nw.min(nh);
        // Sur téléphone, l'échelle suit la taille de l'écran ; sur ordinateur on reprend celle du jeu web.
        self.s = if IS_ANDROID { (m / 600.0).clamp(0.7, 4.0) } else { (m / 800.0).clamp(0.7, 1.25) };
        let n = (nw * nh / (9000.0 * self.s * self.s)) as usize;
        self.motes = (0..n).map(|_| Mote { x: rnd(0.0, nw), y: rnd(0.0, nh), z: rnd(0.2, 1.0), p: rnd(0.0, TAU) }).collect();
        for c in &mut self.creatures {
            c.r = if c.kind == Kind::Brute { 10.0 } else { 5.5 } * self.s;
        }
    }

    fn make_creature(&self, pos: Vec2, kind: Kind) -> Creature {
        let a = rnd(0.0, TAU);
        Creature {
            pos,
            vel: vec2(a.cos(), a.sin()) * 40.0,
            kind,
            r: if kind == Kind::Brute { 10.0 } else { 5.5 } * self.s,
            stun: 0.0,
            wob: rnd(0.0, TAU),
            dead: false,
            heading: a,
            hit_wall: 0.0,
            seed: rnd(0.0, 1.0),
        }
    }

    fn seed_demo(&mut self) {
        self.creatures.clear();
        for i in 0..70 {
            let c = self.make_creature(vec2(rnd(0.0, self.w), rnd(0.0, self.h)), if i % 14 == 0 { Kind::Brute } else { Kind::Mite });
            self.creatures.push(c);
        }
    }

    fn spawn_group(&mut self, n: usize, kind: Kind) {
        let m = 40.0;
        let (cx, cy) = match rand::gen_range(0, 4) {
            0 => (rnd(0.0, self.w), -m),
            1 => (self.w + m, rnd(0.0, self.h)),
            2 => (rnd(0.0, self.w), self.h + m),
            _ => (-m, rnd(0.0, self.h)),
        };
        for _ in 0..n {
            let a = rnd(0.0, TAU);
            let r = rnd(0.0, 1.0f32).sqrt() * 60.0 * self.s;
            let c = self.make_creature(vec2(cx + a.cos() * r, cy + a.sin() * r), kind);
            self.creatures.push(c);
        }
    }

    fn max_speed(&self, c: &Creature) -> f32 {
        let base = if c.kind == Kind::Brute { 120.0 } else { 175.0 } * self.s;
        base * (1.0 + (self.wave.saturating_sub(1) as f32 * 0.06).min(0.6))
    }

    // ---------------- murs
    fn start_stroke(&mut self) {
        if self.ink < 3.0 {
            return;
        }
        self.strokes.push(Stroke { pts: vec![self.ptr], age: 0.0, life: STROKE_LIFE, closed: false, flash: 0.0 });
        self.active = Some(self.strokes.len() - 1);
    }

    fn extend_stroke(&mut self) {
        let Some(ai) = self.active else { return };
        let last = *self.strokes[ai].pts.last().unwrap();
        let d = self.ptr.distance(last);
        if d < STEP_PX * self.s {
            return;
        }
        let n = (d / (STEP_PX * self.s * 1.6)).ceil().max(1.0) as usize;
        for k in 1..=n {
            let cost = d / n as f32 * INK_COST / self.s;
            if self.ink < cost {
                self.ink = 0.0;
                self.end_stroke();
                return;
            }
            self.ink -= cost;
            let p = last + (self.ptr - last) * (k as f32 / n as f32);
            self.strokes[ai].pts.push(p);
        }
        // fermeture automatique : le trait recroise un de ses anciens points
        let tol = 16.0 * self.s;
        let pts = &self.strokes[ai].pts;
        let p = *pts.last().unwrap();
        if pts.len() > 10 {
            for i in 0..pts.len() - 10 {
                if pts[i].distance(p) < tol {
                    let poly = pts[i..].to_vec();
                    self.capture(poly, ai);
                    self.active = None;
                    if self.down {
                        self.start_stroke();
                    }
                    return;
                }
            }
        }
    }

    fn end_stroke(&mut self) {
        let Some(ai) = self.active else { return };
        self.active = None;
        let pts = &self.strokes[ai].pts;
        if pts.len() < 2 {
            self.strokes.remove(ai);
        } else if pts.len() >= 10 && pts[0].distance(*pts.last().unwrap()) < 48.0 * self.s {
            let poly = pts.clone();
            self.capture(poly, ai);
        }
    }

    fn capture(&mut self, poly: Vec<Vec2>, si: usize) {
        {
            let st = &mut self.strokes[si];
            st.closed = true;
            st.life = st.life.min(st.age + 1.6);
            st.flash = 1.0;
        }
        self.fills.push(Fill { tris: triangulate(&poly), age: 0.0 });
        let mut n = 0u32;
        let mut sum = Vec2::ZERO;
        let mut bursts = vec![];
        for c in &mut self.creatures {
            if !c.dead && point_in_poly(c.pos, &poly) {
                c.dead = true;
                n += 1;
                sum += c.pos;
                bursts.push((c.pos, c.kind));
            }
        }
        for (p, k) in bursts {
            if k == Kind::Brute {
                self.burst(p, BRUTE, 26, 1.4);
            } else {
                self.burst(p, SWARM, 14, 1.0);
            }
        }
        self.creatures.retain(|c| !c.dead);
        if n > 0 {
            let pts = (n as f32 * 25.0 * (1.0 + (n as f32 - 1.0) * 0.15)).round();
            self.score += pts;
            self.kills += n;
            self.hp = (self.hp + n as f32 * 0.8).min(100.0);
            self.popups.push(Popup {
                pos: sum / n as f32,
                txt: if n > 1 { format!("PIÉGÉS ×{}", n) } else { "PIÉGÉ".into() },
                sub: format!("+{}", pts as u64),
                age: 0.0,
                big: n >= 8,
                dim: false,
            });
            self.shake = self.shake.max((3.0 + n as f32 * 0.5).min(10.0));
        } else {
            let c = poly.iter().fold(Vec2::ZERO, |a, p| a + *p) / poly.len() as f32;
            self.popups.push(Popup { pos: c, txt: "VIDE".into(), sub: String::new(), age: 0.0, big: false, dim: true });
        }
    }

    fn build_seg_grid(&mut self) {
        for v in self.seg_grid.values_mut() {
            v.clear();
        }
        for (si, s) in self.strokes.iter().enumerate() {
            for w in s.pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                let x0 = (a.x.min(b.x) / CELL).floor() as i32;
                let x1 = (a.x.max(b.x) / CELL).floor() as i32;
                let y0 = (a.y.min(b.y) / CELL).floor() as i32;
                let y1 = (a.y.max(b.y) / CELL).floor() as i32;
                for gx in x0..=x1 {
                    for gy in y0..=y1 {
                        self.seg_grid.entry(gkey(gx, gy)).or_default().push((a, b, si));
                    }
                }
            }
        }
    }

    // ---------------- essaim (boids)
    fn step_swarm(&mut self, dt: f32, target: Vec2) {
        let s = self.s;
        let cc = 60.0 * s;
        let mut cgrid: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, c) in self.creatures.iter().enumerate() {
            cgrid.entry(gkey((c.pos.x / cc).floor() as i32, (c.pos.y / cc).floor() as i32)).or_default().push(i);
        }
        let vis2 = (60.0 * s).powi(2);
        let n = self.creatures.len();
        for i in 0..n {
            let c = &self.creatures[i];
            let max_s = self.max_speed(c);
            let (mut sep, mut ali, mut coh, mut cnt) = (Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, 0.0f32);
            let gx = (c.pos.x / cc).floor() as i32;
            let gy = (c.pos.y / cc).floor() as i32;
            let sep_r = c.r * 3.2 + 8.0 * s;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(arr) = cgrid.get(&gkey(gx + dx, gy + dy)) {
                        for &j in arr {
                            if j == i {
                                continue;
                            }
                            let o = &self.creatures[j];
                            let d = o.pos - c.pos;
                            let d2 = d.length_squared();
                            if d2 > vis2 {
                                continue;
                            }
                            ali += o.vel;
                            coh += d;
                            cnt += 1.0;
                            if d2 < sep_r * sep_r {
                                let dl = d2.sqrt().max(0.01);
                                sep -= d / dl * (1.0 - dl / sep_r);
                            }
                        }
                    }
                }
            }
            let mut f = sep * 2.2;
            if cnt > 0.0 {
                f += (ali / cnt - c.vel) / max_s * 0.45;
                f += coh.normalize_or_zero() * 0.25;
            }
            // poursuite de la proie
            let to = target - c.pos;
            let td = to.length().max(1.0);
            let seek = if c.stun > 0.0 { -0.6 } else { 1.15 };
            f += to / td * seek;
            // flottement
            let wob = c.wob;
            f += vec2((wob * 0.37 + c.seed * 9.0).cos(), (wob * 0.41 + c.seed * 7.0).sin()) * 0.18;
            // évitement anticipé des murs
            let look = 34.0 * s + c.r;
            let (wgx, wgy) = ((c.pos.x / CELL).floor() as i32, (c.pos.y / CELL).floor() as i32);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(arr) = self.seg_grid.get(&gkey(wgx + dx, wgy + dy)) {
                        for &(a, b, _) in arr {
                            let q = closest(c.pos, a, b);
                            let away = c.pos - q;
                            let d = away.length().max(0.01);
                            if d < look {
                                let k = (1.0 - d / look) * 2.4;
                                f += away / d * k;
                                let tg = (b - a).normalize_or_zero();
                                let dot = (to / td).dot(tg);
                                f += tg * dot.signum() * k * 0.6;
                            }
                        }
                    }
                }
            }
            let acc = max_s * 3.2;
            let c = &mut self.creatures[i];
            c.wob += dt * (6.0 + c.seed * 4.0);
            c.vel += f * acc * dt;
            let lim = if c.stun > 0.0 { max_s * 0.7 } else { max_s };
            let sp = c.vel.length();
            if sp > lim {
                c.vel *= lim / sp;
            }
            c.stun = (c.stun - dt).max(0.0);
            c.hit_wall = (c.hit_wall - dt).max(0.0);
        }
        // déplacement + collision dure : impossible de traverser un mur
        let wall_r = 4.0 * s;
        let mut sparks = vec![];
        let mut flashes = vec![];
        let (w, h) = (self.w, self.h);
        for c in &mut self.creatures {
            c.pos += c.vel * dt;
            let (gx, gy) = ((c.pos.x / CELL).floor() as i32, (c.pos.y / CELL).floor() as i32);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(arr) = self.seg_grid.get(&gkey(gx + dx, gy + dy)) {
                        for &(a, b, si) in arr {
                            let q = closest(c.pos, a, b);
                            let mut d = c.pos - q;
                            let mut dl = d.length();
                            let min = c.r + wall_r;
                            if dl < min {
                                if dl < 0.001 {
                                    d = vec2(-(b.y - a.y), b.x - a.x);
                                    dl = d.length().max(1.0);
                                }
                                let nrm = d / dl;
                                c.pos = q + nrm * min;
                                let vn = c.vel.dot(nrm);
                                if vn < 0.0 {
                                    c.vel -= nrm * vn * 1.3;
                                }
                                if c.hit_wall <= 0.0 && rnd(0.0, 1.0) < 0.25 {
                                    c.hit_wall = 0.25;
                                    sparks.push((q, nrm));
                                    flashes.push(si);
                                }
                            }
                        }
                    }
                }
            }
            c.pos.x = c.pos.x.clamp(-80.0, w + 80.0);
            c.pos.y = c.pos.y.clamp(-80.0, h + 80.0);
            let th = c.vel.y.atan2(c.vel.x);
            let mut dh = th - c.heading;
            dh = dh.sin().atan2(dh.cos());
            c.heading += dh * (dt * 10.0).min(1.0);
        }
        for (q, n) in sparks {
            for _ in 0..4 {
                let a = n.y.atan2(n.x) + rnd(-1.0, 1.0);
                let sp = rnd(60.0, 180.0) * s;
                let life = rnd(0.15, 0.35);
                self.particles.push(Particle { pos: q, vel: vec2(a.cos(), a.sin()) * sp, life, max: 0.35, color: Color::new(0.75, 0.965, 1.0, 1.0), size: 1.6 * s });
            }
        }
        for si in flashes {
            if let Some(st) = self.strokes.get_mut(si) {
                st.flash = st.flash.max(0.35);
            }
        }
    }

    fn burst(&mut self, p: Vec2, color: Color, n: usize, sz: f32) {
        for _ in 0..n {
            let a = rnd(0.0, TAU);
            let sp = rnd(40.0, 260.0) * self.s;
            self.particles.push(Particle { pos: p, vel: vec2(a.cos(), a.sin()) * sp, life: rnd(0.4, 0.9), max: 0.9, color, size: rnd(1.5, 3.5) * self.s * sz });
        }
    }

    // ---------------- boucle
    fn update(&mut self, dt: f32) {
        self.t += dt;
        if self.t >= self.next_wave {
            self.wave += 1;
            self.next_wave = self.t + 22.0;
            let n = (14 + self.wave as usize * 7).min(48);
            let groups = 1 + (self.wave as usize / 2).min(3);
            for _ in 0..groups {
                self.spawn_group(((n as f32) / groups as f32).round() as usize, Kind::Mite);
            }
            if self.wave >= 3 {
                for _ in 0..(self.wave as usize - 2).min(4) {
                    self.spawn_group(2, Kind::Brute);
                }
            }
            if self.creatures.len() > MAX_CREATURES {
                let extra = self.creatures.len() - MAX_CREATURES;
                self.creatures.drain(0..extra);
            }
            let sub = match self.wave {
                1 => "ils arrivent",
                3 => "des colosses rejoignent l’essaim",
                _ => "l’essaim grossit",
            };
            self.wave_banner = Some((format!("VAGUE {}", self.wave), sub.to_uppercase(), 0.0));
        }
        self.score += dt * 10.0;
        self.player = self.ptr;
        if self.down {
            self.extend_stroke();
        } else {
            self.ink = (self.ink + dt * 16.0).min(100.0);
        }
        let active = self.active;
        for (i, st) in self.strokes.iter_mut().enumerate() {
            if Some(i) != active {
                st.age += dt;
            }
            st.flash = (st.flash - dt * 2.0).max(0.0);
        }
        // retire les murs expirés en conservant l'indice du trait actif
        let mut i = 0;
        while i < self.strokes.len() {
            let st = &self.strokes[i];
            if Some(i) != self.active && st.age >= st.life {
                self.strokes.remove(i);
                if let Some(a) = self.active {
                    if a > i {
                        self.active = Some(a - 1);
                    }
                }
            } else {
                i += 1;
            }
        }
        self.build_seg_grid();
        for _ in 0..2 {
            let p = self.player;
            self.step_swarm(dt / 2.0, p);
        }
        // contacts avec la proie
        self.inv = (self.inv - dt).max(0.0);
        let pr = 11.0 * self.s;
        let mut hits = vec![];
        for (i, c) in self.creatures.iter().enumerate() {
            let d = c.pos.distance(self.player);
            if d < pr + c.r && c.stun <= 0.0 {
                hits.push(i);
            }
        }
        for i in hits {
            let kind = self.creatures[i].kind;
            let ms = self.max_speed(&self.creatures[i]);
            if self.inv <= 0.0 {
                self.hp -= if kind == Kind::Brute { 13.0 } else { 5.0 };
                self.inv = 0.12;
                self.hurt = 1.0;
                self.shake = self.shake.max(if kind == Kind::Brute { 9.0 } else { 4.0 });
                let p = self.player;
                self.burst(p, DANGER, 8, 1.0);
            }
            let c = &mut self.creatures[i];
            c.stun = 0.8;
            let dir = (c.pos - self.player).normalize_or_zero();
            c.vel = if dir == Vec2::ZERO { vec2(ms, 0.0) } else { dir * ms };
        }
        if self.hp <= 0.0 {
            self.hp = 0.0;
            self.game_over();
        }
    }

    fn update_fx(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.pos += p.vel * dt;
            p.vel *= 1.0 - 3.0 * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for p in &mut self.popups {
            p.age += dt;
        }
        self.popups.retain(|p| p.age < 1.3);
        for f in &mut self.fills {
            f.age += dt;
        }
        self.fills.retain(|f| f.age < 0.7);
        if let Some(b) = &mut self.wave_banner {
            b.2 += dt;
            if b.2 > 2.2 {
                self.wave_banner = None;
            }
        }
        self.shake = (self.shake - dt * 30.0).max(0.0);
        self.hurt = (self.hurt - dt * 2.5).max(0.0);
        self.pulse += dt;
        self.hint_t += dt;
    }

    // ---------------- états
    fn new_game(&mut self) {
        self.state = State::Play;
        self.t = 0.0;
        self.score = 0.0;
        self.kills = 0;
        self.wave = 0;
        self.next_wave = 0.6;
        self.hp = 100.0;
        self.ink = 100.0;
        self.inv = 0.0;
        self.strokes.clear();
        self.active = None;
        self.creatures.clear();
        self.particles.clear();
        self.popups.clear();
        self.fills.clear();
        self.wave_banner = None;
        self.hint_t = 0.0;
        self.new_record = false;
        self.down = false;
    }

    fn pause(&mut self, on: bool) {
        if self.state != State::Play && self.state != State::Pause {
            return;
        }
        self.state = if on { State::Pause } else { State::Play };
        if on {
            self.down = false;
            self.end_stroke();
        }
    }

    fn game_over(&mut self) {
        self.state = State::Over;
        self.end_stroke();
        self.down = false;
        let p = self.player;
        self.burst(p, CORE, 40, 1.4);
        self.burst(p, DANGER, 30, 1.2);
        self.shake = 14.0;
        let sc = self.score.floor() as u64;
        self.new_record = sc > self.best;
        if self.new_record {
            self.best = sc;
            save_best(sc);
        }
        self.over_t = 0.0;
    }

    fn to_title(&mut self) {
        self.state = State::Title;
        self.strokes.clear();
        self.active = None;
        self.seed_demo();
    }

    // ---------------- entrées
    fn read_input(&mut self) -> Input {
        let mut inp = Input { pos: self.ptr, down: false, tap: None };
        let ts = touches();
        if !ts.is_empty() {
            self.touch_seen = true;
        }
        if self.touch_seen {
            for t in &ts {
                match t.phase {
                    TouchPhase::Started => {
                        if self.primary.is_none() {
                            self.primary = Some(t.id);
                            self.ptr = t.position;
                            inp.tap = Some(t.position);
                        } else if self.primary != Some(t.id) && !self.secondary.contains(&t.id) {
                            self.secondary.push(t.id);
                        }
                    }
                    TouchPhase::Moved | TouchPhase::Stationary => {
                        if self.primary == Some(t.id) {
                            self.ptr = t.position;
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        if self.primary == Some(t.id) {
                            self.primary = None;
                            // le doigt secondaire devient le doigt principal
                            if !self.secondary.is_empty() {
                                self.primary = Some(self.secondary.remove(0));
                            }
                        } else {
                            self.secondary.retain(|&i| i != t.id);
                        }
                    }
                }
            }
            // nettoie les identifiants disparus
            let alive: Vec<u64> = ts.iter().map(|t| t.id).collect();
            if let Some(p) = self.primary {
                if !alive.contains(&p) && !ts.is_empty() {
                    self.primary = None;
                }
            }
            if ts.is_empty() {
                self.primary = None;
                self.secondary.clear();
            }
            self.secondary.retain(|i| alive.contains(i));
            inp.down = self.primary.is_some() && !self.secondary.is_empty();
        } else {
            let (mx, my) = mouse_position();
            self.ptr = vec2(mx, my);
            inp.down = is_mouse_button_down(MouseButton::Left);
            if is_mouse_button_pressed(MouseButton::Left) {
                inp.tap = Some(self.ptr);
            }
        }
        inp.pos = self.ptr;
        inp
    }
}

// ---------------------------------------------------------------- rendu
struct Ui {
    s: f32,
}

fn hit(r: Rect, p: Option<Vec2>) -> bool {
    p.map(|p| r.contains(p)).unwrap_or(false)
}

fn draw_glow(a: &Assets, x: f32, y: f32, size: f32, color: Color) {
    draw_texture_ex(&a.glow, x - size / 2.0, y - size / 2.0, color, DrawTextureParams { dest_size: Some(vec2(size, size)), ..Default::default() });
}

fn render_world(g: &Game, a: &Assets) {
    let (w, h, s, t) = (g.w, g.h, g.s, g.t + g.demo_t);
    clear_background(ABYSS);
    // fond : dégradé radial
    let r = (w * w + h * h).sqrt() * 0.6;
    let (cx, cy) = (w * 0.5, h * 0.45);
    draw_texture_ex(&a.bg, cx - r, cy - r, WHITE, DrawTextureParams { dest_size: Some(vec2(2.0 * r, 2.0 * r)), ..Default::default() });
    // trame hexagonale
    let hs = 34.0 * s;
    let hh = hs * 3f32.sqrt() / 2.0;
    let dot = Color::new(0.55, 0.67, 1.0, 0.07);
    let ds = (1.6 * s).max(1.5);
    let mut y = 0.0;
    let mut row = 0;
    while y < h + hh {
        let mut x = if row % 2 == 1 { hs / 2.0 } else { 0.0 };
        while x < w + hs {
            draw_rectangle(x - ds / 2.0, y - ds / 2.0, ds, ds, dot);
            x += hs;
        }
        y += hh;
        row += 1;
    }
    let (sx, sy) = if g.shake > 0.0 { (rnd(-g.shake, g.shake), rnd(-g.shake, g.shake)) } else { (0.0, 0.0) };
    let o = vec2(sx, sy);
    // poussière en suspension
    for m in &g.motes {
        let yy = (m.y + t * 8.0 * m.z * s).rem_euclid(h);
        let xx = m.x + (m.p + t * 0.3 * m.z).sin() * 10.0 * s;
        let sz = 1.4 * m.z * s;
        draw_rectangle(xx + sx, yy + sy, sz, sz, Color::new(0.67, 0.78, 1.0, (0.15 + 0.35 * m.z) * 0.35));
    }
    // formes refermées
    for f in &g.fills {
        let k = f.age / 0.7;
        let col = rgba(WALL, 0.28 * (1.0 - k));
        for tri in f.tris.chunks(3) {
            if tri.len() == 3 {
                draw_triangle(tri[0] + o, tri[1] + o, tri[2] + o, col);
            }
        }
    }
    // murs : halo additif
    gl_use_material(&a.additive);
    for (i, st) in g.strokes.iter().enumerate() {
        let al = stroke_alpha(g, st, i, t);
        let step = (st.pts.len() / 200).max(1);
        for (k, p) in st.pts.iter().enumerate() {
            if k % step == 0 {
                draw_glow(a, p.x + sx, p.y + sy, 26.0 * s, Color::new(0.24, 0.7, 1.0, 0.16 * al));
            }
        }
    }
    gl_use_default_material();
    // murs : tube + cœur
    for (i, st) in g.strokes.iter().enumerate() {
        if st.pts.len() < 2 {
            continue;
        }
        let al = stroke_alpha(g, st, i, t);
        let mid = mix(Color::new(0.06, 0.2, 0.35, 1.0), WALL, 0.55 + st.flash * 0.4);
        let mid = rgba(mid, al);
        let core = Color::new(0.92, 0.99, 1.0, 0.9 * al);
        let th = 8.0 * s;
        for wpts in st.pts.windows(2) {
            draw_line(wpts[0].x + sx, wpts[0].y + sy, wpts[1].x + sx, wpts[1].y + sy, th, mid);
        }
        for p in &st.pts {
            draw_circle(p.x + sx, p.y + sy, th / 2.0, mid);
        }
        for wpts in st.pts.windows(2) {
            draw_line(wpts[0].x + sx, wpts[0].y + sy, wpts[1].x + sx, wpts[1].y + sy, (1.8 * s).max(1.5), core);
        }
    }
    // essaim : halos
    gl_use_material(&a.additive);
    for c in &g.creatures {
        let (col, k) = if c.kind == Kind::Brute { (BRUTE, 5.0) } else { (SWARM, 6.0) };
        let al = if c.stun > 0.0 { 0.5 } else { 0.85 };
        draw_glow(a, c.pos.x + sx, c.pos.y + sy, c.r * k, rgba(col, al));
    }
    gl_use_default_material();
    // essaim : corps
    for c in &g.creatures {
        draw_creature(c, o);
    }
    // proie
    let p = g.player + o;
    let pr = 11.0 * s;
    let pul = 1.0 + (g.pulse * 4.0).sin() * 0.06;
    gl_use_material(&a.additive);
    let gs = pr * 9.0 * pul;
    let base_a = if g.state == State::Title { 0.6 } else { 1.0 };
    draw_glow(a, p.x, p.y, gs, rgba(CORE, base_a));
    draw_glow(a, p.x, p.y, gs * 0.45, rgba(CORE, base_a * 0.8));
    if g.hurt > 0.0 {
        draw_glow(a, p.x, p.y, gs, rgba(DANGER, g.hurt));
    }
    gl_use_default_material();
    draw_circle(p.x, p.y, pr * 0.5 * pul, Color::new(1.0, 0.973, 0.882, 1.0));
    if g.state == State::Play || g.state == State::Pause {
        let ring = pr * 1.6;
        draw_arc(p.x, p.y, 48, ring, 0.0, 3.0 * s.max(1.0) * 0.8, 360.0, Color::new(1.0, 1.0, 1.0, 0.1));
        let hc = if g.hp > 35.0 { CORE } else { DANGER };
        if g.hp > 0.0 {
            draw_arc(p.x, p.y, 48, ring, -90.0, 3.0 * s.max(1.0) * 0.8, 360.0 * g.hp / 100.0, hc);
        }
        if g.ink > 0.5 {
            draw_arc(p.x, p.y, 48, pr * 2.15, -90.0, 2.0 * s.max(1.0) * 0.8, 360.0 * g.ink / 100.0, rgba(WALL, 0.85));
        }
    }
    // particules
    gl_use_material(&a.additive);
    for q in &g.particles {
        let al = (q.life / q.max).clamp(0.0, 1.0);
        draw_circle(q.pos.x + sx, q.pos.y + sy, q.size, rgba(q.color, al));
    }
    gl_use_default_material();
    // popups
    for pp in &g.popups {
        let k = pp.age / 1.3;
        let yy = pp.pos.y - k * 40.0 * s + sy;
        let sc = if k < 0.12 { 0.5 + k / 0.12 * 0.6 } else { 1.1 - k * 0.1 };
        let al = 1.0 - ((k - 0.65).max(0.0) / 0.35);
        let size = ((if pp.big { 34.0 } else { 24.0 }) * s) as u16;
        let fill = if pp.dim { MUTED } else { Color::new(0.75, 0.965, 1.0, 1.0) };
        text_outlined(&pp.txt, pp.pos.x + sx, yy, size, sc, &a.title, rgba(fill, al), Color::new(0.016, 0.024, 0.06, 0.85 * al), 2.5 * s);
        if !pp.sub.is_empty() {
            text_outlined(&pp.sub, pp.pos.x + sx, yy + 31.0 * s * sc, (17.0 * s) as u16, sc, &a.ui_bold, rgba(CORE, al), Color::new(0.016, 0.024, 0.06, 0.85 * al), 2.0 * s);
        }
    }
    // vignette + voile rouge
    let vr = (w * w + h * h).sqrt() * 0.62;
    draw_texture_ex(&a.vignette, w / 2.0 - vr, h / 2.0 - vr, Color::new(0.0, 0.0, 0.0, 0.6), DrawTextureParams { dest_size: Some(vec2(2.0 * vr, 2.0 * vr)), ..Default::default() });
    let low = if g.state == State::Play && g.hp < 30.0 { (0.25 + 0.15 * (g.t * 6.0).sin()) * (1.0 - g.hp / 30.0) } else { 0.0 };
    let red = (g.hurt * 0.45).max(low);
    if red > 0.0 {
        draw_texture_ex(&a.vignette, w / 2.0 - vr, h / 2.0 - vr, Color::new(1.0, 0.16, 0.31, red), DrawTextureParams { dest_size: Some(vec2(2.0 * vr, 2.0 * vr)), ..Default::default() });
    }
}

fn stroke_alpha(g: &Game, st: &Stroke, i: usize, t: f32) -> f32 {
    let rem = if Some(i) == g.active { 99.0 } else { st.life - st.age };
    let mut a = if rem < 1.5 { rem / 1.5 } else { 1.0 };
    if rem < 1.5 && !st.closed {
        a *= 0.65 + 0.35 * (t * 30.0).sin();
    }
    a.clamp(0.0, 1.0)
}

fn draw_creature(c: &Creature, o: Vec2) {
    let r = c.r;
    let (sn, cs) = c.heading.sin_cos();
    let tf = |x: f32, y: f32| vec2(c.pos.x + o.x + x * cs - y * sn, c.pos.y + o.y + x * sn + y * cs);
    let brute = c.kind == Kind::Brute;
    let leg = (c.wob * 2.2).sin() * 0.5;
    let leg_col = if brute { Color::new(0.86, 0.67, 1.0, 0.75) } else { Color::new(1.0, 0.745, 0.59, 0.7) };
    let lw = (r * 0.18).max(1.0);
    for sg in [-1.0f32, 1.0] {
        for k in [-0.4f32, 0.25] {
            let lx = r * k;
            let sw = if k > 0.0 { leg } else { -leg } * sg;
            let a = tf(lx, 0.0);
            let b = tf(lx - r * 0.35 + sw * r * 0.4, sg * r * 1.25);
            draw_line(a.x, a.y, b.x, b.y, lw, leg_col);
        }
    }
    // corps en goutte (courbes de Bézier quadratiques échantillonnées)
    let quad = |p0: Vec2, p1: Vec2, p2: Vec2, t: f32| p0 * (1.0 - t) * (1.0 - t) + p1 * 2.0 * (1.0 - t) * t + p2 * t * t;
    let mut pts = vec![];
    let segs = [
        (vec2(1.25, 0.0), vec2(0.4, -0.95), vec2(-1.1, -0.45)),
        (vec2(-1.1, -0.45), vec2(-1.5, 0.0), vec2(-1.1, 0.45)),
        (vec2(-1.1, 0.45), vec2(0.4, 0.95), vec2(1.25, 0.0)),
    ];
    for (p0, p1, p2) in segs {
        for k in 0..5 {
            let q = quad(p0, p1, p2, k as f32 / 5.0) * r;
            pts.push(tf(q.x, q.y));
        }
    }
    let body = if brute { Color::new(0.482, 0.184, 0.722, 1.0) } else { Color::new(0.761, 0.255, 0.165, 1.0) };
    let centre = tf(-0.1 * r, 0.0);
    for i in 0..pts.len() {
        draw_triangle(centre, pts[i], pts[(i + 1) % pts.len()], body);
    }
    // tête
    let head = if brute { Color::new(0.85, 0.65, 1.0, 1.0) } else { Color::new(1.0, 0.698, 0.478, 1.0) };
    let hp: Vec<Vec2> = (0..12).map(|k| {
        let a = k as f32 / 12.0 * TAU;
        tf(r * 0.35 + a.cos() * r * 0.55, a.sin() * r * 0.42)
    }).collect();
    fill_convex(&hp, head);
    // yeux
    let eye = if c.stun > 0.0 { WHITE } else { Color::new(1.0, 0.965, 0.878, 1.0) };
    for sg in [-1.0f32, 1.0] {
        let e = tf(r * 0.8, sg * r * 0.22);
        draw_circle(e.x, e.y, r * 0.16, eye);
    }
}

// ---------------------------------------------------------------- interface
fn draw_label(s: &str, x: f32, y: f32, ui: &Ui, a: &Assets, align: Align) {
    // petites capitales espacées, lettre par lettre
    let size = (10.5 * ui.s).round().max(6.0) as u16;
    let sp = 2.2 * ui.s;
    let widths: Vec<f32> = s.chars().map(|c| measure_text(&c.to_string(), Some(&a.ui_bold), size, 1.0).width).collect();
    let total: f32 = widths.iter().sum::<f32>() + sp * (widths.len().saturating_sub(1)) as f32;
    let mut cx = match align {
        Align::Left => x,
        Align::Center => x - total / 2.0,
        Align::Right => x - total,
    };
    for (c, w) in s.chars().zip(widths) {
        if c != ' ' {
            draw_text_ex(&c.to_string(), cx, y, TextParams { font: Some(&a.ui_bold), font_size: size, color: MUTED, ..Default::default() });
        }
        cx += w + sp;
    }
}

fn draw_bar(x: f32, y: f32, w: f32, h: f32, v: f32, c0: Color, c1: Color) {
    let sk = h * 0.5;
    fill_convex(&[vec2(x + sk, y), vec2(x + w, y), vec2(x + w - sk, y + h), vec2(x, y + h)], Color::new(1.0, 1.0, 1.0, 0.08));
    let fw = (w - sk) * (v / 100.0).clamp(0.0, 1.0);
    if fw > 0.5 {
        let n = 12;
        for i in 0..n {
            let x0 = x + fw * i as f32 / n as f32;
            let x1 = x + fw * (i + 1) as f32 / n as f32;
            let col = mix(c0, c1, (i as f32 + 0.5) / n as f32);
            fill_convex(&[vec2(x0 + sk, y), vec2(x1 + sk, y), vec2(x1, y + h), vec2(x0, y + h)], col);
        }
    }
}

fn draw_hud(g: &Game, a: &Assets, ui: &Ui, tap: Option<Vec2>) -> bool {
    let u = ui.s;
    let pad = 16.0 * u;
    let top = 14.0 * u;
    draw_label("SCORE", pad, top + 10.0 * u, ui, a, Align::Left);
    text(&fmt_num(g.score.floor() as u64), pad, top + 48.0 * u, 38.0 * u, &a.ui_bold, TEXT, Align::Left);
    let stats = [("VAGUE", g.wave.to_string()), ("TEMPS", fmt_time(g.t)), ("PIÉGÉS", g.kills.to_string()), ("ESSAIM", g.creatures.len().to_string())];
    let mut x = pad;
    for (l, v) in stats.iter() {
        draw_label(l, x, top + 72.0 * u, ui, a, Align::Left);
        text(v, x, top + 92.0 * u, 16.0 * u, &a.ui_bold, TEXT, Align::Left);
        x += 66.0 * u;
    }
    // jauges + pause
    let bsize = 40.0 * u;
    let bx = g.w - pad - bsize;
    let pause_r = Rect::new(bx, top, bsize, bsize);
    draw_panel(bx, top, bsize, bsize, 6.0 * u, Color::new(1.0, 1.0, 1.0, 0.06), PANEL_LINE);
    draw_rectangle(bx + bsize * 0.36, top + bsize * 0.3, bsize * 0.09, bsize * 0.4, TEXT);
    draw_rectangle(bx + bsize * 0.55, top + bsize * 0.3, bsize * 0.09, bsize * 0.4, TEXT);
    let mw = (220.0 * u).min(g.w * 0.4);
    let mx = bx - 12.0 * u - mw;
    let lw = 52.0 * u;
    draw_label("VIE", mx, top + 12.0 * u, ui, a, Align::Left);
    draw_bar(mx + lw, top + 4.0 * u, mw - lw, 9.0 * u, g.hp, DANGER, CORE);
    draw_label("ENCRE", mx, top + 33.0 * u, ui, a, Align::Left);
    draw_bar(mx + lw, top + 25.0 * u, mw - lw, 9.0 * u, g.ink, Color::new(0.18, 0.55, 1.0, 1.0), WALL);
    // bannière de vague
    if let Some((title, sub, bt)) = &g.wave_banner {
        let k = bt / 2.2;
        let (al, sc) = if k < 0.12 {
            let e = k / 0.12;
            (e, 1.4 - 0.4 * e)
        } else if k < 0.75 {
            (1.0, 1.0)
        } else {
            let e = (k - 0.75) / 0.25;
            (1.0 - e, 1.0 - 0.04 * e)
        };
        let y = g.h * 0.36;
        gl_use_material(&a.additive);
        draw_glow(a, g.w / 2.0, y - 20.0 * u, 420.0 * u * sc, rgba(SWARM, 0.25 * al));
        gl_use_default_material();
        text_outlined(title, g.w / 2.0, y - 20.0 * u, (72.0 * u.min(g.w / 520.0).max(0.6)) as u16, sc, &a.title, rgba(TEXT, al), Color::new(0.0, 0.0, 0.0, 0.0), 0.0);
        text(sub, g.w / 2.0, y + 32.0 * u, 13.0 * u, &a.ui_bold, rgba(MUTED, al), Align::Center);
    }
    // astuce de départ
    if g.hint_t < 9.6 {
        let al = if g.hint_t < 9.0 { 1.0 } else { 1.0 - (g.hint_t - 9.0) / 0.6 };
        let msg = if IS_ANDROID || g.touch_seen {
            "Pose un 2e doigt pour tracer un mur avec le 1er · referme une forme pour piéger l’essaim"
        } else {
            "Maintiens le clic pour tracer un mur · referme une forme pour piéger l’essaim"
        };
        let lines = wrap(msg, &a.ui, 13.0 * u, g.w - 2.0 * pad);
        let lh = 18.0 * u;
        let base = g.h - 18.0 * u - lh * (lines.len() as f32 - 1.0);
        for (i, l) in lines.iter().enumerate() {
            text(l, g.w / 2.0, base + i as f32 * lh, 13.0 * u, &a.ui, rgba(MUTED, al), Align::Center);
        }
    }
    hit(pause_r, tap)
}

/// Bouton principal en parallélogramme. Renvoie vrai s'il est touché.
fn go_button(label: &str, cx: f32, cy: f32, ui: &Ui, a: &Assets, tap: Option<Vec2>, primary: bool) -> bool {
    let u = ui.s;
    let fs = 17.0 * u;
    let tw = measure_text(label, Some(&a.ui_bold), fs as u16, 1.0).width;
    let w = tw + 68.0 * u;
    let h = 50.0 * u;
    let (x, y) = (cx - w / 2.0, cy - h / 2.0);
    let sk = 10.0 * u;
    if primary {
        gl_use_material(&a.additive);
        draw_glow(a, cx, cy + 8.0 * u, w * 1.3, rgba(WALL, 0.35));
        gl_use_default_material();
        fill_convex(&[vec2(x + sk, y), vec2(x + w, y), vec2(x + w - sk, y + h), vec2(x, y + h)], WALL);
        fill_convex(&[vec2(x + sk, y), vec2(x + w, y), vec2(x + w - sk * 0.5, y + h * 0.5), vec2(x + sk * 0.5, y + h * 0.5)], Color::new(0.62, 0.94, 1.0, 1.0));
        text(label, cx, cy + fs * 0.36, fs, &a.ui_bold, Color::new(0.024, 0.063, 0.118, 1.0), Align::Center);
    } else {
        draw_panel(x, y + h * 0.1, w, h * 0.8, 6.0 * u, Color::new(1.0, 1.0, 1.0, 0.06), PANEL_LINE);
        text(label, cx, cy + fs * 0.3, fs * 0.82, &a.ui_bold, TEXT, Align::Center);
    }
    hit(Rect::new(x, y, w, h), tap)
}

enum MenuAction {
    None,
    Play,
    Resume,
    Quit,
}

fn draw_title_screen(g: &Game, a: &Assets, tap: Option<Vec2>) -> MenuAction {
    // mise en page calculée à l'échelle 1, puis réduite pour tenir à l'écran
    let base_h = 610.0;
    let k = (g.s * 1.0).min((g.h - 32.0) / base_h).min((g.w - 32.0) / 520.0).max(0.45);
    let ui = Ui { s: k };
    let cw = 520.0 * k;
    let ch = base_h * k;
    let (x, y) = ((g.w - cw) / 2.0, (g.h - ch) / 2.0);
    draw_panel(x, y, cw, ch, 14.0 * k, PANEL, PANEL_LINE);
    let cx = g.w / 2.0;
    // titre lumineux
    gl_use_material(&a.additive);
    draw_glow(a, cx, y + 92.0 * k, 520.0 * k, rgba(SWARM, 0.28));
    gl_use_default_material();
    let tsize = (104.0 * k) as u16;
    text_outlined("ESSAIM", cx, y + 92.0 * k, tsize, 1.0, &a.title, Color::new(0.72, 0.2, 0.35, 1.0), BLANK, 0.0);
    text_outlined("ESSAIM", cx, y + 89.0 * k, tsize, 1.0, &a.title, SWARM, BLANK, 0.0);
    text("Tu es la proie. Ton doigt est leur seule obsession.".replace("doigt", if IS_ANDROID { "doigt" } else { "curseur" }).as_str(), cx, y + 162.0 * k, 14.0 * k, &a.ui, MUTED, Align::Center);
    let how: [(&str, &str); 3] = if IS_ANDROID || g.touch_seen {
        [
            ("FUIR", "Glisse ton doigt : l’essaim te traque en nuée."),
            ("BLOQUER", "Pose un 2e doigt et trace avec le 1er : un mur de lumière infranchissable."),
            ("PIÉGER", "Referme une forme autour d’eux pour les dissoudre."),
        ]
    } else {
        [
            ("FUIR", "Déplace la souris : l’essaim te traque en nuée."),
            ("BLOQUER", "Clic maintenu : un mur de lumière qu’ils ne traversent pas."),
            ("PIÉGER", "Referme une forme autour d’eux pour les dissoudre."),
        ]
    };
    let mut yy = y + 192.0 * k;
    for (i, (t, d)) in how.iter().enumerate() {
        let rx = x + 24.0 * k;
        let rw = cw - 48.0 * k;
        let rh = 74.0 * k;
        draw_panel(rx, yy, rw, rh, 8.0 * k, Color::new(1.0, 1.0, 1.0, 0.04), Color::new(1.0, 1.0, 1.0, 0.06));
        let ic = vec2(rx + 40.0 * k, yy + rh / 2.0);
        draw_how_icon(i, ic, k);
        draw_label(t, rx + 78.0 * k, yy + 24.0 * k, &ui, a, Align::Left);
        let lines = wrap(d, &a.ui, 13.5 * k, rw - 96.0 * k);
        for (j, l) in lines.iter().take(2).enumerate() {
            text(l, rx + 78.0 * k, yy + 44.0 * k + j as f32 * 18.0 * k, 13.5 * k, &a.ui, TEXT, Align::Left);
        }
        yy += rh + 10.0 * k;
    }
    let play = go_button("JOUER", cx, y + 485.0 * k, &ui, a, tap, true);
    let keys = if IS_ANDROID { "Bouton Retour : pause · l’encre se recharge quand tu ne traces pas" } else { "P ou Échap : pause · l’encre se recharge quand tu ne traces pas" };
    for (j, l) in wrap(keys, &a.ui, 12.0 * k, cw - 40.0 * k).iter().enumerate() {
        text(l, cx, y + 545.0 * k + j as f32 * 16.0 * k, 12.0 * k, &a.ui, MUTED, Align::Center);
    }
    if g.best > 0 {
        text(&format!("Record : {}", fmt_num(g.best)), cx, y + 588.0 * k, 13.0 * k, &a.ui_bold, CORE, Align::Center);
    }
    if play {
        MenuAction::Play
    } else {
        MenuAction::None
    }
}

fn draw_how_icon(i: usize, c: Vec2, k: f32) {
    match i {
        0 => {
            draw_circle_lines(c.x, c.y, 11.0 * k, 2.0 * k, rgba(CORE, 0.35));
            draw_circle(c.x, c.y, 5.0 * k, CORE);
            for (dx, dy) in [(-15.0, -11.0), (-12.0, 11.0), (14.0, -14.0), (15.0, 13.0)] {
                draw_circle(c.x + dx * k, c.y + dy * k, 2.4 * k, SWARM);
            }
        }
        1 => {
            let mut prev = None;
            for s in 0..=16 {
                let t = s as f32 / 16.0;
                let p = vec2(c.x - 14.0 * k + 28.0 * k * t, c.y + (t * TAU * 1.0).sin() * -9.0 * k);
                if let Some(q) = prev {
                    let q: Vec2 = q;
                    draw_line(q.x, q.y, p.x, p.y, 3.0 * k, WALL);
                }
                prev = Some(p);
            }
            draw_circle(c.x - 8.0 * k, c.y - 10.0 * k, 2.4 * k, SWARM);
            draw_circle(c.x + 8.0 * k, c.y + 11.0 * k, 2.4 * k, SWARM);
        }
        _ => {
            let pts: Vec<Vec2> = (0..24).map(|s| {
                let a = s as f32 / 24.0 * TAU;
                vec2(c.x + a.cos() * 15.0 * k * (1.0 + 0.08 * (3.0 * a).sin()), c.y + a.sin() * 14.0 * k)
            }).collect();
            fill_convex(&pts, rgba(WALL, 0.15));
            for s in 0..pts.len() {
                let (p, q) = (pts[s], pts[(s + 1) % pts.len()]);
                draw_line(p.x, p.y, q.x, q.y, 3.0 * k, WALL);
            }
            for (dx, dy) in [(-3.0, -1.0), (3.0, 5.0), (4.0, -5.0)] {
                draw_circle(c.x + dx * k, c.y + dy * k, 2.4 * k, SWARM);
            }
        }
    }
}

fn draw_pause_screen(g: &Game, a: &Assets, tap: Option<Vec2>) -> MenuAction {
    let k = g.s.min((g.h - 32.0) / 260.0).min((g.w - 32.0) / 360.0).max(0.45);
    let ui = Ui { s: k };
    let (cw, ch) = (360.0 * k, 260.0 * k);
    let (x, y) = ((g.w - cw) / 2.0, (g.h - ch) / 2.0);
    draw_rectangle(0.0, 0.0, g.w, g.h, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_panel(x, y, cw, ch, 14.0 * k, PANEL, PANEL_LINE);
    let cx = g.w / 2.0;
    text_outlined("Pause", cx, y + 62.0 * k, (52.0 * k) as u16, 1.0, &a.title, WALL, BLANK, 0.0);
    if go_button("REPRENDRE", cx, y + 140.0 * k, &ui, a, tap, true) {
        return MenuAction::Resume;
    }
    if go_button("ABANDONNER", cx, y + 210.0 * k, &ui, a, tap, false) {
        return MenuAction::Quit;
    }
    MenuAction::None
}

fn draw_over_screen(g: &Game, a: &Assets, tap: Option<Vec2>) -> MenuAction {
    let k = g.s.min((g.h - 32.0) / 340.0).min((g.w - 32.0) / 520.0).max(0.45);
    let ui = Ui { s: k };
    let (cw, ch) = (520.0 * k, 340.0 * k);
    let (x, y) = ((g.w - cw) / 2.0, (g.h - ch) / 2.0);
    draw_panel(x, y, cw, ch, 14.0 * k, PANEL, PANEL_LINE);
    let cx = g.w / 2.0;
    gl_use_material(&a.additive);
    draw_glow(a, cx, y + 58.0 * k, 460.0 * k, rgba(DANGER, 0.3));
    gl_use_default_material();
    text_outlined("L’essaim t’a eu", cx, y + 58.0 * k, (48.0 * k) as u16, 1.0, &a.title, DANGER, BLANK, 0.0);
    if g.new_record {
        text("NOUVEAU RECORD", cx, y + 104.0 * k, 13.0 * k, &a.ui_bold, CORE, Align::Center);
    }
    let stats = [("SCORE", fmt_num(g.score.floor() as u64)), ("SURVIE", fmt_time(g.t)), ("PIÉGÉS", g.kills.to_string())];
    let bw = (cw - 48.0 * k - 20.0 * k) / 3.0;
    for (i, (l, v)) in stats.iter().enumerate() {
        let bx = x + 24.0 * k + i as f32 * (bw + 10.0 * k);
        let by = y + 122.0 * k;
        draw_panel(bx, by, bw, 66.0 * k, 8.0 * k, Color::new(1.0, 1.0, 1.0, 0.04), BLANK);
        draw_label(l, bx + bw / 2.0, by + 22.0 * k, &ui, a, Align::Center);
        text(v, bx + bw / 2.0, by + 52.0 * k, 22.0 * k, &a.ui_bold, TEXT, Align::Center);
    }
    text(&format!("Record : {} · Vague atteinte : {}", fmt_num(g.best), g.wave), cx, y + 222.0 * k, 13.0 * k, &a.ui, MUTED, Align::Center);
    if go_button("REJOUER", cx, y + 280.0 * k, &ui, a, tap, true) {
        return MenuAction::Play;
    }
    MenuAction::None
}

// ---------------------------------------------------------------- point d'entrée
fn window_conf() -> Conf {
    Conf {
        window_title: "Essaim".to_owned(),
        window_width: 1200,
        window_height: 800,
        high_dpi: true,
        sample_count: 4,
        window_resizable: true,
        ..Default::default()
    }
}

/// Pilote automatique pour vérifier le rendu sans écran (ordinateur uniquement) :
/// ESSAIM_AUTOTEST=dossier cargo run → captures d'écran puis fermeture.
struct AutoTest {
    dir: String,
    frame: u32,
}

#[macroquad::main(window_conf)]
async fn main() {
    rand::srand(macroquad::miniquad::date::now().to_bits());
    simulate_mouse_with_touch(false);
    prevent_quit();
    let assets = load_assets().await;
    let mut g = Game::new();
    let mut auto = std::env::var("ESSAIM_AUTOTEST").ok().map(|dir| AutoTest { dir, frame: 0 });

    loop {
        let (sw, sh) = (screen_width(), screen_height());
        if (sw - g.w).abs() > 0.5 || (sh - g.h).abs() > 0.5 {
            g.resize(sw, sh);
        }
        let raw_dt = get_frame_time();
        let dt = raw_dt.min(0.04);
        let mut inp = g.read_input();

        // pilote automatique (tests)
        if let Some(at) = &mut auto {
            at.frame += 1;
            let f = at.frame as f32 / 60.0;
            inp.tap = None;
            if at.frame == 50 && g.state == State::Title {
                g.new_game();
            }
            let c = vec2(g.w / 2.0, g.h / 2.0);
            let r = 170.0 * g.s;
            if f > 3.6 && f < 4.6 {
                let a = (f - 3.6) / 0.85 * TAU;
                g.ptr = c + vec2(a.cos(), a.sin()) * r;
                inp.down = true;
            } else if f > 5.0 && f < 5.6 {
                g.ptr = vec2(g.w * 0.25 + (f - 5.0) / 0.6 * g.w * 0.5, g.h * 0.2);
                inp.down = true;
            } else {
                g.ptr = c + vec2((f * 0.7).cos(), (f * 0.9).sin()) * 40.0 * g.s;
                inp.down = false;
            }
            inp.pos = g.ptr;
        }

        // retour Android / échap / P
        let back = is_quit_requested() || is_key_pressed(KeyCode::Back) || is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P);
        if back {
            match g.state {
                State::Play => g.pause(true),
                State::Pause => g.pause(false),
                State::Over => g.to_title(),
                State::Title => {
                    if is_quit_requested() || is_key_pressed(KeyCode::Back) {
                        macroquad::miniquad::window::order_quit();
                    }
                }
            }
        }
        if is_key_pressed(KeyCode::Enter) && (g.state == State::Title || g.state == State::Over) {
            g.new_game();
        }
        // application mise en arrière-plan : pause automatique
        if raw_dt > 0.5 && g.state == State::Play {
            g.pause(true);
        }

        match g.state {
            State::Play => {
                // tracé : front montant / descendant
                if inp.down && !g.down {
                    g.down = true;
                    g.start_stroke();
                } else if !inp.down && g.down {
                    g.down = false;
                    g.end_stroke();
                }
                g.update(dt);
            }
            State::Title | State::Over => {
                g.demo_t += dt;
                let tgt = if g.state == State::Title {
                    let p = vec2(g.w / 2.0 + (g.demo_t * 0.45).cos() * g.w * 0.32, g.h / 2.0 + (g.demo_t * 0.7).sin() * g.h * 0.3);
                    g.player = p;
                    p
                } else {
                    g.over_t += dt;
                    g.player
                };
                g.build_seg_grid();
                g.step_swarm(dt, tgt);
                if g.state == State::Title {
                    for c in &mut g.creatures {
                        if c.pos.distance(tgt) < 12.0 * g.s {
                            c.stun = 1.0;
                            c.vel = -c.vel;
                        }
                    }
                }
            }
            State::Pause => {}
        }
        if g.state != State::Pause {
            g.update_fx(dt);
        }

        // rendu
        render_world(&g, &assets);
        let ui = Ui { s: g.s };
        match g.state {
            State::Play => {
                if draw_hud(&g, &assets, &ui, inp.tap) {
                    g.pause(true);
                }
            }
            State::Pause => {
                draw_hud(&g, &assets, &ui, None);
                match draw_pause_screen(&g, &assets, inp.tap) {
                    MenuAction::Resume => g.pause(false),
                    MenuAction::Quit => g.to_title(),
                    _ => {}
                }
            }
            State::Title => {
                if let MenuAction::Play = draw_title_screen(&g, &assets, inp.tap) {
                    g.new_game();
                }
            }
            State::Over => {
                if g.over_t > 0.9 {
                    if let MenuAction::Play = draw_over_screen(&g, &assets, inp.tap) {
                        g.new_game();
                    }
                } else {
                    draw_hud(&g, &assets, &ui, None);
                }
            }
        }

        if let Some(at) = &auto {
            if [40u32, 260, 290, 330, 420].contains(&at.frame) {
                get_screen_data().export_png(&format!("{}/frame_{:04}.png", at.dir, at.frame));
            }
            if at.frame == 420 {
                g.pause(true);
            }
            if at.frame == 425 {
                get_screen_data().export_png(&format!("{}/frame_pause.png", at.dir));
                g.hp = 0.0;
                g.state = State::Play;
            }
            if at.frame == 500 {
                get_screen_data().export_png(&format!("{}/frame_over.png", at.dir));
                println!("autotest ok: score={} kills={} creatures={}", g.score as u64, g.kills, g.creatures.len());
                break;
            }
        }
        next_frame().await;
    }
}
