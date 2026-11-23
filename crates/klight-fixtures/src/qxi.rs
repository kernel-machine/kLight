//! QLC+ input profile (`.qxi`) parser: what a controller has (channels with
//! their type and feedback), the colors its LEDs can show and the LED
//! behaviours its feedback channels select.
//!
//! Tolerant like the `.qxf` parser: unknown elements are ignored and
//! malformed entries are skipped. Only a wrong root element or malformed XML
//! is an error.

use klight_model::Rgb8;

use crate::xml::{self, Element};
use crate::QxfError;

/// Feedback range of a channel (LED value range) and the MIDI channel the
/// feedback must be sent on, when the controller needs a specific one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QxiFeedback {
    pub lower: u8,
    pub upper: u8,
    pub midi_channel: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QxiChannel {
    /// QLC+ numbering: for MIDI, controllers are `0..=127` and notes start at 128.
    pub number: u16,
    pub name: String,
    /// `Button`, `Slider`, `Knob`, ...
    pub kind: String,
    pub feedback: Option<QxiFeedback>,
}

/// An LED color: the value to send and the color it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QxiColor {
    pub value: u8,
    pub label: String,
    pub rgb: Rgb8,
}

/// An LED behaviour selected by the MIDI channel of the feedback message
/// (e.g. "On 100% brightness", "Pulsing 1/4").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QxiBehavior {
    pub midi_channel: u8,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputProfileDef {
    pub manufacturer: String,
    pub model: String,
    pub channels: Vec<QxiChannel>,
    pub colors: Vec<QxiColor>,
    pub behaviors: Vec<QxiBehavior>,
}

impl InputProfileDef {
    /// `Manufacturer Model`.
    pub fn name(&self) -> String {
        format!("{} {}", self.manufacturer, self.model)
            .trim()
            .to_owned()
    }

    /// Whether the controller can show anything (colors, behaviours or at
    /// least a feedback range).
    pub fn has_feedback(&self) -> bool {
        !self.colors.is_empty()
            || !self.behaviors.is_empty()
            || self.channels.iter().any(|c| c.feedback.is_some())
    }

    /// The MIDI feedback channel the controller's own channels ask for (the
    /// most common one), if any.
    pub fn feedback_midi_channel(&self) -> Option<u8> {
        self.channels
            .iter()
            .filter_map(|c| c.feedback.and_then(|f| f.midi_channel))
            .next()
    }
}

/// Parses the text of a `.qxi` file.
pub fn parse_qxi(input: &str) -> Result<InputProfileDef, QxfError> {
    let root = xml::parse(input)?;
    if root.name != "InputProfile" {
        return Err(QxfError::Invalid(format!(
            "root element is <{}>, expected <InputProfile>",
            root.name
        )));
    }
    let channels: Vec<QxiChannel> = root
        .children_named("Channel")
        .filter_map(parse_channel)
        .collect();
    let colors: Vec<QxiColor> = root
        .child("ColorTable")
        .map(|t| t.children_named("Color").filter_map(parse_color).collect())
        .unwrap_or_default();
    let behaviors: Vec<QxiBehavior> = root
        .child("MidiChannelTable")
        .map(|t| {
            t.children_named("Channel")
                .filter_map(parse_behavior)
                .collect()
        })
        .unwrap_or_default();
    Ok(InputProfileDef {
        manufacturer: root.child_text("Manufacturer").unwrap_or("").to_owned(),
        model: root.child_text("Model").unwrap_or("").to_owned(),
        channels,
        colors,
        behaviors,
    })
}

fn parse_channel(e: &Element) -> Option<QxiChannel> {
    let number = e.attr("Number")?.trim().parse().ok()?;
    let feedback = e.child("Feedback").map(|f| QxiFeedback {
        lower: f.attr("LowerValue").and_then(parse_u8).unwrap_or(0),
        upper: f.attr("UpperValue").and_then(parse_u8).unwrap_or(127),
        midi_channel: f.attr("MidiChannel").and_then(parse_u8),
    });
    Some(QxiChannel {
        number,
        name: e.child_text("Name").unwrap_or("").to_owned(),
        kind: e.child_text("Type").unwrap_or("").to_owned(),
        feedback,
    })
}

fn parse_color(e: &Element) -> Option<QxiColor> {
    Some(QxiColor {
        value: parse_u8(e.attr("Value")?)?,
        label: e.attr("Label").unwrap_or("").to_owned(),
        rgb: Rgb8::parse_hex(e.attr("RGB")?)?,
    })
}

fn parse_behavior(e: &Element) -> Option<QxiBehavior> {
    Some(QxiBehavior {
        midi_channel: parse_u8(e.attr("Value")?)?,
        label: e.attr("Label").unwrap_or("").to_owned(),
    })
}

fn parse_u8(s: &str) -> Option<u8> {
    s.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MK2: &str = r##"<?xml version='1.0' encoding='UTF-8'?>
<!DOCTYPE InputProfile>
<InputProfile xmlns="http://www.qlcplus.org/InputProfile">
 <Manufacturer>Akai</Manufacturer>
 <Model>APC Mini mk2</Model>
 <Type>MIDI</Type>
 <Channel Number="48"><Name>Slider 1</Name><Type>Slider</Type></Channel>
 <Channel Number="128">
  <Name>Button 1-8</Name>
  <Type>Button</Type>
  <Feedback LowerValue="1" UpperValue="26" MidiChannel="6"/>
 </Channel>
 <Channel Number="bad"><Name>skipped</Name></Channel>
 <ColorTable>
  <Color Value="0" Label="Black" RGB="#000000"/>
  <Color Value="10" Label="Red" RGB="#ff0000"/>
  <Color Value="999" Label="Out of range" RGB="#ffffff"/>
  <Color Value="11" Label="Bad color" RGB="nope"/>
 </ColorTable>
 <MidiChannelTable>
  <Channel Value="0" Label="On 10% brightness"/>
  <Channel Value="6" Label="On 100% brightness"/>
  <Channel Value="7" Label="Pulsing 1/16"/>
 </MidiChannelTable>
</InputProfile>"##;

    #[test]
    fn parses_channels_colors_and_behaviors() {
        let p = parse_qxi(MK2).unwrap();
        assert_eq!(p.name(), "Akai APC Mini mk2");
        assert_eq!(p.channels.len(), 2, "the malformed channel is skipped");
        assert_eq!(
            p.channels.get(1).and_then(|c| c.feedback),
            Some(QxiFeedback {
                lower: 1,
                upper: 26,
                midi_channel: Some(6)
            })
        );
        assert_eq!(
            p.colors.len(),
            2,
            "out-of-range and malformed colors are skipped"
        );
        assert_eq!(p.colors.get(1).map(|c| c.rgb), Some(Rgb8::new(255, 0, 0)));
        assert_eq!(
            p.behaviors
                .iter()
                .map(|b| (b.midi_channel, b.label.as_str()))
                .collect::<Vec<_>>(),
            [
                (0, "On 10% brightness"),
                (6, "On 100% brightness"),
                (7, "Pulsing 1/16")
            ]
        );
        assert!(p.has_feedback());
        assert_eq!(p.feedback_midi_channel(), Some(6));
    }

    #[test]
    fn wrong_root_and_garbage_are_errors() {
        assert!(parse_qxi("<FixtureDefinition/>").is_err());
        assert!(parse_qxi("<InputProfile>").is_err());
        assert!(parse_qxi("").is_err());
    }

    #[test]
    fn profile_without_feedback_reports_it() {
        let p = parse_qxi("<InputProfile><Model>x</Model><Channel Number=\"1\"><Name>a</Name></Channel></InputProfile>").unwrap();
        assert!(!p.has_feedback());
    }

    #[test]
    fn every_downloaded_profile_parses() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/qlcplus");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("SKIPPED: profiles/qlcplus not downloaded");
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "qxi") {
                let text = std::fs::read_to_string(&p).unwrap();
                parse_qxi(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            }
        }
    }
}
