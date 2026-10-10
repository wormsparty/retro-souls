//! PS1 interface style: everything is drawn on a grid of big pixels ("dots"),
//! like the game's pixels: home-made bitmap font (`tools/pixel_font.py`) with a drop shadow,
//! and key icons (keyboard, mouse, Xbox gamepad) in pixel art generated at startup.
//!
//! A dot is a whole number of screen pixels (grid of about 360 rows): font
//! and icon sizes are recomputed when the window is resized.
//!
//! A help line (`Hint`) mixes icons and text; its children are rebuilt when
//! its content changes. One icon pixel = one dot.

use std::borrow::Cow;
use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::{FontSmoothing, LineHeight};
use bevy::window::PrimaryWindow;

pub const FONT: &str = "fonts/psx-souls.ttf";
/// Font em size, in dots (see `tools/pixel_font.py`).
const FONT_EM: f32 = 12.0;
/// Target number of dot rows over the screen height.
const GRID_LINES: f32 = 360.0;
/// Text drop shadow.
const SHADOW: Color = Color::srgba(0.02, 0.015, 0.02, 0.9);

/// Size of a dot in UI units (already multiplied by `UiScale`).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct UiPixel(pub f32);

impl Default for UiPixel {
    fn default() -> Self {
        Self(2.0)
    }
}

/// Text at scale `n` of the font (1 = body text, 2 = titles…).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PixelText(pub u8);

/// Node sized in dots (icons, images).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PixelSize(pub UVec2);

#[derive(Resource, Clone)]
pub struct UiFont(pub Handle<Font>);

impl UiFont {
    /// Font at scale `n`: its size is fitted to the grid by `fit_pixels`.
    pub fn at(&self, n: u8) -> impl Bundle + use<> {
        (
            TextFont {
                font: self.0.clone().into(),
                font_size: FontSize::Px(FONT_EM * n as f32 * UiPixel::default().0),
                font_smoothing: FontSmoothing::None,
                ..default()
            },
            LineHeight::RelativeToFont(1.0),
            PixelText(n),
            TextShadow { offset: Vec2::splat(n as f32 * UiPixel::default().0), color: SHADOW },
        )
    }

    pub fn text<S: Into<String>>(&self, s: S, n: u8, color: Color) -> impl Bundle + use<S> {
        (Text::new(s), self.at(n), TextColor(color))
    }
}

/// Key icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    /// Keyboard key with its label (letters, digits, ↑↓←→↵, or a short word).
    Key(&'static str),
    MouseLeft,
    MouseRight,
    MouseMiddle,
    MouseMove,
    PadA,
    PadB,
    PadX,
    PadY,
    PadLB,
    PadRB,
    PadLT,
    PadRT,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    /// Whole D-pad (navigate).
    Dpad,
    StickL,
    StickR,
    StickL3,
    StickR3,
    PadMenu,
    /// ‹ › arrows of the menu values.
    ValueLeft,
    ValueRight,
    /// GitHub logo (the cat in a disc).
    GitHub,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Icon(Glyph),
    Text(Cow<'static, str>),
}

pub const fn t(s: &'static str) -> Seg {
    Seg::Text(Cow::Borrowed(s))
}

pub const fn i(g: Glyph) -> Seg {
    Seg::Icon(g)
}

/// "Icons + text" line. Changing `segs` rebuilds the line.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Hint {
    pub segs: Vec<Seg>,
    /// Scale (like `PixelText`).
    pub scale: u8,
    pub color: Color,
}

impl Hint {
    pub fn new(scale: u8, color: Color) -> Self {
        Self { segs: Vec::new(), scale, color }
    }
}

/// Replaces the content of a line if it changed. Goes through `Mut` without dereferencing it for
/// writing otherwise: the line would be marked changed, hence rebuilt, every frame.
pub fn set_hint(h: &mut Mut<Hint>, segs: Vec<Seg>) {
    if h.segs != segs {
        h.segs = segs;
    }
}

/// Node of a help line (to be used with `Hint`).
pub fn hint_node() -> Node {
    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4), ..default() }
}

#[derive(Resource, Default)]
pub struct Icons(HashMap<Glyph, (Handle<Image>, UVec2)>);

impl Icons {
    pub fn get(&mut self, g: Glyph, images: &mut Assets<Image>) -> (Handle<Image>, UVec2) {
        self.0
            .entry(g)
            .or_insert_with(|| {
                let c = draw(g);
                let size = UVec2::new(c.w as u32, c.h as u32);
                (images.add(c.image()), size)
            })
            .clone()
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        let font = app.world().resource::<AssetServer>().load(FONT);
        app.insert_resource(UiFont(font))
            .init_resource::<Icons>()
            .init_resource::<UiPixel>()
            .add_systems(
                PostUpdate,
                (pixel_grid, build_hints, fit_pixels, shadow_alpha).chain().before(bevy::ui::UiSystems::Prepare),
            );
    }
}

/// Size of a dot: a whole number of screen pixels, about 1/360 of its height.
fn pixel_grid(window: Single<&Window, With<PrimaryWindow>>, ui_scale: Res<UiScale>, mut px: ResMut<UiPixel>) {
    let physical = (window.physical_height() as f32 / GRID_LINES).round().max(1.0);
    let dot = physical / (ui_scale.0 * window.scale_factor()).max(1e-3);
    if (px.0 - dot).abs() > 1e-4 {
        px.0 = dot;
    }
}

/// Snaps font and icon sizes to the dot grid.
fn fit_pixels(
    px: Res<UiPixel>,
    mut texts: Query<(Ref<PixelText>, &mut TextFont, &mut TextShadow)>,
    mut nodes: Query<(Ref<PixelSize>, &mut Node)>,
) {
    let all = px.is_changed();
    for (p, mut f, mut shadow) in &mut texts {
        if all || p.is_changed() {
            f.font_size = FontSize::Px(FONT_EM * p.0 as f32 * px.0);
            shadow.offset = Vec2::splat(p.0 as f32 * px.0);
        }
    }
    for (p, mut n) in &mut nodes {
        if all || p.is_changed() {
            n.width = Val::Px(p.0.x as f32 * px.0);
            n.height = Val::Px(p.0.y as f32 * px.0);
        }
    }
}

/// The drop shadow follows the text's transparency (fades).
fn shadow_alpha(mut q: Query<(&TextColor, &mut TextShadow), Changed<TextColor>>) {
    for (c, mut s) in &mut q {
        let a = SHADOW.alpha() * c.0.alpha();
        if (s.color.alpha() - a).abs() > 1e-3 {
            s.color.set_alpha(a);
        }
    }
}

/// Image node of an icon, at scale `n` (one icon pixel = `n` dots).
pub fn icon_bundle(icons: &mut Icons, images: &mut Assets<Image>, g: Glyph, n: u8) -> (ImageNode, Node, PixelSize) {
    let (h, s) = icons.get(g, images);
    image_bundle(h, s * n as u32)
}

/// Image node of `size` dots.
pub fn image_bundle(image: Handle<Image>, size: UVec2) -> (ImageNode, Node, PixelSize) {
    (ImageNode::new(image), Node { flex_shrink: 0.0, ..default() }, PixelSize(size))
}

fn build_hints(
    mut commands: Commands,
    font: Res<UiFont>,
    mut icons: ResMut<Icons>,
    mut images: ResMut<Assets<Image>>,
    q: Query<(Entity, &Hint), Changed<Hint>>,
) {
    for (e, hint) in &q {
        let mut ec = commands.entity(e);
        ec.despawn_children();
        for seg in &hint.segs {
            match seg {
                Seg::Icon(g) => {
                    ec.with_child(icon_bundle(&mut icons, &mut images, *g, hint.scale));
                }
                Seg::Text(s) => {
                    ec.with_child((font.text(s.clone().into_owned(), hint.scale, hint.color), Node { margin: UiRect::horizontal(px(2)), ..default() }));
                }
            }
        }
    }
}

// --- Drawing the icons -------------------------------------------------------------------

type Rgba = [u8; 4];

const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    [r, g, b, 255]
}

const CLEAR: Rgba = [0, 0, 0, 0];
const INK: Rgba = rgb(14, 12, 12);
const PAD_BODY: Rgba = rgb(38, 38, 44);
const PAD_RIM: Rgba = rgb(78, 78, 88);
const PAD_TEXT: Rgba = rgb(225, 222, 214);
const KEY_TOP: Rgba = rgb(214, 208, 194);
const KEY_SIDE: Rgba = rgb(140, 134, 122);
const KEY_TEXT: Rgba = rgb(34, 30, 28);
const GOLD: Rgba = rgb(230, 209, 158);

struct Canvas {
    w: i32,
    h: i32,
    px: Vec<Rgba>,
}

impl Canvas {
    fn new(w: i32, h: i32) -> Self {
        Self { w, h, px: vec![CLEAR; (w * h) as usize] }
    }

    fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if (0..self.w).contains(&x) && (0..self.h).contains(&y) {
            self.px[(y * self.w + x) as usize] = c;
        }
    }

    fn get(&self, x: i32, y: i32) -> Rgba {
        if (0..self.w).contains(&x) && (0..self.h).contains(&y) { self.px[(y * self.w + x) as usize] } else { CLEAR }
    }

    fn rect(&mut self, x0: i32, y0: i32, w: i32, h: i32, c: Rgba) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.set(x, y, c);
            }
        }
    }

    /// Rectangle with cut corners (one pixel).
    fn rounded(&mut self, x0: i32, y0: i32, w: i32, h: i32, c: Rgba) {
        self.rect(x0 + 1, y0, w - 2, h, c);
        self.rect(x0, y0 + 1, 1, h - 2, c);
        self.rect(x0 + w - 1, y0 + 1, 1, h - 2, c);
    }

    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        for y in 0..self.h {
            for x in 0..self.w {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if dx * dx + dy * dy <= r * r {
                    self.set(x, y, c);
                }
            }
        }
    }

    /// Dark outline around everything drawn.
    fn outline(&mut self) {
        let src: Vec<(i32, i32)> = (0..self.h)
            .flat_map(|y| (0..self.w).map(move |x| (x, y)))
            .filter(|&(x, y)| self.get(x, y)[3] == 0)
            .filter(|&(x, y)| [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| self.get(x + dx, y + dy)[3] > 0 && self.get(x + dx, y + dy) != INK))
            .collect();
        for (x, y) in src {
            self.set(x, y, INK);
        }
    }

    fn text(&mut self, x: i32, y: i32, s: &str, c: Rgba) {
        let mut cx = x;
        for ch in s.chars() {
            if let Some(rows) = glyph5x7(ch) {
                for (dy, row) in rows.iter().enumerate() {
                    for (dx, b) in row.bytes().enumerate() {
                        if b == b'#' {
                            self.set(cx + dx as i32, y + dy as i32, c);
                        }
                    }
                }
            }
            cx += 6;
        }
    }

    fn image(&self) -> Image {
        Image::new(
            Extent3d { width: self.w as u32, height: self.h as u32, depth_or_array_layers: 1 },
            TextureDimension::D2,
            self.px.iter().flatten().copied().collect(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        )
    }
}

fn text_width(s: &str) -> i32 {
    s.chars().count() as i32 * 6 - 1
}

/// Round gamepad button (dark body, coloured letter).
fn pad_round(label: &str, color: Rgba) -> Canvas {
    let mut c = Canvas::new(17, 17);
    c.disc(8.5, 8.5, 7.6, PAD_RIM);
    c.disc(8.5, 8.0, 6.6, PAD_BODY);
    c.text(8 - text_width(label) / 2, 5, label, color);
    c.outline();
    c
}

fn keycap(label: &str) -> Canvas {
    let w = (text_width(label) + 8).max(15);
    let mut c = Canvas::new(w + 2, 15);
    c.rounded(1, 1, w, 13, KEY_SIDE);
    c.rounded(1, 1, w, 10, KEY_TOP);
    c.text(1 + (w - text_width(label)) / 2, 2, label, KEY_TEXT);
    c.outline();
    c
}

fn mouse(button: Option<i32>) -> Canvas {
    let mut c = Canvas::new(13, 15);
    c.rounded(1, 2, 11, 12, PAD_RIM);
    c.rect(2, 1, 9, 1, PAD_RIM);
    c.rect(6, 2, 1, 6, PAD_BODY);
    c.rect(1, 7, 11, 1, PAD_BODY);
    match button {
        Some(-1) => c.rect(2, 2, 4, 5, GOLD),
        Some(1) => c.rect(7, 2, 4, 5, GOLD),
        Some(0) => c.rect(5, 2, 3, 4, GOLD),
        _ => {}
    }
    c.outline();
    c
}

fn dpad(dir: Option<(i32, i32)>) -> Canvas {
    let mut c = Canvas::new(17, 17);
    c.rect(6, 1, 5, 15, PAD_RIM);
    c.rect(1, 6, 15, 5, PAD_RIM);
    c.rect(7, 7, 3, 3, PAD_BODY);
    match dir {
        Some((0, -1)) => c.rect(7, 2, 3, 4, GOLD),
        Some((0, 1)) => c.rect(7, 11, 3, 4, GOLD),
        Some((-1, 0)) => c.rect(2, 7, 4, 3, GOLD),
        Some((1, 0)) => c.rect(11, 7, 4, 3, GOLD),
        _ => {}
    }
    c.outline();
    c
}

fn stick(label: &str) -> Canvas {
    let mut c = Canvas::new(17, 17);
    c.disc(8.5, 8.5, 7.6, PAD_BODY);
    c.disc(8.5, 8.5, 5.6, PAD_RIM);
    c.disc(8.5, 8.5, 4.6, PAD_BODY);
    c.text(8 - text_width(label) / 2 + (label.len() as i32 - 1) % 2, 5, label, PAD_TEXT);
    c.outline();
    c
}

fn bumper(label: &str) -> Canvas {
    let mut c = Canvas::new(23, 13);
    c.rounded(1, 1, 21, 11, PAD_RIM);
    c.rounded(2, 2, 19, 9, PAD_BODY);
    c.text(12 - text_width(label) / 2 - 1, 3, label, PAD_TEXT);
    c.outline();
    c
}

fn trigger(label: &str) -> Canvas {
    let mut c = Canvas::new(17, 17);
    c.rounded(1, 3, 15, 13, PAD_RIM);
    c.rect(3, 1, 11, 2, PAD_RIM);
    c.rounded(2, 4, 13, 11, PAD_BODY);
    c.text(9 - text_width(label) / 2 - 1, 6, label, PAD_TEXT);
    c.outline();
    c
}

fn draw(g: Glyph) -> Canvas {
    match g {
        Glyph::Key(label) => keycap(label),
        Glyph::MouseLeft => mouse(Some(-1)),
        Glyph::MouseRight => mouse(Some(1)),
        Glyph::MouseMiddle => mouse(Some(0)),
        Glyph::MouseMove => mouse(None),
        Glyph::PadA => pad_round("A", rgb(110, 196, 64)),
        Glyph::PadB => pad_round("B", rgb(228, 70, 58)),
        Glyph::PadX => pad_round("X", rgb(70, 132, 236)),
        Glyph::PadY => pad_round("Y", rgb(246, 196, 48)),
        Glyph::PadLB => bumper("LB"),
        Glyph::PadRB => bumper("RB"),
        Glyph::PadLT => trigger("LT"),
        Glyph::PadRT => trigger("RT"),
        Glyph::DpadUp => dpad(Some((0, -1))),
        Glyph::DpadDown => dpad(Some((0, 1))),
        Glyph::DpadLeft => dpad(Some((-1, 0))),
        Glyph::DpadRight => dpad(Some((1, 0))),
        Glyph::Dpad => dpad(None),
        Glyph::StickL => stick("L"),
        Glyph::StickR => stick("R"),
        Glyph::StickL3 => stick("L3"),
        Glyph::StickR3 => stick("R3"),
        Glyph::PadMenu => {
            let mut c = Canvas::new(15, 15);
            c.disc(7.5, 7.5, 6.6, PAD_BODY);
            for y in [4, 7, 10] {
                c.rect(4, y, 7, 1, PAD_TEXT);
            }
            c.outline();
            c
        }
        Glyph::GitHub => {
            // Light disc, cat cut out in dark: head and ears, neck, tail.
            let (light, dark) = (PAD_TEXT, rgb(24, 22, 28));
            let mut c = Canvas::new(16, 16);
            c.disc(8.0, 8.0, 7.7, light);
            for y in 0..16 {
                for x in 0..16 {
                    let (dx, dy) = ((x as f32 + 0.5 - 8.0) / 4.4, (y as f32 + 0.5 - 7.4) / 3.6);
                    if dx * dx + dy * dy <= 1.0 {
                        c.set(x, y, dark);
                    }
                }
            }
            for (x, y) in [(4, 2), (4, 3), (5, 3), (11, 2), (11, 3), (10, 3), (3, 11), (4, 12), (5, 12)] {
                c.set(x, y, dark);
            }
            c.rect(6, 10, 4, 6, dark);
            for y in 0..16 {
                for x in 0..16 {
                    let (dx, dy) = (x as f32 + 0.5 - 8.0, y as f32 + 0.5 - 8.0);
                    if dx * dx + dy * dy > 7.7 * 7.7 {
                        c.set(x, y, CLEAR);
                    }
                }
            }
            c
        }
        Glyph::ValueLeft | Glyph::ValueRight => {
            let mut c = Canvas::new(5, 9);
            for k in 0..4 {
                let x = if g == Glyph::ValueLeft { 1 + k } else { 3 - k };
                c.rect(x, 4 - k, 1, 2 * k + 1, GOLD);
            }
            c
        }
    }
}

/// 5×7 font for icon labels (capitals, digits, arrows).
fn glyph5x7(c: char) -> Option<[&'static str; 7]> {
    Some(match c.to_ascii_uppercase() {
        'A' => [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'B' => ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."],
        'C' => [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
        'D' => ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."],
        'E' => ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
        'F' => ["#####", "#....", "#....", "####.", "#....", "#....", "#...."],
        'G' => [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####"],
        'H' => ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'I' => [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."],
        'J' => ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."],
        'K' => ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"],
        'L' => ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
        'M' => ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"],
        'N' => ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#"],
        'O' => [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'P' => ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."],
        'Q' => [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"],
        'R' => ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
        'S' => [".####", "#....", "#....", ".###.", "....#", "....#", "####."],
        'T' => ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
        'U' => ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'V' => ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
        'W' => ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#."],
        'X' => ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"],
        'Y' => ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."],
        'Z' => ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"],
        '0' => [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
        '1' => ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
        '2' => [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
        '3' => ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###."],
        '4' => ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
        '5' => ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
        '↑' => ["..#..", ".###.", "#.#.#", "..#..", "..#..", "..#..", "..#.."],
        '↓' => ["..#..", "..#..", "..#..", "..#..", "#.#.#", ".###.", "..#.."],
        '←' => [".....", "..#..", ".#...", "#####", ".#...", "..#..", "....."],
        '→' => [".....", "..#..", "...#.", "#####", "...#.", "..#..", "....."],
        '↵' => ["....#", "....#", "..#.#", ".#..#", "#####", ".#...", "..#.."],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_icons_draw() {
        for g in [Glyph::Key("ESPACE"), Glyph::Key("↵"), Glyph::PadA, Glyph::PadLT, Glyph::StickR3, Glyph::DpadDown, Glyph::MouseMiddle, Glyph::ValueLeft] {
            let c = draw(g);
            assert!(c.px.iter().any(|p| p[3] > 0), "{g:?} empty");
        }
    }
}
