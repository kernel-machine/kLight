//! Movement (EFX) and matrix effects. They work in physical units (degrees,
//! normalised RGB) and are converted to DMX only when written to the layer.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use klight_fixtures::convert::{
    additive_mix, apply_calibration, degrees_to_dmx16, dmx16_to_degrees, nearest_wheel_value,
    unit_to16,
};
use klight_model::function::{Direction, MovementShape, RunMode};
use klight_model::{
    Color, MatrixAlgorithm, MatrixAxis, MatrixControls, MatrixEffect, MovementEffect, TimeValue,
};

use crate::compile::{CompiledMatrix, CompiledMovement, LogicalChannel, NO_CHANNEL};
use crate::music::{beats_to_fixed, MusicClock};
use crate::playback::{scale, Layer};

/// Position in the cycle, in turns (fractional part is the phase).
pub fn cycle_turns(
    period: TimeValue,
    started_us: u64,
    started_beat: u64,
    now_us: u64,
    music: &MusicClock,
) -> f64 {
    match period {
        TimeValue::Ms(ms) if ms > 0 => {
            now_us.saturating_sub(started_us) as f64 / (f64::from(ms) * 1000.0)
        }
        TimeValue::Beats(b) => {
            let len = beats_to_fixed(b);
            if len == 0 {
                return 0.0;
            }
            music.beat_at(now_us).saturating_sub(started_beat) as f64 / len as f64
        }
        TimeValue::Ms(_) => 0.0,
    }
}

/// Unit shape position for phase `t` (turns), each axis in -1..=1.
pub fn shape_point(shape: MovementShape, t: f32) -> (f32, f32) {
    let a = t * TAU;
    match shape {
        MovementShape::Circle => (a.cos(), a.sin()),
        MovementShape::Eight => (a.sin(), (2.0 * a).sin()),
        MovementShape::Line => (a.sin(), 0.0),
        MovementShape::Polygon { sides } => polygon_point(sides, t),
        MovementShape::Lissajous { x_freq, y_freq } => (
            (f32::from(x_freq.max(1)) * a).sin(),
            (f32::from(y_freq.max(1)) * a).sin(),
        ),
        MovementShape::Sine { periods } => {
            // Across during the first half of the cycle, back along the same
            // wave during the second.
            let t = t.rem_euclid(1.0);
            let u = if t < 0.5 { 2.0 * t } else { 2.0 - 2.0 * t };
            (
                2.0 * u - 1.0,
                (TAU * f32::from(periods.clamp(1, 8)) * u).sin(),
            )
        }
        MovementShape::Fan { hold } => (fan_x(hold, t), 0.0),
    }
}

/// Corner `k` of a regular polygon of `n` sides on the unit circle, the first
/// one at the bottom left of a flat bottom edge, going counterclockwise.
fn polygon_vertex(k: usize, n: usize) -> (f32, f32) {
    let a = -FRAC_PI_2 - PI / n as f32 + TAU * k as f32 / n as f32;
    (a.cos(), a.sin())
}

/// A point along a regular polygon, one side after the other at the same
/// speed, stretched so that it spans exactly -1..=1 on both axes.
fn polygon_point(sides: u8, t: f32) -> (f32, f32) {
    let n = usize::from(sides.clamp(3, 12));
    let (mut lo_x, mut hi_x, mut lo_y, mut hi_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for k in 0..n {
        let (x, y) = polygon_vertex(k, n);
        lo_x = lo_x.min(x);
        hi_x = hi_x.max(x);
        lo_y = lo_y.min(y);
        hi_y = hi_y.max(y);
    }
    let u = t.rem_euclid(1.0) * n as f32;
    let k = (u as usize).min(n - 1);
    let along = u - k as f32;
    let (x0, y0) = polygon_vertex(k, n);
    let (x1, y1) = polygon_vertex((k + 1) % n, n);
    let fit = |v: f32, lo: f32, hi: f32| {
        let span = hi - lo;
        if span > 1e-6 {
            (v - lo) / span * 2.0 - 1.0
        } else {
            0.0
        }
    };
    (
        fit(x0 + (x1 - x0) * along, lo_x, hi_x),
        fit(y0 + (y1 - y0) * along, lo_y, hi_y),
    )
}

/// Opening of a fan at `t`, from -1 (closed) to 1 (open): opening, a hold,
/// closing, a hold, at a steady speed. `hold` is the percent of the cycle spent
/// at each end, up to 40. Also the line the fan covers as a whole.
fn fan_x(hold: u8, t: f32) -> f32 {
    let hold = f32::from(hold.min(40)) / 100.0;
    let sweep = 0.5 - hold;
    let t = t.rem_euclid(1.0);
    if t < sweep {
        -1.0 + 2.0 * t / sweep
    } else if t < 0.5 {
        1.0
    } else if t < 0.5 + sweep {
        1.0 - 2.0 * (t - 0.5) / sweep
    } else {
        -1.0
    }
}

fn write(layer: &mut Layer, channels: &[LogicalChannel], ch: u32, v: u16) {
    if ch == NO_CHANNEL {
        return;
    }
    if let Some(lc) = channels.get(ch as usize) {
        layer.write(ch as usize, lc.merge, v);
    }
}

/// Where head `order` of `n` stands among the others, from -1 for the first to
/// 1 for the last (0 for a lone head): the side a fan opens it towards.
pub fn head_spread(order: usize, n: usize) -> f32 {
    if n < 2 {
        return 0.0;
    }
    let last = (n - 1) as f32;
    (2.0 * order as f32 - last) / last
}

/// Phase offset (turns) of head `order` of `n` for a phase spread in degrees.
pub fn head_phase(phase_spread: f32, order: usize, n: usize) -> f32 {
    if n == 0 {
        return 0.0;
    }
    (phase_spread / 360.0) * order as f32 / n as f32
}

/// Where along the shape (turns, 0..1) a head with phase offset `phase` is
/// at cycle position `turns`, following the direction and the run mode: a
/// single shot stops at the end of its cycle, ping-pong goes back along the
/// shape every other cycle.
pub fn movement_t(e: &MovementEffect, turns: f64, phase: f32) -> f32 {
    let sign = if e.direction == Direction::Backward {
        -1.0
    } else {
        1.0
    };
    match e.run_mode {
        RunMode::Loop => ((turns * sign) as f32 + phase).rem_euclid(1.0),
        RunMode::SingleShot => ((turns.clamp(0.0, 1.0) * sign) as f32 + phase).rem_euclid(1.0),
        RunMode::PingPong => {
            let u = (turns * sign + f64::from(phase)).rem_euclid(2.0);
            (if u > 1.0 { 2.0 - u } else { u }) as f32
        }
    }
}

/// Offset from the center of the point of a shape: scaled to the width and
/// height, rotated, mirrored left to right for a mirrored head, and scaled by
/// `size` (0..=1, the fades).
fn place(e: &MovementEffect, (x, y): (f32, f32), mirrored: bool, size: f32) -> (f32, f32) {
    let (dx, dy) = (x * e.width / 2.0, y * e.height / 2.0);
    let angle = if e.rotation.is_finite() {
        e.rotation.to_radians()
    } else {
        0.0
    };
    let (sin, cos) = angle.sin_cos();
    let (dx, dy) = (dx * cos - dy * sin, dx * sin + dy * cos);
    // A fan sends each head to its own side already: mirroring has no say.
    let dx = if mirrored && !matches!(e.shape, MovementShape::Fan { .. }) {
        -dx
    } else {
        dx
    };
    (dx * size, dy * size)
}

/// Whether the head at `order` of `n` draws the shape mirrored. Never for a
/// fan, whose heads each open towards their own side.
pub fn head_mirrored(e: &MovementEffect, order: usize, n: usize) -> bool {
    !matches!(e.shape, MovementShape::Fan { .. }) && e.mirror.mirrors(order, n)
}

/// Where a head with `spread` (see [`head_spread`]) is on the unit shape at
/// `t` (turns). The shapes are one path for every head, except the fan: all
/// its heads open and close together, each towards its own side.
fn head_shape_point(shape: MovementShape, t: f32, spread: f32) -> (f32, f32) {
    match shape {
        MovementShape::Fan { hold } => (spread * (fan_x(hold, t) + 1.0) / 2.0, 0.0),
        _ => shape_point(shape, t),
    }
}

/// Offset from the center of a head with `spread` at `t` (turns).
pub fn movement_offset(
    e: &MovementEffect,
    t: f32,
    mirrored: bool,
    size: f32,
    spread: f32,
) -> (f32, f32) {
    place(e, head_shape_point(e.shape, t, spread), mirrored, size)
}

/// Pan/tilt in degrees (before the fixture orientation, and before the
/// position below for a relative effect) of a head with phase offset `phase`
/// at cycle position `turns`. Shared with the UI preview.
pub fn movement_point(
    e: &MovementEffect,
    turns: f64,
    phase: f32,
    mirrored: bool,
    spread: f32,
) -> (f32, f32) {
    let (dx, dy) = movement_offset(e, movement_t(e, turns, phase), mirrored, 1.0, spread);
    (e.center_pan + dx, e.center_tilt + dy)
}

/// The point of the whole shape at `t` (turns, 0..=1), whatever the run mode
/// and direction: what the preview draws. For a fan, the line that all its
/// heads cover together.
pub fn movement_path_point(e: &MovementEffect, t: f32, mirrored: bool) -> (f32, f32) {
    let (dx, dy) = place(e, shape_point(e.shape, t), mirrored, 1.0);
    (e.center_pan + dx, e.center_tilt + dy)
}

/// Size of a movement effect `elapsed_us` after it started (its fade in),
/// and with `remaining_us` of its fade out left if it is stopping.
pub fn movement_size(
    e: &MovementEffect,
    bpm: f64,
    elapsed_us: u64,
    remaining_us: Option<u64>,
) -> f32 {
    effect_size(e.fade_in, e.fade_out, bpm, elapsed_us, remaining_us)
}

/// How much of an effect shows (0..=1) `elapsed_us` after it started, with
/// `remaining_us` of its fade out left if it is stopping: the size of a
/// movement, the level of a matrix.
pub fn effect_size(
    fade_in: TimeValue,
    fade_out: TimeValue,
    bpm: f64,
    elapsed_us: u64,
    remaining_us: Option<u64>,
) -> f32 {
    let part = |num: u64, den: u64| {
        if den == 0 {
            1.0
        } else {
            (num.min(den) as f64 / den as f64) as f32
        }
    };
    let grown = part(elapsed_us, fade_in.to_micros(bpm));
    let left = remaining_us.map_or(1.0, |r| part(r, fade_out.to_micros(bpm)));
    grown.min(left)
}

/// Writes the pan and tilt of every head. A relative effect adds its shape
/// to what `below` (the scenes) gives each head, the center of its range
/// where nothing does.
pub fn render_movement(
    m: &CompiledMovement,
    turns: f64,
    size: f32,
    below: &Layer,
    channels: &[LogicalChannel],
    layer: &mut Layer,
) {
    let e = &m.effect;
    for h in &m.heads {
        let (dx, dy) =
            movement_offset(e, movement_t(e, turns, h.phase), h.mirrored, size, h.spread);
        let (mut pan, mut tilt) = (e.center_pan + dx, e.center_tilt + dy);
        if h.orientation.effective_invert_pan() {
            pan = -pan;
        }
        if h.orientation.effective_invert_tilt() {
            tilt = -tilt;
        }
        if h.orientation.swap_pan_tilt {
            std::mem::swap(&mut pan, &mut tilt);
        }
        if e.relative {
            let at = |ch: u32, range: f32| {
                below
                    .get(ch as usize)
                    .map_or(0.0, |v| dmx16_to_degrees(v, range))
            };
            pan += at(h.pan, h.pan_range);
            tilt += at(h.tilt, h.tilt_range);
        }
        if h.pan_range > 0.0 {
            write(layer, channels, h.pan, degrees_to_dmx16(pan, h.pan_range));
        }
        if h.tilt_range > 0.0 {
            write(
                layer,
                channels,
                h.tilt,
                degrees_to_dmx16(tilt, h.tilt_range),
            );
        }
    }
}

/// The color of hue `h` (0..1, red at 0) at full saturation and brightness.
fn hue(h: f32) -> Color {
    let channel = |n: f32| {
        let k = (n + h * 6.0).rem_euclid(6.0);
        1.0 - k.min(4.0 - k).clamp(0.0, 1.0)
    };
    Color {
        r: channel(5.0),
        g: channel(3.0),
        b: channel(1.0),
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

fn mul(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k)
}

/// 3x5 font, rows top to bottom, 3 bits per row.
const FONT: &[(char, &str)] = &[
    ('A', "010101111101101"),
    ('B', "110101110101110"),
    ('C', "011100100100011"),
    ('D', "110101101101110"),
    ('E', "111100110100111"),
    ('F', "111100110100100"),
    ('G', "011100101101011"),
    ('H', "101101111101101"),
    ('I', "111010010010111"),
    ('J', "001001001101010"),
    ('K', "101101110101101"),
    ('L', "100100100100111"),
    ('M', "101111111101101"),
    ('N', "110101101101101"),
    ('O', "010101101101010"),
    ('P', "110101110100100"),
    ('Q', "010101101110011"),
    ('R', "110101110101101"),
    ('S', "011100010001110"),
    ('T', "111010010010010"),
    ('U', "101101101101111"),
    ('V', "101101101101010"),
    ('W', "101101111111101"),
    ('X', "101101010101101"),
    ('Y', "101101010010010"),
    ('Z', "111001010100111"),
    ('0', "111101101101111"),
    ('1', "010110010010111"),
    ('2', "110001010100111"),
    ('3', "110001010001110"),
    ('4', "101101111001001"),
    ('5', "111100110001110"),
    ('6', "011100111101111"),
    ('7', "111001010010010"),
    ('8', "111101111101111"),
    ('9', "111101111001110"),
    ('!', "010010010000010"),
    ('-', "000000111000000"),
];

fn glyph_pixel(c: char, col: u32, row: u32) -> bool {
    if col >= 3 || row >= 5 {
        return false;
    }
    let c = c.to_ascii_uppercase();
    FONT.iter()
        .find(|(g, _)| *g == c)
        .and_then(|(_, bits)| bits.as_bytes().get((row * 3 + col) as usize))
        .is_some_and(|b| *b == b'1')
}

/// Whether the scrolling text lights grid cell (x, y). Characters are 4
/// columns wide (3 + 1 spacing); text scrolls right to left.
fn text_pixel(text: &str, width: u16, height: u16, offset: u32, x: u16, y: u16) -> bool {
    let top = height.saturating_sub(5) / 2;
    let Some(row) = y.checked_sub(top) else {
        return false;
    };
    let col = (u32::from(x) + offset).checked_sub(u32::from(width));
    let Some(col) = col else { return false };
    let idx = (col / 4) as usize;
    let Some(c) = text.chars().nth(idx) else {
        return false;
    };
    glyph_pixel(c, col % 4, u32::from(row))
}

/// Just before the end of a cycle: the last frame of a single shot, the turn
/// of a ping-pong (a phase of 1 would be the start again).
const LAST: f64 = 1.0 - 1e-9;

/// Phase (0..1) of a matrix effect at cycle position `turns`: a single shot
/// stays on its last frame after one cycle, ping-pong plays every other cycle
/// backwards.
pub fn matrix_phase(turns: f64, direction: Direction, run_mode: RunMode) -> f32 {
    let sign = if direction == Direction::Backward {
        -1.0
    } else {
        1.0
    };
    let turns = match run_mode {
        RunMode::Loop => turns,
        RunMode::SingleShot => turns.clamp(0.0, LAST),
        RunMode::PingPong => {
            let u = turns.rem_euclid(2.0);
            (if u > 1.0 { 2.0 - u } else { u }).min(LAST)
        }
    };
    ((turns * sign).rem_euclid(1.0)) as f32
}

/// Where a cell is along the motion of an effect, and how long that is: the
/// column or the row, or for the radial axis its distance from the center
/// of the grid, in rings (the center is the first).
fn along_axis(axis: MatrixAxis, width: u16, height: u16, x: u16, y: u16) -> (f32, f32) {
    match axis {
        MatrixAxis::Horizontal => (f32::from(x), f32::from(width.max(1))),
        MatrixAxis::Vertical => (f32::from(y), f32::from(height.max(1))),
        MatrixAxis::Radial => {
            let (cx, cy) = (
                (f32::from(width.max(1)) - 1.0) / 2.0,
                (f32::from(height.max(1)) - 1.0) / 2.0,
            );
            let distance = (f32::from(x) - cx).hypot(f32::from(y) - cy);
            (distance, cx.hypot(cy).floor() + 1.0)
        }
    }
}

/// Color of the cell (x, y) of a `width` x `height` grid for matrix effect
/// `e` at `phase`, along the effect's axis. Shared with the UI preview.
pub fn matrix_effect_color(
    e: &MatrixEffect,
    width: u16,
    height: u16,
    x: u16,
    y: u16,
    phase: f32,
) -> Color {
    let moves = matches!(
        e.algorithm,
        MatrixAlgorithm::Chase { .. }
            | MatrixAlgorithm::Wave { .. }
            | MatrixAlgorithm::Gradient { .. }
            | MatrixAlgorithm::Fill { .. }
            | MatrixAlgorithm::Rainbow { .. }
    );
    let axis = if moves {
        e.axis
    } else {
        MatrixAxis::Horizontal
    };
    let (along, len) = along_axis(axis, width, height, x, y);
    matrix_cell_color_along(&e.algorithm, width, height, x, y, along, len, phase)
}

/// A number in 0..1 for a cell and a draw, the same every time.
fn draw(x: u16, y: u16, n: u32) -> f32 {
    let mut h = u32::from(x).wrapping_mul(0x9E37_79B1)
        ^ u32::from(y).wrapping_mul(0x85EB_CA77)
        ^ n.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (f64::from(h) / f64::from(u32::MAX)) as f32
}

/// Draws per cycle of a twinkle.
const TWINKLE_DRAWS: f32 = 8.0;

/// Color (linear RGB, clamped) of the grid cell (x, y) at `phase`.
/// Shared with the UI preview.
pub fn matrix_cell_color(
    algorithm: &MatrixAlgorithm,
    width: u16,
    height: u16,
    x: u16,
    y: u16,
    phase: f32,
) -> Color {
    let (along, len) = along_axis(MatrixAxis::Horizontal, width, height, x, y);
    matrix_cell_color_along(algorithm, width, height, x, y, along, len, phase)
}

/// The same, with the place of the cell along the motion (`along`, of `len`)
/// given: the algorithms that move use it, the others the cell (x, y).
#[allow(clippy::too_many_arguments)]
fn matrix_cell_color_along(
    algorithm: &MatrixAlgorithm,
    width: u16,
    height: u16,
    x: u16,
    y: u16,
    along: f32,
    len: f32,
    phase: f32,
) -> Color {
    let w = len.max(1.0);
    let xf = along;
    match algorithm {
        MatrixAlgorithm::Solid { color } => *color,
        MatrixAlgorithm::Chase {
            color,
            background,
            width: band,
        } => {
            // The band starts at the line the phase is on and goes on from
            // the first when it passes the last.
            let lines = w as i32;
            let start = (phase * w).floor() as i32;
            let behind = (xf.floor() as i32 - start).rem_euclid(lines.max(1));
            if behind < i32::from((*band).clamp(1, 16)) {
                *color
            } else {
                *background
            }
        }
        MatrixAlgorithm::Wave { color, wavelength } => {
            let wl = if *wavelength > 0.0 { *wavelength } else { w };
            let k = 0.5 + 0.5 * ((xf / wl - phase) * TAU).sin();
            mul(*color, k)
        }
        MatrixAlgorithm::Rainbow { spread } => {
            let spread = if spread.is_finite() {
                spread.clamp(0.25, 4.0)
            } else {
                1.0
            };
            hue((xf / w * spread + phase).rem_euclid(1.0))
        }
        MatrixAlgorithm::Gradient { from, to } => {
            let u = (xf / w + phase).rem_euclid(1.0);
            let tri = 1.0 - (2.0 * u - 1.0).abs();
            lerp_color(*from, *to, tri)
        }
        MatrixAlgorithm::Text { text, color } => {
            let total = text.chars().count() as u32 * 4 + u32::from(width);
            let offset = (phase * total as f32) as u32;
            if text_pixel(text, width, height, offset, x, y) {
                *color
            } else {
                Color::BLACK
            }
        }
        MatrixAlgorithm::Fill { color, background } => {
            // From no column lit to all of them over the cycle.
            if xf < (phase * (w + 1.0)).floor() {
                *color
            } else {
                *background
            }
        }
        MatrixAlgorithm::Twinkle {
            color,
            background,
            density,
        } => {
            let n = (phase * TWINKLE_DRAWS).floor() as u32;
            if draw(x, y, n) < density.clamp(0.0, 1.0) {
                *color
            } else {
                *background
            }
        }
    }
    .clamped()
}

/// Brightness of a color (0..=1): its brightest component. What a matrix
/// that drives the dimmer gives a cell.
pub fn color_level(c: Color) -> f32 {
    c.r.max(c.g).max(c.b).clamp(0.0, 1.0)
}

/// What the cell (x, y) of matrix effect `e` shows at `phase`: its color, or
/// for an effect that drives the dimmer, the gray of its level (the colors
/// come from the scenes). Shared with the UI preview.
pub fn matrix_cell_look(
    e: &MatrixEffect,
    width: u16,
    height: u16,
    x: u16,
    y: u16,
    phase: f32,
) -> Color {
    let c = matrix_effect_color(e, width, height, x, y, phase);
    match e.controls {
        MatrixControls::Color => c,
        MatrixControls::Dimmer => {
            let l = color_level(c);
            Color { r: l, g: l, b: l }
        }
    }
}

/// Writes every cell: its color at `level` of intensity, or for an effect
/// that drives the dimmer, only its intensity (its level times `level`),
/// leaving the color channels to the scenes.
pub fn render_matrix(
    m: &CompiledMatrix,
    turns: f64,
    level: u16,
    channels: &[LogicalChannel],
    layer: &mut Layer,
) {
    let e = &m.effect;
    let phase = matrix_phase(turns, e.direction, e.run_mode);
    for cell in &m.cells {
        let color = matrix_effect_color(e, m.width, m.height, cell.x, cell.y, phase);
        match e.controls {
            MatrixControls::Color => {
                // The color goes to each light as a scene's does: the
                // calibration first, then the emitters the light has.
                let c = apply_calibration(color, cell.calibration.as_ref());
                if cell.red != NO_CHANNEL || cell.green != NO_CHANNEL || cell.blue != NO_CHANNEL {
                    let mix = additive_mix(c, cell.white != NO_CHANNEL, cell.amber != NO_CHANNEL);
                    write(layer, channels, cell.red, unit_to16(mix.r));
                    write(layer, channels, cell.green, unit_to16(mix.g));
                    write(layer, channels, cell.blue, unit_to16(mix.b));
                    write(layer, channels, cell.white, unit_to16(mix.w));
                    write(layer, channels, cell.amber, unit_to16(mix.a));
                } else if cell.cyan != NO_CHANNEL
                    || cell.magenta != NO_CHANNEL
                    || cell.yellow != NO_CHANNEL
                {
                    write(layer, channels, cell.cyan, unit_to16(1.0 - c.r));
                    write(layer, channels, cell.magenta, unit_to16(1.0 - c.g));
                    write(layer, channels, cell.yellow, unit_to16(1.0 - c.b));
                } else if let Some(v) = nearest_wheel_value(c, &cell.wheel) {
                    write(layer, channels, cell.wheel_channel, v);
                }
                write(layer, channels, cell.intensity, scale(u16::MAX, level));
            }
            MatrixControls::Dimmer => {
                let v = scale(unit_to16(color_level(color)), level);
                write(layer, channels, cell.intensity, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_stay_in_unit_square() {
        for shape in [
            MovementShape::Circle,
            MovementShape::Eight,
            MovementShape::Line,
            MovementShape::Polygon { sides: 4 },
            MovementShape::Polygon { sides: 5 },
            MovementShape::Sine { periods: 3 },
            MovementShape::Fan { hold: 10 },
            MovementShape::Lissajous {
                x_freq: 3,
                y_freq: 2,
            },
        ] {
            for i in 0..100 {
                let (x, y) = shape_point(shape, i as f32 / 100.0);
                assert!((-1.0001..=1.0001).contains(&x) && (-1.0001..=1.0001).contains(&y));
            }
        }
    }

    #[test]
    fn font_renders_i() {
        assert!(glyph_pixel('i', 1, 2));
        assert!(!glyph_pixel('I', 0, 2));
    }
}
