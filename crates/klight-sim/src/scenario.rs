//! Reference scenarios used by tests, golden files and soak runs.
//! Test-harness code: failures here are bugs in the scenario itself.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use klight_engine_core::{Engine, EngineConfig};
use klight_model::*;

pub const RGB_PAR: &str = include_str!("../../../fixtures/Generic-RGB-Par.qxf");
pub const RGBW_PAR: &str = include_str!("../../../fixtures/Generic-RGBW-Dimmer-Par.qxf");
pub const DIMMER: &str = include_str!("../../../fixtures/Generic-Dimmer.qxf");
pub const SPOT: &str = include_str!("../../../fixtures/Test-Spot-Moving-Head.qxf");
pub const PIXEL_BAR: &str = include_str!("../../../fixtures/Test-Pixel-Bar-4.qxf");

pub const U1: UniverseId = UniverseId(1);
pub const DIM1: FixtureId = FixtureId(1);
pub const DIM2: FixtureId = FixtureId(2);
pub const RGB: FixtureId = FixtureId(3);
pub const RGBW: FixtureId = FixtureId(4);
pub const SPOT1: FixtureId = FixtureId(5);
pub const SPOT2: FixtureId = FixtureId(6);
pub const BAR: FixtureId = FixtureId(7);
pub const G_DIMMERS: GroupId = GroupId(1);
pub const G_SPOTS: GroupId = GroupId(2);
pub const G_PIXELS: GroupId = GroupId(3);

pub fn def(text: &str) -> FixtureDefinition {
    klight_fixtures::parse_qxf(text).unwrap()
}

fn patch(
    id: FixtureId,
    name: &str,
    d: &FixtureDefinition,
    mode: &str,
    address: u16,
) -> PatchedFixture {
    PatchedFixture {
        id,
        name: name.into(),
        definition: d.key(),
        mode: mode.into(),
        universe: U1,
        address,
        orientation: Orientation::default(),
        calibration: None,
    }
}

/// A small but representative show.
pub fn test_show() -> Show {
    let mut s = Show::new("test");
    s.universes.push(Universe {
        id: U1,
        name: "U1".into(),
        output: None,
    });
    let (rgb, rgbw, dim, spot, bar) = (
        def(RGB_PAR),
        def(RGBW_PAR),
        def(DIMMER),
        def(SPOT),
        def(PIXEL_BAR),
    );
    s.fixtures.push(patch(DIM1, "Dim 1", &dim, "1 Channel", 1));
    s.fixtures.push(patch(DIM2, "Dim 2", &dim, "1 Channel", 2));
    s.fixtures.push(patch(RGB, "RGB", &rgb, "3 Channel", 10));
    s.fixtures.push(patch(RGBW, "RGBW", &rgbw, "6 Channel", 20));
    s.fixtures
        .push(patch(SPOT1, "Spot 1", &spot, "Extended", 40));
    let mut s2 = patch(SPOT2, "Spot 2", &spot, "Extended", 60);
    s2.orientation.invert_pan = true;
    s.fixtures.push(s2);
    s.fixtures.push(patch(BAR, "Bar", &bar, "12 Channel", 100));
    s.definitions = vec![rgb, rgbw, dim, spot, bar];
    let member = |f, head| GroupMember {
        fixture: f,
        head,
        grid: None,
    };
    s.groups.push(FixtureGroup {
        id: G_DIMMERS,
        name: "Dimmers".into(),
        members: vec![member(DIM1, 0), member(DIM2, 0)],
        grid_width: 0,
        grid_height: 0,
    });
    s.groups.push(FixtureGroup {
        id: G_SPOTS,
        name: "Spots".into(),
        members: vec![member(SPOT1, 0), member(SPOT2, 0)],
        grid_width: 0,
        grid_height: 0,
    });
    s.groups.push(FixtureGroup {
        id: G_PIXELS,
        name: "Pixels".into(),
        members: (0..4)
            .map(|h| GroupMember {
                fixture: BAR,
                head: h,
                grid: Some(patch::GridPos { x: h, y: 0 }),
            })
            .collect(),
        grid_width: 4,
        grid_height: 1,
    });
    s
}

pub fn engine(show: Show) -> Engine {
    Engine::new(show, EngineConfig::default())
}

pub fn ui(id: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        id,
        source: CommandSource::LocalUi,
        command,
    }
}

pub fn ok(e: &mut Engine, now: u64, c: Command) -> CommandOutcome {
    let r = e.apply(now, ui(0, c.clone()));
    match r.result {
        Ok(o) => o,
        Err(err) => panic!("command {c:?} failed: {err}"),
    }
}

pub fn err(e: &mut Engine, now: u64, c: Command) -> CommandError {
    e.apply(now, ui(0, c)).result.unwrap_err()
}

pub fn intensity(level: f32) -> AttributeValue {
    AttributeValue::Intensity { level }
}

pub fn explicit(target: Target, value: AttributeValue) -> SceneValue {
    SceneValue {
        target,
        value: ValueSource::Explicit { value },
    }
}

pub fn fixture(id: FixtureId) -> Target {
    Target::Fixture { id }
}

pub fn group(id: GroupId) -> Target {
    Target::Group { id }
}

pub fn scene(id: u32, values: Vec<SceneValue>, fade_ms: u32) -> Function {
    Function {
        id: FunctionId(id),
        folder: None,
        name: format!("scene {id}"),
        kind: FunctionKind::Scene(Scene {
            values,
            timing: Timing {
                delay: TimeValue::ZERO,
                fade_in: TimeValue::Ms(fade_ms),
                fade_out: TimeValue::Ms(fade_ms),
            },
            ..Scene::default()
        }),
    }
}

/// DMX value at a 1-based address of universe 1.
pub fn dmx(e: &Engine, address: usize) -> u8 {
    e.frame(U1).unwrap()[address - 1]
}
