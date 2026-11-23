//! Named color sequences: `n` bright colors spread evenly around the hue
//! wheel (3 → every 120°, 4 → every 90°, 5 → every 72°, ...), starting at
//! red, for generating one scene per color.
//!
//! Colors are fully saturated and fully bright, so a fixture never gets a
//! dark color that looks switched off. Each color is named after the nearest
//! node of a table of named hues.

use klight_model::attributes::srgb_to_linear;
use klight_model::Color;

/// A color with the name it is shown under.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NamedColor {
    pub name: &'static str,
    /// Linear RGB, like every other color in the core.
    pub color: Color,
}

/// Named hues, one every 30° on the usual color wheel (red 0°, green 120°,
/// blue 240°). With one node per 30° and at most [`MAX_SEQUENCE_COLORS`]
/// colors, two colors of a sequence never get the same name.
const NODES: [(&str, f32); 12] = [
    ("Red", 0.0),
    ("Orange", 30.0),
    ("Yellow", 60.0),
    ("Lime", 90.0),
    ("Green", 120.0),
    ("Spring Green", 150.0),
    ("Cyan", 180.0),
    ("Azure", 210.0),
    ("Blue", 240.0),
    ("Violet", 270.0),
    ("Magenta", 300.0),
    ("Rose", 330.0),
];

/// Upper bound of [`color_sequence`]: the number of named hues.
pub const MAX_SEQUENCE_COLORS: usize = NODES.len();

/// Darkest OKLab lightness of any generated color (pure blue is about 0.45).
/// Checked by tests.
pub const MIN_LIGHTNESS: f32 = 0.4;

/// Smallest angle between two hues, in degrees.
fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Name of the node closest to `hue` (degrees); ties go to the earlier node.
fn nearest_name(hue: f32) -> &'static str {
    let mut best: Option<(&'static str, f32)> = None;
    for &(name, node) in &NODES {
        let d = hue_distance(hue, node);
        if best.is_none_or(|(_, bd)| d < bd - 1e-3) {
            best = Some((name, d));
        }
    }
    best.map_or("Red", |(name, _)| name)
}

/// Fully saturated, fully bright color of a hue in degrees (sRGB wheel),
/// as linear RGB.
fn hue_to_color(hue: f32) -> Color {
    let sector = hue.rem_euclid(360.0) / 60.0;
    // The middle channel rises or falls inside each 60° sector.
    let x = 1.0 - (sector.rem_euclid(2.0) - 1.0).abs();
    let (r, g, b) = match sector as u8 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    Color::new(srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b))
}

/// `n` bright colors evenly spread over the hue wheel from red, each named
/// after the nearest named hue. `n` is capped at [`MAX_SEQUENCE_COLORS`].
pub fn color_sequence(n: usize) -> Vec<NamedColor> {
    let n = n.min(MAX_SEQUENCE_COLORS);
    (0..n)
        .map(|i| {
            let hue = 360.0 * i as f32 / n as f32;
            NamedColor {
                name: nearest_name(hue),
                color: hue_to_color(hue),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::linear_to_oklab;

    fn names(n: usize) -> Vec<&'static str> {
        color_sequence(n).iter().map(|c| c.name).collect()
    }

    #[test]
    fn evenly_spaced_hues() {
        assert_eq!(names(3), ["Red", "Green", "Blue"]);
        assert_eq!(names(4), ["Red", "Lime", "Cyan", "Violet"]);
        assert_eq!(color_sequence(1).first().map(|c| c.color), Some(Color::RED));
    }

    #[test]
    fn twelve_colors_are_the_twelve_nodes() {
        let all: Vec<&str> = NODES.iter().map(|&(name, _)| name).collect();
        assert_eq!(names(12), all);
    }

    #[test]
    fn counts_and_cap() {
        assert!(color_sequence(0).is_empty());
        for n in 1..=MAX_SEQUENCE_COLORS {
            assert_eq!(color_sequence(n).len(), n);
        }
        assert_eq!(color_sequence(1000).len(), MAX_SEQUENCE_COLORS);
    }

    #[test]
    fn names_are_unique() {
        for n in 1..=MAX_SEQUENCE_COLORS {
            let mut v = names(n);
            v.sort_unstable();
            v.dedup();
            assert_eq!(v.len(), n, "n = {n}");
        }
    }

    #[test]
    fn colors_are_never_dark_and_never_white() {
        for n in 1..=MAX_SEQUENCE_COLORS {
            for c in color_sequence(n) {
                let max = c.color.r.max(c.color.g).max(c.color.b);
                let min = c.color.r.min(c.color.g).min(c.color.b);
                assert!((max - 1.0).abs() < 1e-6, "{} is not at full level", c.name);
                assert!(min < 1e-6, "{} is not fully saturated", c.name);
                let l = linear_to_oklab(c.color).l;
                assert!(l >= MIN_LIGHTNESS, "{} is too dark ({l})", c.name);
            }
        }
    }

    #[test]
    fn nearest_name_wraps_around_the_wheel() {
        assert_eq!(nearest_name(344.0), "Rose");
        assert_eq!(nearest_name(346.0), "Red");
        assert_eq!(nearest_name(10.0), "Red");
    }
}
