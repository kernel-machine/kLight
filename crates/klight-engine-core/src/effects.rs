//! Movement (EFX) and matrix effects. They work in physical units (degrees,
//! normalised RGB) and are converted to DMX only when written to the layer.

use std::f32::consts::TAU;

use klight_fixtures::convert::{degrees_to_dmx16, unit_to16};
use klight_model::function::{Direction, MovementShape};
use klight_model::{Color, MatrixAlgorithm, MovementEffect, TimeValue};

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
        MovementShape::Square => {
            let u = t.rem_euclid(1.0) * 4.0;
            let f = u.fract() * 2.0 - 1.0;
            match u as u32 {
                0 => (f, -1.0),
                1 => (1.0, f),
                2 => (-f, 1.0),
                _ => (-1.0, -f),
            }
        }
        MovementShape::Lissajous { x_freq, y_freq } => (
            (f32::from(x_freq.max(1)) * a).sin(),
            (f32::from(y_freq.max(1)) * a).sin(),
        ),
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

/// Phase offset (turns) of head `order` of `n` for a phase spread in degrees.
pub fn head_phase(phase_spread: f32, order: usize, n: usize) -> f32 {
    if n == 0 {
        return 0.0;
    }
    (phase_spread / 360.0) * order as f32 / n as f32
}

/// Pan/tilt in degrees (before the fixture orientation) of a head with
/// phase offset `phase` at cycle position `turns`. Shared with the UI preview.
pub fn movement_point(e: &MovementEffect, turns: f64, phase: f32) -> (f32, f32) {
    let sign = if e.direction == Direction::Backward {
        -1.0
    } else {
        1.0
    };
    let t = ((turns * sign) as f32 + phase).rem_euclid(1.0);
    let (x, y) = shape_point(e.shape, t);
    (
        e.center_pan + x * e.width / 2.0,
        e.center_tilt + y * e.height / 2.0,
    )
}

pub fn render_movement(
    m: &CompiledMovement,
    turns: f64,
    channels: &[LogicalChannel],
    layer: &mut Layer,
) {
    for h in &m.heads {
        let (mut pan, mut tilt) = movement_point(&m.effect, turns, h.phase);
        if h.orientation.effective_invert_pan() {
            pan = -pan;
        }
        if h.orientation.effective_invert_tilt() {
            tilt = -tilt;
        }
        if h.orientation.swap_pan_tilt {
            std::mem::swap(&mut pan, &mut tilt);
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

/// Phase (0..1) of a matrix effect at cycle position `turns`.
pub fn matrix_phase(turns: f64, direction: Direction) -> f32 {
    let sign = if direction == Direction::Backward {
        -1.0
    } else {
        1.0
    };
    ((turns * sign).rem_euclid(1.0)) as f32
}

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
    let w = f32::from(width.max(1));
    let xf = f32::from(x);
    match algorithm {
        MatrixAlgorithm::Solid { color } => *color,
        MatrixAlgorithm::Chase { color, background } => {
            let col = (phase * w).floor() as u16;
            if x == col {
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
    }
    .clamped()
}

pub fn render_matrix(
    m: &CompiledMatrix,
    turns: f64,
    level: u16,
    channels: &[LogicalChannel],
    layer: &mut Layer,
) {
    let phase = matrix_phase(turns, m.direction);
    for cell in &m.cells {
        let color = matrix_cell_color(&m.algorithm, m.width, m.height, cell.x, cell.y, phase);
        write(layer, channels, cell.red, unit_to16(color.r));
        write(layer, channels, cell.green, unit_to16(color.g));
        write(layer, channels, cell.blue, unit_to16(color.b));
        write(layer, channels, cell.white, 0);
        write(layer, channels, cell.intensity, scale(u16::MAX, level));
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
            MovementShape::Square,
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
