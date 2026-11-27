//! Read-only views of the patch used by clients (through `klight-wasm`) and
//! by the core. One implementation, so the UI never re-implements them.

use serde::{Deserialize, Serialize};

use crate::attributes::{AttributeValue, Color};
use crate::fixture_def::{Attribute, CapabilityKind, ChannelByte};
use crate::function::{
    ChannelRef, Chaser, FunctionKind, Scene, SceneValue, Target, TimeValue, ValueSource,
};
use crate::ids::{FixtureId, FunctionId, UniverseId};
use crate::patch::{PatchedFixture, UNIVERSE_SIZE};
use crate::show::Show;
use crate::validate;

/// What is patched on one DMX address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChannelInfo {
    pub fixture: FixtureId,
    pub fixture_name: String,
    /// Position of the fixture in the universe, by address (0-based).
    pub fixture_index: u16,
    /// Offset of the channel inside the fixture (0 = first channel).
    pub offset: u16,
    pub channel_name: String,
    pub attribute: Attribute,
}

/// Summary of a patched fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FixtureInfo {
    pub id: FixtureId,
    /// Number of DMX channels (0 if the definition or mode is missing).
    pub footprint: u16,
    pub heads: u16,
    pub attributes: Vec<Attribute>,
    /// Has pan or tilt (moving head / scanner).
    pub has_position: bool,
    /// Has additive color mixing.
    pub has_color_mixing: bool,
}

pub fn fixture_info(show: &Show, f: &PatchedFixture) -> FixtureInfo {
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut heads = 1;
    let mut footprint = 0;
    if let Some((def, mode)) = show.definition_and_mode(f) {
        footprint = mode.footprint();
        heads = u16::try_from(mode.heads.len().max(1)).unwrap_or(u16::MAX);
        for &ci in &mode.channels {
            if let Some(ch) = def.channels.get(usize::from(ci)) {
                if !attributes.contains(&ch.attribute) {
                    attributes.push(ch.attribute);
                }
            }
        }
    }
    FixtureInfo {
        id: f.id,
        footprint,
        heads,
        has_position: attributes
            .iter()
            .any(|a| matches!(a, Attribute::Pan | Attribute::Tilt)),
        has_color_mixing: attributes.iter().any(|a| a.is_additive_color()),
        attributes,
    }
}

/// The 512 addresses of a universe (`None` = not patched).
pub fn channel_map(show: &Show, universe: UniverseId) -> Vec<Option<ChannelInfo>> {
    let mut map: Vec<Option<ChannelInfo>> = vec![None; UNIVERSE_SIZE];
    let mut fixtures: Vec<&PatchedFixture> = show
        .fixtures
        .iter()
        .filter(|f| f.universe == universe)
        .collect();
    fixtures.sort_by_key(|f| (f.address, f.id));
    for (index, f) in fixtures.into_iter().enumerate() {
        let Some((def, mode)) = show.definition_and_mode(f) else {
            continue;
        };
        for (offset, &ci) in mode.channels.iter().enumerate() {
            let slot = usize::from(f.address.saturating_sub(1)) + offset;
            let Some(cell) = map.get_mut(slot) else {
                continue;
            };
            if cell.is_some() {
                continue; // conflicting patch: the first fixture keeps the address
            }
            let ch = def.channels.get(usize::from(ci));
            *cell = Some(ChannelInfo {
                fixture: f.id,
                fixture_name: f.name.clone(),
                fixture_index: u16::try_from(index).unwrap_or(u16::MAX),
                offset: u16::try_from(offset).unwrap_or(u16::MAX),
                channel_name: ch.map_or_else(
                    || format!("Channel {}", offset + 1),
                    |c| c.name.trim().to_owned(),
                ),
                attribute: ch.map_or(Attribute::Other, |c| c.attribute),
            });
        }
    }
    map
}

/// A named preset of a channel: a DMX range with a meaning (a color wheel
/// slot, a gobo, a shutter mode, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChannelPreset {
    pub name: String,
    pub min: u8,
    pub max: u8,
    /// DMX value that selects the preset: the one scenes and effects use for
    /// it (the middle of the range).
    pub value: u8,
    pub kind: CapabilityKind,
}

/// The presets of the channel patched at `address` (1-based) of a universe,
/// in definition order. Empty for an unpatched address, a fine byte (its
/// coarse channel holds the presets) and channels without named ranges.
pub fn channel_presets(show: &Show, universe: UniverseId, address: u16) -> Vec<ChannelPreset> {
    // Same rule as `channel_map`: on a conflicting patch the first fixture
    // (by address, then id) keeps the address.
    let mut fixtures: Vec<&PatchedFixture> = show
        .fixtures
        .iter()
        .filter(|f| f.universe == universe)
        .collect();
    fixtures.sort_by_key(|f| (f.address, f.id));
    for f in fixtures {
        let Some((def, mode)) = show.definition_and_mode(f) else {
            continue;
        };
        // 0-based slots, computed like `channel_map` does.
        let Some(slot) = address.checked_sub(1) else {
            return Vec::new();
        };
        let Some(offset) = slot.checked_sub(f.address.saturating_sub(1)) else {
            continue;
        };
        let Some(&ci) = mode.channels.get(usize::from(offset)) else {
            continue;
        };
        let Some(ch) = def.channels.get(usize::from(ci)) else {
            return Vec::new();
        };
        if ch.byte == ChannelByte::Fine {
            return Vec::new();
        }
        return ch
            .capabilities
            .iter()
            .filter(|c| !c.name.trim().is_empty())
            .map(|c| ChannelPreset {
                name: c.name.trim().to_owned(),
                min: c.min.min(c.max),
                max: c.min.max(c.max),
                value: c.midpoint(),
                kind: c.kind.clone(),
            })
            .collect();
    }
    Vec::new()
}

/// A color a color-wheel fixture can show: one slot of its wheel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WheelColor {
    pub name: String,
    pub color: Color,
}

/// Which colors make sense for some fixtures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ColorChoices {
    /// Slots of the color wheels of the fixtures that have no color mixing
    /// (the colors a requested color snaps to), in wheel order, each color
    /// once.
    pub wheel: Vec<WheelColor>,
    /// Any color is meaningful: a fixture mixes colors, or none has a wheel.
    pub free: bool,
}

/// Attributes through which the engine mixes a requested color (before it
/// falls back to a color wheel).
fn mixes_color(a: Attribute) -> bool {
    matches!(
        a,
        Attribute::Red
            | Attribute::Green
            | Attribute::Blue
            | Attribute::Cyan
            | Attribute::Magenta
            | Attribute::Yellow
    )
}

/// Colors available on `fixtures`. Follows how the engine resolves a color
/// (a fixture with red/green/blue or cyan/magenta/yellow channels mixes it,
/// otherwise a color wheel snaps to the nearest single-color slot, and an
/// open slot counts as white).
///
/// Known limits: the engine decides per head, this per fixture (a fixture
/// with an RGB head and a wheel head counts as mixing, and only its first
/// wheel is listed); and it ignores a fixture's color calibration, which the
/// engine applies before snapping to a slot. Neither happens with the
/// fixtures and settings the UI offers today (calibration is only offered
/// for fixtures that mix colors).
pub fn color_choices(show: &Show, fixtures: &[FixtureId]) -> ColorChoices {
    let mut wheel: Vec<WheelColor> = Vec::new();
    let mut mixes = false;
    for id in fixtures {
        let Some(f) = show.fixture(*id) else { continue };
        let Some((def, mode)) = show.definition_and_mode(f) else {
            continue;
        };
        let channels: Vec<&crate::fixture_def::ChannelDef> = mode
            .channels
            .iter()
            .filter_map(|&ci| def.channels.get(usize::from(ci)))
            .collect();
        if channels.iter().any(|c| mixes_color(c.attribute)) {
            mixes = true;
            continue;
        }
        let Some(ch) = channels
            .iter()
            .find(|c| c.attribute == Attribute::ColorWheel)
        else {
            continue;
        };
        for cap in &ch.capabilities {
            let color = match cap.kind {
                CapabilityKind::Color {
                    color,
                    color2: None,
                } => Color::from_srgb8(color),
                CapabilityKind::Open => Color::WHITE,
                _ => continue,
            };
            if !wheel.iter().any(|w| w.color == color) {
                wheel.push(WheelColor {
                    name: cap.name.trim().to_owned(),
                    color,
                });
            }
        }
    }
    ColorChoices {
        free: mixes || wheel.is_empty(),
        wheel,
    }
}

/// Which kinds of scene values at least one of some fixtures can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ValueSupport {
    pub intensity: bool,
    pub color: bool,
    /// Pan or tilt, in either of the two ways below.
    pub position: bool,
    /// Pan and tilt in degrees (also fans).
    pub position_degrees: bool,
    /// Pan or tilt without a range in degrees: plain DMX values.
    pub position_dmx: bool,
    pub gobo: bool,
    pub prism: bool,
    pub zoom: bool,
    /// A strobe or shutter channel, in either of the two ways below.
    pub strobe: bool,
    /// A strobe in hertz.
    pub strobe_hz: bool,
    /// A shutter without frequencies: plain DMX values.
    pub strobe_dmx: bool,
}

/// What `fixtures` can do, following how the engine resolves each value: an
/// intensity needs a dimmer or color emitters (a virtual dimmer scales them),
/// a color needs mixing channels or a color wheel, a zoom a zoom channel with
/// a range in degrees, and the rest their own channel. A position is given in
/// degrees when the definition has the range of the fixture's pan and tilt, and
/// a strobe in hertz when its shutter channel tells frequencies; otherwise they
/// take plain DMX values. Same per-fixture limit as [`color_choices`].
pub fn value_support(show: &Show, fixtures: &[FixtureId]) -> ValueSupport {
    let mut s = ValueSupport::default();
    for id in fixtures {
        let Some(f) = show.fixture(*id) else { continue };
        let attributes = fixture_info(show, f).attributes;
        let has = |a: Attribute| attributes.contains(&a);
        let physical = show
            .definition_and_mode(f)
            .map(|(def, mode)| def.physical_for(mode))
            .unwrap_or_default();
        let zoom_span = physical.zoom_max_deg - physical.zoom_min_deg;
        s.intensity |= has(Attribute::Dimmer) || attributes.iter().any(|a| a.is_additive_color());
        s.color |= attributes.iter().any(|a| mixes_color(*a)) || has(Attribute::ColorWheel);
        let (pan, tilt) = (has(Attribute::Pan), has(Attribute::Tilt));
        if pan || tilt {
            let ranges =
                (!pan || physical.pan_max_deg > 0.0) && (!tilt || physical.tilt_max_deg > 0.0);
            s.position_degrees |= ranges;
            s.position_dmx |= !ranges;
        }
        s.gobo |= has(Attribute::Gobo);
        s.prism |= has(Attribute::Prism);
        s.zoom |= has(Attribute::Zoom) && zoom_span.is_finite() && zoom_span > 0.0;
        // The channel the engine writes a strobe on: the first shutter.
        let shutter = show.definition_and_mode(f).and_then(|(def, mode)| {
            mode.channels
                .iter()
                .filter_map(|&c| def.channels.get(usize::from(c)))
                .find(|c| c.attribute == Attribute::Shutter)
        });
        if let Some(shutter) = shutter {
            s.strobe_hz |= shutter.gives_strobe_hz();
            s.strobe_dmx |= !shutter.gives_strobe_hz();
        }
    }
    s.position = s.position_degrees || s.position_dmx;
    s.strobe = s.strobe_hz || s.strobe_dmx;
    s
}

/// First address (1-based) from `from` where `size` channels fit without
/// overlapping any fixture of the universe.
pub fn next_free_address(show: &Show, universe: UniverseId, size: u16, from: u16) -> Option<u16> {
    if size == 0 {
        return None;
    }
    let mut spans: Vec<(u16, u16)> = show
        .fixtures
        .iter()
        .filter(|f| f.universe == universe)
        .filter_map(|f| {
            let fp = validate::footprint(show, f)?;
            (fp > 0).then(|| (f.address, f.address.saturating_add(fp - 1)))
        })
        .collect();
    spans.sort_unstable();
    let mut candidate = from.max(1);
    for (start, end) in spans {
        if u32::from(candidate) + u32::from(size) - 1 < u32::from(start) {
            break;
        }
        if end >= candidate {
            candidate = end.saturating_add(1);
        }
    }
    (u32::from(candidate) + u32::from(size) - 1 <= UNIVERSE_SIZE as u32).then_some(candidate)
}

/// Fixtures that a span of `size` channels at `address` would overlap.
pub fn overlapping(
    show: &Show,
    universe: UniverseId,
    address: u16,
    size: u16,
    exclude: Option<FixtureId>,
) -> Vec<FixtureId> {
    let probe = PatchedFixture {
        id: exclude.unwrap_or(FixtureId(u32::MAX)),
        name: String::new(),
        definition: crate::ids::DefinitionKey(String::new()),
        mode: String::new(),
        universe,
        address,
        orientation: Default::default(),
        calibration: None,
    };
    validate::conflicts_for(show, &probe, size)
        .into_iter()
        .map(|c| c.first)
        .collect()
}

/// Where the value of a channel in a scene comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ChannelOrigin {
    /// The scene does not set it: the value is the default of the channel.
    Unset,
    /// Set by an intention or a palette.
    Value,
    /// Set by hand.
    Manual,
    /// The other byte of this 16-bit channel is set by hand: channels merge
    /// whole, so the scene drives this one too, at the channel default.
    Paired,
}

/// One channel of a fixture driven by a scene, as the engine computes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SceneChannel {
    /// How to refer to this channel to set it by hand.
    pub channel: ChannelRef,
    /// DMX address (1-based); `None` for a virtual dimmer (fixture without
    /// intensity channel: it scales the color channels).
    pub address: Option<u16>,
    pub channel_name: String,
    pub attribute: Attribute,
    /// Value at the end of the fade.
    pub value: u8,
    pub origin: ChannelOrigin,
    /// Service channels cannot be stored in scenes.
    pub locked: bool,
}

/// All the channels of one fixture driven by a scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SceneFixtureChannels {
    pub fixture: FixtureId,
    pub fixture_name: String,
    pub universe: UniverseId,
    pub address: u16,
    /// Virtual dimmers first, then by address.
    pub channels: Vec<SceneChannel>,
}

/// One group or fixture added to a scene, with its patched fixtures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SceneMember {
    pub target: Target,
    pub fixtures: Vec<FixtureId>,
}

/// The lights of a scene.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SceneLights {
    pub members: Vec<SceneMember>,
    /// Every fixture of the members once, in member order.
    pub fixtures: Vec<FixtureId>,
}

/// The color and intensity a scene gives one of its fixtures, as intentions:
/// from its values in order, the last one that reaches the fixture wins
/// (directly, through a group, or to all the lights of the scene). None where
/// no value sets it (or only channels set by hand do).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LightLook {
    pub fixture: FixtureId,
    pub color: Option<Color>,
    /// 0..=1.
    pub intensity: Option<f32>,
}

/// The times of one chaser step, its own or the chaser's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChaserStepTime {
    /// From its start to the start of the next step.
    pub hold: TimeValue,
    /// Crossfade into it, at its start (within its hold).
    pub fade: TimeValue,
    /// The step has its own hold / fade (else the chaser's).
    pub own_hold: bool,
    pub own_fade: bool,
    /// The same at a tempo, in milliseconds.
    pub hold_ms: f64,
    pub fade_ms: f64,
}

/// The times of a chaser at a tempo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChaserTimes {
    /// The chaser's own hold and fade at that tempo, in milliseconds.
    pub hold_ms: f64,
    pub fade_ms: f64,
    pub steps: Vec<ChaserStepTime>,
    /// Once through every step, in milliseconds.
    pub pass_ms: f64,
}

/// A value of a scene that later values override, on some or all of the
/// channels it writes (values apply in order: the last one wins on every
/// channel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CoveredValue {
    /// Index of the value in the scene.
    pub index: u32,
    /// Indices of the later values that override it, in order.
    pub by: Vec<u32>,
    /// Every channel it writes is overridden: it has no effect.
    pub full: bool,
}

/// The fixtures a target names, each once. None for [`Target::Scene`], which
/// only means something inside its scene.
pub fn target_fixtures(show: &Show, target: &Target) -> Vec<FixtureId> {
    match *target {
        Target::Fixture { id } | Target::Head { id, .. } => vec![id],
        Target::Group { id } => {
            let mut out = Vec::new();
            for m in show.group(id).iter().flat_map(|g| &g.members) {
                if !out.contains(&m.fixture) {
                    out.push(m.fixture);
                }
            }
            out
        }
        Target::Scene => Vec::new(),
    }
}

pub fn scene_lights(show: &Show, scene: &Scene) -> SceneLights {
    let mut out = SceneLights::default();
    for &target in &scene.members {
        let mut fixtures = target_fixtures(show, &target);
        fixtures.retain(|&f| show.fixture(f).is_some());
        for &f in &fixtures {
            if !out.fixtures.contains(&f) {
                out.fixtures.push(f);
            }
        }
        out.members.push(SceneMember { target, fixtures });
    }
    out
}

/// The color a function shows (for a button or a swatch): the first color
/// its scene sets, explicitly or through a color palette, with the palette
/// values of the first fixture the value targets (as the engine resolves
/// them). `None` for functions that are not scenes or set no color.
pub fn function_color(show: &Show, id: FunctionId) -> Option<Color> {
    let FunctionKind::Scene(scene) = &show.function(id)?.kind else {
        return None;
    };
    scene
        .values
        .iter()
        .find_map(|sv| scene_value_color(show, &scene.members, sv))
}

/// The color one value of a scene with these `members` sets, explicitly or
/// through a color palette (values of the first fixture it targets), or `None`.
pub fn scene_value_color(show: &Show, members: &[Target], sv: &SceneValue) -> Option<Color> {
    match &sv.value {
        ValueSource::Explicit {
            value: AttributeValue::Color { color },
        } => Some(*color),
        ValueSource::Explicit { .. } | ValueSource::Channel { .. } => None,
        ValueSource::Palette { id } => {
            let palette = show.palette(*id)?;
            let first = match sv.target {
                Target::Scene => members.iter().find_map(|m| first_fixture(show, m)),
                ref target => first_fixture(show, target),
            };
            let values = match first {
                Some(f) => palette.values_for(f),
                None => palette.generic.as_slice(),
            };
            values.iter().find_map(|v| match v {
                AttributeValue::Color { color } => Some(*color),
                _ => None,
            })
        }
    }
}

/// The times of every step of a chaser at `bpm`, as the engine plays them
/// (a step's own hold and fade, else the chaser's).
pub fn chaser_times(chaser: &Chaser, bpm: f64) -> ChaserTimes {
    let ms = |t: TimeValue| t.to_micros(bpm) as f64 / 1000.0;
    let steps: Vec<ChaserStepTime> = chaser
        .steps
        .iter()
        .map(|s| {
            let hold = s.hold.unwrap_or(chaser.hold);
            let fade = s.fade.unwrap_or(chaser.fade);
            ChaserStepTime {
                hold,
                fade,
                own_hold: s.hold.is_some(),
                own_fade: s.fade.is_some(),
                hold_ms: ms(hold),
                fade_ms: ms(fade),
            }
        })
        .collect();
    let pass_ms = steps.iter().map(|s| s.hold_ms).sum();
    ChaserTimes {
        hold_ms: ms(chaser.hold),
        fade_ms: ms(chaser.fade),
        steps,
        pass_ms,
    }
}

/// The look of every fixture of a scene, in the order of its lights.
pub fn scene_light_looks(show: &Show, scene: &Scene) -> Vec<LightLook> {
    let lights = scene_lights(show, scene);
    let mut looks: Vec<LightLook> = lights
        .fixtures
        .iter()
        .map(|&fixture| LightLook {
            fixture,
            color: None,
            intensity: None,
        })
        .collect();
    for sv in &scene.values {
        let reached = match sv.target {
            Target::Scene => lights.fixtures.clone(),
            ref target => target_fixtures(show, target),
        };
        for look in looks.iter_mut().filter(|l| reached.contains(&l.fixture)) {
            let values: &[AttributeValue] = match &sv.value {
                ValueSource::Explicit { value } => std::slice::from_ref(value),
                ValueSource::Palette { id } => match show.palette(*id) {
                    Some(p) => p.values_for(look.fixture),
                    None => &[],
                },
                ValueSource::Channel { .. } => &[],
            };
            for v in values {
                match v {
                    AttributeValue::Color { color } => look.color = Some(*color),
                    AttributeValue::Intensity { level } => look.intensity = Some(*level),
                    _ => {}
                }
            }
        }
    }
    looks
}

fn first_fixture(show: &Show, target: &Target) -> Option<FixtureId> {
    match target {
        Target::Fixture { id } | Target::Head { id, .. } => Some(*id),
        Target::Group { id } => show.group(*id)?.members.first().map(|m| m.fixture),
        Target::Scene => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture_def::{
        ChannelByte, ChannelDef, FixtureDefinition, FixtureKind, ModeDef, Physical,
    };
    use crate::patch::{Universe, UNIVERSE_SIZE};

    fn show(addresses: &[u16]) -> Show {
        let mut s = Show::new("t");
        s.universes.push(Universe {
            id: UniverseId(1),
            name: "U".into(),
            output: None,
        });
        let ch = |name: &str, attribute| ChannelDef {
            name: name.into(),
            attribute,
            byte: ChannelByte::Coarse,
            default: 0,
            capabilities: vec![],
        };
        s.definitions.push(FixtureDefinition {
            manufacturer: "G".into(),
            model: "P".into(),
            kind: FixtureKind::Other,
            channels: vec![
                ch("Pan", Attribute::Pan),
                ch("Red ", Attribute::Red),
                ch("D", Attribute::Dimmer),
            ],
            modes: vec![ModeDef {
                name: "m".into(),
                channels: vec![0, 1, 2, 2, 2, 2],
                heads: vec![],
                physical: None,
            }],
            physical: Physical::default(),
        });
        for (i, a) in addresses.iter().enumerate() {
            s.fixtures.push(PatchedFixture {
                id: FixtureId(i as u32 + 1),
                name: format!("f{i}"),
                definition: crate::ids::DefinitionKey::new("G", "P"),
                mode: "m".into(),
                universe: UniverseId(1),
                address: *a,
                orientation: Default::default(),
                calibration: None,
            });
        }
        s
    }

    #[test]
    fn next_free_address_cases() {
        let u = UniverseId(1);
        assert_eq!(next_free_address(&show(&[]), u, 6, 1), Some(1));
        assert_eq!(next_free_address(&show(&[1]), u, 6, 1), Some(7));
        assert_eq!(next_free_address(&show(&[1, 13]), u, 6, 1), Some(7));
        assert_eq!(next_free_address(&show(&[1, 10]), u, 6, 1), Some(16));
        assert_eq!(next_free_address(&show(&[]), u, 6, 508), None);
        assert_eq!(next_free_address(&show(&[]), u, 0, 1), None);
    }

    #[test]
    fn channel_presets_of_a_wheel() {
        use crate::fixture_def::{Capability, CapabilityKind};
        use crate::Rgb8;
        let mut s = show(&[1, 20]);
        let cap = |min, max, name: &str, kind| Capability {
            min,
            max,
            name: name.into(),
            kind,
        };
        let red = Rgb8::new(255, 0, 0);
        // Channel "Red " (offset 1) becomes a wheel: open, red, an unnamed range.
        s.definitions[0].channels[1].capabilities = vec![
            cap(0, 9, "Open", CapabilityKind::Open),
            cap(
                20,
                10,
                " Red ",
                CapabilityKind::Color {
                    color: red,
                    color2: None,
                },
            ),
            cap(30, 39, "", CapabilityKind::Generic),
        ];
        let u = UniverseId(1);
        let presets = channel_presets(&s, u, 2);
        assert_eq!(
            presets
                .iter()
                .map(|p| (p.name.as_str(), p.min, p.max, p.value))
                .collect::<Vec<_>>(),
            vec![("Open", 0, 9, 4), ("Red", 10, 20, 15)],
        );
        assert_eq!(
            presets[1].kind,
            CapabilityKind::Color {
                color: red,
                color2: None
            }
        );
        // The second fixture's same channel has them too; others do not.
        assert_eq!(channel_presets(&s, u, 21).len(), 2);
        assert!(channel_presets(&s, u, 1).is_empty()); // Pan: no named ranges
        assert!(channel_presets(&s, u, 100).is_empty()); // not patched
        assert!(channel_presets(&s, u, 0).is_empty());
        assert!(channel_presets(&s, UniverseId(2), 2).is_empty());
        // Regression: a fixture patched at address 0 (a bad import) maps to
        // channels the same way as in channel_map.
        s.fixtures[0].address = 0;
        assert_eq!(
            channel_map(&s, u)[1].as_ref().map(|c| c.offset),
            Some(1),
            "channel_map: DMX 2 is the fixture's 2nd channel"
        );
        assert_eq!(channel_presets(&s, u, 2).len(), 2);
        assert!(channel_presets(&s, u, 3).is_empty());
        s.fixtures[0].address = 1;
        // A fine byte has none: its coarse channel holds the presets.
        s.definitions[0].channels[1].byte = crate::fixture_def::ChannelByte::Fine;
        assert!(channel_presets(&s, u, 2).is_empty());
    }

    #[test]
    fn color_choices_follow_how_colors_resolve() {
        use crate::fixture_def::{Capability, ChannelDef, FixtureDefinition, FixtureKind, ModeDef};
        use crate::Rgb8;
        let cap = |min, max, name: &str, kind| Capability {
            min,
            max,
            name: name.into(),
            kind,
        };
        let solid = |r, g, b| CapabilityKind::Color {
            color: Rgb8::new(r, g, b),
            color2: None,
        };
        let wheel = |first: &str| ChannelDef {
            name: "Wheel".into(),
            attribute: Attribute::ColorWheel,
            byte: ChannelByte::Coarse,
            default: 0,
            capabilities: vec![
                cap(0, 9, "Open", CapabilityKind::Open),
                cap(10, 19, first, solid(255, 0, 0)),
                cap(
                    20,
                    29,
                    "Split",
                    CapabilityKind::Color {
                        color: Rgb8::new(255, 0, 0),
                        color2: Some(Rgb8::new(0, 0, 255)),
                    },
                ),
                cap(30, 39, "Blue", solid(0, 0, 255)),
                cap(40, 49, "Rotation", CapabilityKind::Generic),
            ],
        };
        // Fixtures 1 and 2 ("G/P") mix colors; fixture 3 gets a wheel-only
        // definition.
        let mut s = show(&[1, 20, 40]);
        s.definitions.push(FixtureDefinition {
            manufacturer: "G".into(),
            model: "W".into(),
            kind: FixtureKind::Other,
            channels: vec![wheel("Ruby")],
            modes: vec![ModeDef {
                name: "m".into(),
                channels: vec![0],
                heads: vec![],
                physical: None,
            }],
            physical: Default::default(),
        });
        s.fixtures[2].definition = crate::ids::DefinitionKey::new("G", "W");
        fn names(c: &ColorChoices) -> Vec<&str> {
            c.wheel.iter().map(|w| w.name.as_str()).collect()
        }

        // A wheel-only fixture: its single-color slots (open counts as white,
        // a split slot and a non-color range do not), and no free picker.
        let only = color_choices(&s, &[FixtureId(3)]);
        assert_eq!(names(&only), ["Open", "Ruby", "Blue"]);
        assert!(!only.free);
        assert_eq!(only.wheel[0].color, Color::WHITE);
        // A fixture that mixes colors can show any.
        let mixing = color_choices(&s, &[FixtureId(1)]);
        assert!(mixing.free && mixing.wheel.is_empty());
        // Mixed selection: any color, and the wheel colors still offered.
        let both = color_choices(&s, &[FixtureId(1), FixtureId(3)]);
        assert!(both.free);
        assert_eq!(names(&both), ["Open", "Ruby", "Blue"]);
        // Nothing selected, or no color at all: free.
        assert!(color_choices(&s, &[]).free);
        assert!(color_choices(&s, &[FixtureId(99)]).free);
    }

    #[test]
    fn value_support_follows_the_channels() {
        use crate::fixture_def::{ChannelDef, FixtureDefinition, FixtureKind, ModeDef};
        let ch = |attribute| ChannelDef {
            name: format!("{attribute:?}"),
            attribute,
            byte: ChannelByte::Coarse,
            default: 0,
            capabilities: vec![],
        };
        let def = |model: &str, attributes: Vec<Attribute>| FixtureDefinition {
            manufacturer: "G".into(),
            model: model.into(),
            kind: FixtureKind::Other,
            modes: vec![ModeDef {
                name: "m".into(),
                channels: (0..attributes.len() as u16).collect(),
                heads: vec![],
                physical: None,
            }],
            channels: attributes.into_iter().map(ch).collect(),
            physical: Default::default(),
        };
        // 1: pan + red + dimmer ("G/P"); 2: wheel only; 3: white only (no
        // dimmer, no R/G/B: nothing to scale); 4: beam effects.
        let mut s = show(&[1, 20, 40, 60]);
        s.definitions[0].physical.pan_max_deg = 540.0;
        s.definitions.push(def("W", vec![Attribute::ColorWheel]));
        s.definitions.push(def("L", vec![Attribute::White]));
        s.definitions.push(def(
            "B",
            vec![
                Attribute::Gobo,
                Attribute::Prism,
                Attribute::Zoom,
                Attribute::Shutter,
            ],
        ));
        if let Some(b) = s.definitions.last_mut() {
            b.physical.zoom_min_deg = 10.0;
            b.physical.zoom_max_deg = 40.0;
        }
        for (i, model) in [(1, "W"), (2, "L"), (3, "B")] {
            s.fixtures[i].definition = crate::ids::DefinitionKey::new("G", model);
        }
        let ids = |v: &[u32]| v.iter().map(|&n| FixtureId(n)).collect::<Vec<_>>();
        // Degrees and hertz only where the definition gives them: the shutter
        // of fixture 4 tells no frequencies.
        let some = |intensity, color, position, gobo, prism, zoom, strobe| ValueSupport {
            intensity,
            color,
            position,
            position_degrees: position,
            position_dmx: false,
            gobo,
            prism,
            zoom,
            strobe,
            strobe_hz: false,
            strobe_dmx: strobe,
        };
        assert_eq!(
            value_support(&s, &ids(&[1])),
            some(true, true, true, false, false, false, false)
        );
        // A color wheel shows colors but has no intensity of its own.
        assert_eq!(
            value_support(&s, &ids(&[2])),
            some(false, true, false, false, false, false, false)
        );
        // White is an additive emitter: a virtual dimmer scales it, but with
        // no red/green/blue (or wheel) it cannot show a requested color.
        assert_eq!(
            value_support(&s, &ids(&[3])),
            some(true, false, false, false, false, false, false)
        );
        assert_eq!(
            value_support(&s, &ids(&[4])),
            some(false, false, false, true, true, true, true)
        );
        // A selection supports what any of its fixtures does; nothing, nothing.
        assert_eq!(
            value_support(&s, &ids(&[2, 4])),
            some(false, true, false, true, true, true, true)
        );
        assert_eq!(value_support(&s, &[]), ValueSupport::default());
        assert_eq!(value_support(&s, &ids(&[99])), ValueSupport::default());

        // A strobe in hertz needs the frequencies of a range.
        if let Some(shutter) = s.definitions.last_mut().and_then(|d| d.channels.last_mut()) {
            shutter.capabilities.push(crate::fixture_def::Capability {
                min: 10,
                max: 255,
                name: "Strobe".into(),
                kind: CapabilityKind::Strobe {
                    hz_min: Some(1.0),
                    hz_max: Some(20.0),
                },
            });
        }
        let hz = value_support(&s, &ids(&[4]));
        assert!(hz.strobe_hz && !hz.strobe_dmx && hz.strobe);

        // Without a range in degrees the engine ignores a position or a zoom:
        // the position is offered as plain DMX values, the zoom not at all.
        s.definitions[0].physical.pan_max_deg = 0.0;
        let raw = value_support(&s, &ids(&[1]));
        assert!(raw.position_dmx && !raw.position_degrees && raw.position);
        if let Some(b) = s.definitions.last_mut() {
            b.physical.zoom_max_deg = b.physical.zoom_min_deg;
        }
        assert!(!value_support(&s, &ids(&[4])).zoom);
    }

    #[test]
    fn overlaps_and_channel_map() {
        let s = show(&[1, 20]);
        let u = UniverseId(1);
        assert_eq!(overlapping(&s, u, 5, 6, None), vec![FixtureId(1)]);
        assert!(overlapping(&s, u, 7, 6, None).is_empty());
        assert!(overlapping(&s, u, 1, 6, Some(FixtureId(1))).is_empty());
        let map = channel_map(&s, u);
        assert_eq!(map.len(), UNIVERSE_SIZE);
        let c = map[1].as_ref().unwrap();
        assert_eq!(
            (
                c.fixture,
                c.offset,
                c.channel_name.as_str(),
                c.fixture_index
            ),
            (FixtureId(1), 1, "Red", 0)
        );
        assert_eq!(map[19].as_ref().unwrap().fixture_index, 1);
        assert!(map[6].is_none());
        let info = fixture_info(&s, &s.fixtures[0]);
        assert!(info.has_position && info.has_color_mixing);
        assert_eq!(info.footprint, 6);
    }

    #[test]
    fn chaser_times_take_each_step_own_times_or_the_chaser_ones() {
        use crate::function::{ChaserStep, Direction, RunMode};
        let step = |hold, fade| ChaserStep {
            scene: FunctionId(1),
            hold,
            fade,
        };
        let chaser = Chaser {
            steps: vec![
                step(None, None),
                step(Some(TimeValue::Beats(2.0)), None),
                step(None, Some(TimeValue::Ms(100))),
            ],
            hold: TimeValue::Beats(1.0),
            fade: TimeValue::ZERO,
            direction: Direction::Forward,
            run_mode: RunMode::Loop,
        };
        let t = chaser_times(&chaser, 120.0);
        let summary: Vec<(f64, f64, bool, bool)> = t
            .steps
            .iter()
            .map(|s| (s.hold_ms, s.fade_ms, s.own_hold, s.own_fade))
            .collect();
        assert_eq!(
            summary,
            vec![
                (500.0, 0.0, false, false),
                (1000.0, 0.0, true, false),
                (500.0, 100.0, false, true),
            ]
        );
        assert_eq!(t.steps.get(1).map(|s| s.hold), Some(TimeValue::Beats(2.0)));
        assert_eq!(t.pass_ms, 2000.0);
        assert_eq!((t.hold_ms, t.fade_ms), (500.0, 0.0));
        assert_eq!(
            chaser_times(&chaser, 60.0).pass_ms,
            4000.0,
            "beats follow the tempo"
        );
    }

    #[test]
    fn light_looks_follow_the_values_in_order_whoever_they_go_to() {
        use crate::ids::{GroupId, PaletteId};
        use crate::palette::{FixturePaletteValues, Palette, PaletteKind};
        use crate::patch::{FixtureGroup, GroupMember};
        let mut s = show(&[1, 20]);
        let red = Color::new(1.0, 0.0, 0.0);
        let blue = Color::new(0.0, 0.0, 1.0);
        let green = Color::new(0.0, 1.0, 0.0);
        s.groups.push(FixtureGroup {
            id: GroupId(1),
            name: "both".into(),
            members: [1, 2]
                .map(|f| GroupMember {
                    fixture: FixtureId(f),
                    head: 0,
                    grid: None,
                })
                .to_vec(),
            grid_width: 0,
            grid_height: 0,
        });
        s.palettes.push(Palette {
            id: PaletteId(7),
            name: "p".into(),
            kind: PaletteKind::Color,
            generic: vec![AttributeValue::Color { color: red }],
            per_fixture: vec![FixturePaletteValues {
                fixture: FixtureId(2),
                values: vec![AttributeValue::Color { color: green }],
            }],
        });
        let at = |id| Target::Fixture { id: FixtureId(id) };
        let explicit = |target, value| SceneValue {
            target,
            value: ValueSource::Explicit { value },
        };
        let scene = Scene {
            members: vec![Target::Group { id: GroupId(1) }],
            values: vec![
                explicit(at(1), AttributeValue::Color { color: blue }),
                // The whole scene after: it wins on fixture 1 too.
                explicit(Target::Scene, AttributeValue::Intensity { level: 0.5 }),
                // A palette: each fixture its own color.
                SceneValue {
                    target: Target::Group { id: GroupId(1) },
                    value: ValueSource::Palette { id: PaletteId(7) },
                },
                // Then one fixture again: the last word on it.
                explicit(at(2), AttributeValue::Intensity { level: 1.0 }),
            ],
            ..Scene::default()
        };
        assert_eq!(
            scene_light_looks(&s, &scene),
            vec![
                LightLook {
                    fixture: FixtureId(1),
                    color: Some(red),
                    intensity: Some(0.5)
                },
                LightLook {
                    fixture: FixtureId(2),
                    color: Some(green),
                    intensity: Some(1.0)
                },
            ]
        );
    }

    #[test]
    fn light_looks_are_unset_without_values_and_ignore_channels_by_hand() {
        let s = show(&[1]);
        let scene = Scene {
            members: vec![Target::Fixture { id: FixtureId(1) }],
            values: vec![SceneValue {
                target: Target::Fixture { id: FixtureId(1) },
                value: ValueSource::Channel {
                    channel: ChannelRef::Dmx {
                        attribute: Attribute::Dimmer,
                        index: 0,
                        fine: false,
                    },
                    value: 255,
                },
            }],
            ..Scene::default()
        };
        assert_eq!(
            scene_light_looks(&s, &scene),
            vec![LightLook {
                fixture: FixtureId(1),
                color: None,
                intensity: None
            }]
        );
    }

    #[test]
    fn function_color_explicit_palette_and_per_fixture() {
        use crate::function::{Function, Scene};
        use crate::ids::PaletteId;
        use crate::palette::{FixturePaletteValues, Palette, PaletteKind};
        let mut s = show(&[1, 20]);
        let red = Color::new(1.0, 0.0, 0.0);
        let blue = Color::new(0.0, 0.0, 1.0);
        s.palettes.push(Palette {
            id: PaletteId(1),
            name: "p".into(),
            kind: PaletteKind::Color,
            generic: vec![AttributeValue::Color { color: red }],
            per_fixture: vec![FixturePaletteValues {
                fixture: FixtureId(2),
                values: vec![AttributeValue::Color { color: blue }],
            }],
        });
        let scene = |id: u32, values: Vec<SceneValue>| Function {
            id: FunctionId(id),
            name: format!("s{id}"),
            kind: FunctionKind::Scene(Scene {
                values,
                ..Scene::default()
            }),
            folder: None,
        };
        let at = |id| Target::Fixture { id: FixtureId(id) };
        s.functions = vec![
            scene(
                1,
                vec![
                    SceneValue {
                        target: at(1),
                        value: ValueSource::Explicit {
                            value: AttributeValue::Intensity { level: 1.0 },
                        },
                    },
                    SceneValue {
                        target: at(1),
                        value: ValueSource::Explicit {
                            value: AttributeValue::Color { color: blue },
                        },
                    },
                ],
            ),
            scene(
                2,
                vec![SceneValue {
                    target: at(1),
                    value: ValueSource::Palette { id: PaletteId(1) },
                }],
            ),
            scene(
                3,
                vec![SceneValue {
                    target: at(2),
                    value: ValueSource::Palette { id: PaletteId(1) },
                }],
            ),
            scene(4, vec![]),
        ];
        assert_eq!(function_color(&s, FunctionId(1)), Some(blue));
        assert_eq!(function_color(&s, FunctionId(2)), Some(red));
        // Fixture 2 has its own palette value.
        assert_eq!(function_color(&s, FunctionId(3)), Some(blue));
        assert_eq!(function_color(&s, FunctionId(4)), None);
        assert_eq!(function_color(&s, FunctionId(9)), None);
    }
}
