//! QLC+ fixture definition (`.qxf`) parser.
//!
//! Tolerant: unknown elements and presets are ignored, malformed numbers fall
//! back to defaults. Structural problems (no channels, no modes, a mode that
//! references an unknown channel) produce a descriptive [`QxfError`].

use klight_model::fixture_def::ServiceAction;
use klight_model::{
    Attribute, Capability, CapabilityKind, ChannelByte, ChannelDef, FixtureDefinition, FixtureKind,
    HeadDef, ModeDef, Physical, Rgb8,
};

use crate::xml::{self, Element};
use crate::QxfError;

/// Parses the text of a `.qxf` file.
pub fn parse_qxf(input: &str) -> Result<FixtureDefinition, QxfError> {
    let root = xml::parse(input)?;
    if root.name != "FixtureDefinition" {
        return Err(QxfError::Invalid(format!(
            "root element is <{}>, expected <FixtureDefinition>",
            root.name
        )));
    }
    let manufacturer = root.child_text("Manufacturer").unwrap_or("").to_owned();
    let model = root.child_text("Model").unwrap_or("").to_owned();
    if manufacturer.is_empty() || model.is_empty() {
        return Err(QxfError::Invalid(
            "missing <Manufacturer> or <Model>".into(),
        ));
    }
    let kind = parse_kind(root.child_text("Type").unwrap_or(""));

    let channels: Vec<ChannelDef> = root.children_named("Channel").map(parse_channel).collect();
    if channels.is_empty() {
        return Err(QxfError::Invalid("definition has no channels".into()));
    }
    if channels.len() > usize::from(u16::MAX) {
        return Err(QxfError::TooLarge);
    }

    let physical = root
        .child("Physical")
        .map(parse_physical)
        .unwrap_or_default();

    let mut modes = Vec::new();
    for m in root.children_named("Mode") {
        modes.push(parse_mode(m, &channels)?);
    }
    if modes.is_empty() {
        return Err(QxfError::Invalid("definition has no modes".into()));
    }

    Ok(FixtureDefinition {
        manufacturer,
        model,
        kind,
        channels,
        modes,
        physical,
    })
}

fn parse_kind(s: &str) -> FixtureKind {
    match s.trim() {
        "Dimmer" => FixtureKind::Dimmer,
        "Color Changer" => FixtureKind::ColorChanger,
        "Moving Head" => FixtureKind::MovingHead,
        "Scanner" => FixtureKind::Scanner,
        "Flower" => FixtureKind::Flower,
        "Effect" => FixtureKind::Effect,
        "Strobe" => FixtureKind::Strobe,
        "Smoke" => FixtureKind::Smoke,
        "Hazer" => FixtureKind::Hazer,
        "Fan" => FixtureKind::Fan,
        "Laser" => FixtureKind::Laser,
        "LED Bar (Beams)" => FixtureKind::LedBarBeams,
        "LED Bar (Pixels)" => FixtureKind::LedBarPixels,
        _ => FixtureKind::Other,
    }
}

fn parse_u8(s: Option<&str>) -> Option<u8> {
    s.and_then(|v| v.trim().parse::<u32>().ok())
        .map(|v| u8::try_from(v.min(255)).unwrap_or(u8::MAX))
}

fn parse_f32(s: Option<&str>) -> Option<f32> {
    s.and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
}

/// Attribute and byte from a QLC+ channel preset name.
fn preset_attribute(preset: &str) -> Option<(Attribute, ChannelByte)> {
    let (base, byte) = match preset.strip_suffix("Fine") {
        Some(b) => (b, ChannelByte::Fine),
        None => (preset, ChannelByte::Coarse),
    };
    let attr = match base {
        "IntensityMasterDimmer" | "IntensityDimmer" => Attribute::Dimmer,
        "IntensityRed" => Attribute::Red,
        "IntensityGreen" => Attribute::Green,
        "IntensityBlue" => Attribute::Blue,
        "IntensityCyan" => Attribute::Cyan,
        "IntensityMagenta" => Attribute::Magenta,
        "IntensityYellow" => Attribute::Yellow,
        "IntensityAmber" => Attribute::Amber,
        "IntensityWhite" => Attribute::White,
        "IntensityUV" => Attribute::Uv,
        "IntensityIndigo" => Attribute::Indigo,
        "IntensityLime" => Attribute::Lime,
        "PositionPan" | "PositionXAxis" => Attribute::Pan,
        "PositionTilt" | "PositionYAxis" => Attribute::Tilt,
        "ColorMacro" | "ColorWheel" => Attribute::ColorWheel,
        "ColorCTOMixer" | "ColorCTBMixer" | "ColorCTCMixer" => Attribute::ColorTemperature,
        "GoboWheel" | "GoboIndex" => Attribute::Gobo,
        "ShutterStrobeSlowFast" | "ShutterStrobeFastSlow" => Attribute::Shutter,
        "ShutterIrisMinToMax" | "ShutterIrisMaxToMin" | "ShutterIris" => Attribute::Iris,
        "BeamFocusNearFar" | "BeamFocusFarNear" | "BeamFocus" => Attribute::Focus,
        "BeamZoomSmallBig" | "BeamZoomBigSmall" | "BeamZoom" => Attribute::Zoom,
        "PrismRotationSlowFast" | "PrismRotationFastSlow" => Attribute::PrismRotation,
        "NoFunction" => Attribute::NoFunction,
        p if p.starts_with("Speed") => Attribute::Speed,
        p if p.starts_with("Intensity") => Attribute::Other,
        _ => return None,
    };
    Some((attr, byte))
}

fn group_attribute(group: &str, colour: Option<&str>, name: &str) -> Attribute {
    let lname = name.to_ascii_lowercase();
    match group.trim() {
        "Intensity" => match colour.map(str::trim) {
            Some("Red") => Attribute::Red,
            Some("Green") => Attribute::Green,
            Some("Blue") => Attribute::Blue,
            Some("Cyan") => Attribute::Cyan,
            Some("Magenta") => Attribute::Magenta,
            Some("Yellow") => Attribute::Yellow,
            Some("Amber") => Attribute::Amber,
            Some("White") => Attribute::White,
            Some("UV") => Attribute::Uv,
            Some("Lime") => Attribute::Lime,
            Some("Indigo") => Attribute::Indigo,
            _ => Attribute::Dimmer,
        },
        "Colour" => Attribute::ColorWheel,
        "Gobo" => {
            if lname.contains("rot") {
                Attribute::GoboRotation
            } else {
                Attribute::Gobo
            }
        }
        "Prism" => {
            if lname.contains("rot") {
                Attribute::PrismRotation
            } else {
                Attribute::Prism
            }
        }
        "Shutter" => {
            if lname.contains("iris") {
                Attribute::Iris
            } else {
                Attribute::Shutter
            }
        }
        "Pan" => Attribute::Pan,
        "Tilt" => Attribute::Tilt,
        "Speed" => Attribute::Speed,
        "Effect" => Attribute::Effect,
        "Maintenance" => Attribute::Maintenance,
        "Nothing" => Attribute::NoFunction,
        "Beam" => {
            if lname.contains("zoom") {
                Attribute::Zoom
            } else if lname.contains("focus") {
                Attribute::Focus
            } else if lname.contains("iris") {
                Attribute::Iris
            } else if lname.contains("frost") {
                Attribute::Frost
            } else {
                Attribute::Other
            }
        }
        _ => Attribute::Other,
    }
}

fn parse_channel(el: &Element) -> ChannelDef {
    let name = el.attr("Name").unwrap_or("").to_owned();
    let default = parse_u8(el.attr("Default")).unwrap_or(0);

    let (mut attribute, mut byte) = el
        .attr("Preset")
        .and_then(preset_attribute)
        .unwrap_or((Attribute::Other, ChannelByte::Coarse));

    if el.attr("Preset").and_then(preset_attribute).is_none() {
        if let Some(g) = el.child("Group") {
            attribute = group_attribute(&g.text, el.child_text("Colour"), &name);
            byte = match g.attr("Byte").map(str::trim) {
                Some("1") => ChannelByte::Fine,
                _ => ChannelByte::Coarse,
            };
        }
    }

    let capabilities = el
        .children_named("Capability")
        .map(|c| parse_capability(c, attribute))
        .collect();

    ChannelDef {
        name,
        attribute,
        byte,
        default,
        capabilities,
    }
}

fn parse_capability(el: &Element, attribute: Attribute) -> Capability {
    let min = parse_u8(el.attr("Min")).unwrap_or(0);
    let max = parse_u8(el.attr("Max")).unwrap_or(255);
    let name = el.text.trim().to_owned();
    let lname = name.to_ascii_lowercase();
    let preset = el.attr("Preset").unwrap_or("");

    let color1 = el
        .attr("Res1")
        .and_then(Rgb8::parse_hex)
        .or_else(|| el.attr("Color").and_then(Rgb8::parse_hex));
    let color2 = el
        .attr("Res2")
        .and_then(Rgb8::parse_hex)
        .or_else(|| el.attr("Color2").and_then(Rgb8::parse_hex));

    let kind = match preset {
        "ColorMacro" | "ColorDoubleMacro" => match color1 {
            Some(color) => CapabilityKind::Color {
                color,
                color2: if preset == "ColorDoubleMacro" {
                    color2
                } else {
                    None
                },
            },
            None => CapabilityKind::Generic,
        },
        "GoboMacro" | "GoboShakeMacro" => CapabilityKind::Gobo,
        "ShutterOpen" => CapabilityKind::ShutterOpen,
        "ShutterClose" => CapabilityKind::ShutterClosed,
        "StrobeFrequency" => {
            let hz = parse_f32(el.attr("Res1"));
            CapabilityKind::Strobe {
                hz_min: hz,
                hz_max: hz,
            }
        }
        "StrobeFreqRange" => CapabilityKind::Strobe {
            hz_min: parse_f32(el.attr("Res1")),
            hz_max: parse_f32(el.attr("Res2")),
        },
        p if p.starts_with("Strobe") || p.starts_with("Pulse") => CapabilityKind::Strobe {
            hz_min: None,
            hz_max: None,
        },
        "PrismEffectOn" => CapabilityKind::PrismOn,
        "LampOn" => CapabilityKind::Service {
            action: ServiceAction::LampOn,
        },
        "LampOff" => CapabilityKind::Service {
            action: ServiceAction::LampOff,
        },
        p if p.starts_with("Reset") => CapabilityKind::Service {
            action: ServiceAction::Reset,
        },
        _ => infer_capability(attribute, &lname, color1, color2, el),
    };

    Capability {
        min,
        max,
        name,
        kind,
    }
}

/// Infers the meaning of a capability without preset (older files).
fn infer_capability(
    attribute: Attribute,
    lname: &str,
    color1: Option<Rgb8>,
    color2: Option<Rgb8>,
    el: &Element,
) -> CapabilityKind {
    if let Some(color) = color1 {
        return CapabilityKind::Color { color, color2 };
    }
    match attribute {
        Attribute::ColorWheel if lname == "white" || lname.contains("open") => {
            CapabilityKind::Color {
                color: Rgb8::new(255, 255, 255),
                color2: None,
            }
        }
        Attribute::Gobo => {
            if lname.contains("open") {
                CapabilityKind::Open
            } else if el.attr("Res").is_some() || !lname.contains("rot") {
                CapabilityKind::Gobo
            } else {
                CapabilityKind::Generic
            }
        }
        Attribute::Prism => {
            if lname.contains("open") || lname.contains("off") || lname.contains("no prism") {
                CapabilityKind::Open
            } else {
                CapabilityKind::PrismOn
            }
        }
        Attribute::Shutter => {
            if lname.contains("strob") {
                CapabilityKind::Strobe {
                    hz_min: None,
                    hz_max: None,
                }
            } else if lname.contains("open") || lname == "on" {
                CapabilityKind::ShutterOpen
            } else if lname.contains("close") || lname.contains("blackout") || lname == "off" {
                CapabilityKind::ShutterClosed
            } else {
                CapabilityKind::Generic
            }
        }
        Attribute::Maintenance => {
            if lname.contains("lamp on") {
                CapabilityKind::Service {
                    action: ServiceAction::LampOn,
                }
            } else if lname.contains("lamp off") {
                CapabilityKind::Service {
                    action: ServiceAction::LampOff,
                }
            } else if lname.contains("reset") {
                CapabilityKind::Service {
                    action: ServiceAction::Reset,
                }
            } else {
                CapabilityKind::Generic
            }
        }
        _ => CapabilityKind::Generic,
    }
}

fn parse_physical(el: &Element) -> Physical {
    let mut p = Physical::default();
    if let Some(f) = el.child("Focus") {
        p.pan_max_deg = parse_f32(f.attr("PanMax")).unwrap_or(0.0).max(0.0);
        p.tilt_max_deg = parse_f32(f.attr("TiltMax")).unwrap_or(0.0).max(0.0);
    }
    if let Some(l) = el.child("Lens") {
        p.zoom_min_deg = parse_f32(l.attr("DegreesMin")).unwrap_or(0.0).max(0.0);
        p.zoom_max_deg = parse_f32(l.attr("DegreesMax")).unwrap_or(0.0).max(0.0);
    }
    p
}

fn parse_mode(el: &Element, channels: &[ChannelDef]) -> Result<ModeDef, QxfError> {
    let name = el.attr("Name").unwrap_or("").trim().to_owned();
    if name.is_empty() {
        return Err(QxfError::Invalid("mode without name".into()));
    }
    let mut entries: Vec<(u32, u16)> = Vec::new();
    for (pos, c) in el.children_named("Channel").enumerate() {
        let number = c
            .attr("Number")
            .and_then(|n| n.trim().parse::<u32>().ok())
            .unwrap_or(u32::try_from(pos).unwrap_or(u32::MAX));
        let cname = c.text.trim();
        let idx = channels
            .iter()
            .position(|ch| ch.name.trim() == cname)
            .ok_or_else(|| {
                QxfError::Invalid(format!(
                    "mode '{name}' references unknown channel '{cname}'"
                ))
            })?;
        let idx = u16::try_from(idx).map_err(|_| QxfError::TooLarge)?;
        entries.push((number, idx));
    }
    if entries.is_empty() {
        return Err(QxfError::Invalid(format!("mode '{name}' has no channels")));
    }
    if entries.len() > klight_model::UNIVERSE_SIZE {
        return Err(QxfError::Invalid(format!(
            "mode '{name}' has more than 512 channels"
        )));
    }
    entries.sort_by_key(|(n, _)| *n);
    let mode_channels: Vec<u16> = entries.into_iter().map(|(_, i)| i).collect();
    let len = mode_channels.len();

    let mut heads = Vec::new();
    for h in el.children_named("Head") {
        let mut head = HeadDef::default();
        for c in h.children_named("Channel") {
            if let Ok(n) = c.text.trim().parse::<u16>() {
                if usize::from(n) < len && !head.channels.contains(&n) {
                    head.channels.push(n);
                }
            }
        }
        if !head.channels.is_empty() {
            heads.push(head);
        }
    }

    let physical = el.child("Physical").map(parse_physical);

    Ok(ModeDef {
        name,
        channels: mode_channels,
        heads,
        physical,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RGB: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE FixtureDefinition>
<FixtureDefinition xmlns="http://www.qlcplus.org/FixtureDefinition">
 <Creator><Name>Q Light Controller Plus</Name></Creator>
 <Manufacturer>Generic</Manufacturer>
 <Model>RGB Par</Model>
 <Type>Color Changer</Type>
 <Channel Name="Dimmer" Preset="IntensityMasterDimmer"/>
 <Channel Name="Red" Preset="IntensityRed"/>
 <Channel Name="Green"><Group Byte="0">Intensity</Group><Colour>Green</Colour></Channel>
 <Channel Name="Blue" Preset="IntensityBlue"/>
 <Channel Name="Strobe">
  <Group Byte="0">Shutter</Group>
  <Capability Min="0" Max="9" Preset="ShutterOpen">Open</Capability>
  <Capability Min="10" Max="255" Preset="StrobeFreqRange" Res1="1" Res2="20">Strobe 1-20Hz</Capability>
 </Channel>
 <Mode Name="4 Channel">
  <Channel Number="0">Red</Channel>
  <Channel Number="1">Green</Channel>
  <Channel Number="2">Blue</Channel>
  <Channel Number="3">Dimmer</Channel>
 </Mode>
 <Mode Name="5 Channel">
  <Channel Number="0">Dimmer</Channel>
  <Channel Number="1">Red</Channel>
  <Channel Number="2">Green</Channel>
  <Channel Number="3">Blue</Channel>
  <Channel Number="4">Strobe</Channel>
 </Mode>
</FixtureDefinition>"##;

    #[test]
    fn parses_rgb_par() {
        let d = parse_qxf(RGB).unwrap();
        assert_eq!(d.manufacturer, "Generic");
        assert_eq!(d.model, "RGB Par");
        assert_eq!(d.kind, FixtureKind::ColorChanger);
        assert_eq!(d.channels.len(), 5);
        assert_eq!(d.channels[2].attribute, Attribute::Green);
        assert_eq!(d.modes.len(), 2);
        assert_eq!(d.modes[0].channels, vec![1, 2, 3, 0]);
        let strobe = &d.channels[4];
        assert_eq!(strobe.attribute, Attribute::Shutter);
        assert_eq!(
            strobe.capabilities[1].kind,
            CapabilityKind::Strobe {
                hz_min: Some(1.0),
                hz_max: Some(20.0)
            }
        );
    }

    #[test]
    fn rejects_unknown_channel_in_mode() {
        let bad = RGB.replace(
            "<Channel Number=\"3\">Dimmer</Channel>",
            "<Channel Number=\"3\">Nope</Channel>",
        );
        let err = parse_qxf(&bad).unwrap_err();
        assert!(err.to_string().contains("Nope"), "{err}");
    }

    #[test]
    fn channel_names_match_ignoring_surrounding_spaces() {
        // Regression: real QLC+ files have names like `Name="Red "`.
        let spaced = RGB.replace(
            "<Channel Name=\"Red\" Preset",
            "<Channel Name=\"Red \" Preset",
        );
        assert!(spaced.contains("\"Red \""));
        let d = parse_qxf(&spaced).unwrap();
        assert_eq!(d.modes[0].channels, vec![1, 2, 3, 0]);
    }

    #[test]
    fn rejects_garbage_without_panic() {
        for input in [
            "",
            "<",
            "<a>",
            "<FixtureDefinition>",
            "<x/><y/>",
            "\u{0}\u{1}",
            "<FixtureDefinition><Manufacturer>a</Manufacturer><Model>b</Model></FixtureDefinition>",
        ] {
            assert!(parse_qxf(input).is_err(), "{input:?}");
        }
    }
}
