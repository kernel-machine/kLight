//! The lights of a scene: the groups and fixtures added to it, values given to
//! all of them, and the edits that add and remove them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use common::*;
use klight_engine_core::compile::CompiledShow;
use klight_engine_core::preview::scene_channels;
use klight_engine_core::scene_edit::{add_scene_members, add_scene_values, remove_scene_member};
use klight_model::inspect::{scene_lights, ChannelOrigin};
use klight_model::*;

const WHOLE: Target = Target::Scene;

fn full(target: Target) -> SceneValue {
    explicit(target, intensity(1.0))
}

fn scene_of(members: Vec<Target>, values: Vec<SceneValue>) -> Scene {
    Scene {
        members,
        values,
        ..Scene::default()
    }
}

fn function(scene: Scene) -> Function {
    Function {
        id: FunctionId(1),
        folder: None,
        name: "s".into(),
        kind: FunctionKind::Scene(scene),
    }
}

/// Runs the scene and returns the engine.
fn run(show: Show, scene: Scene) -> klight_engine_core::Engine {
    let mut e = engine(show);
    ok(
        &mut e,
        0,
        Command::SetFunction {
            function: function(scene),
        },
    );
    ok(&mut e, 0, Command::StartFunction { id: FunctionId(1) });
    e.tick(1_000);
    e
}

fn stored(e: &klight_engine_core::Engine) -> &Scene {
    match &e.show().function(FunctionId(1)).unwrap().kind {
        FunctionKind::Scene(s) => s,
        other => panic!("not a scene: {other:?}"),
    }
}

#[test]
fn new_scenes_give_whole_scene_values_to_lights_added_later() {
    assert!(Scene::default().new_members_follow);
    let old: Scene = serde_json::from_str(r#"{"values":[]}"#).unwrap();
    assert!(old.new_members_follow && old.members.is_empty());
}

#[test]
fn a_whole_scene_value_drives_every_light_of_the_scene() {
    let scene = scene_of(vec![group(G_DIMMERS), fixture(RGB)], vec![full(WHOLE)]);
    let e = run(test_show(), scene);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 255), "the group");
    assert_eq!(dmx(&e, 20), 0, "a fixture that is not in the scene");
}

#[test]
fn a_group_in_a_scene_stays_linked_to_the_group() {
    let mut show = test_show();
    show.groups[0].members.truncate(1);
    let mut e = run(show, scene_of(vec![group(G_DIMMERS)], vec![full(WHOLE)]));
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 0));
    ok(
        &mut e,
        2_000,
        Command::SetGroup {
            group: test_show().groups[0].clone(),
        },
    );
    e.tick(3_000);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 255));
}

#[test]
fn a_whole_scene_fan_spreads_over_the_lights_counted_once() {
    let fan = AttributeValue::PositionFan {
        pan: 0.0,
        tilt: 0.0,
        pan_spread: 90.0,
        tilt_spread: 0.0,
        mode: FanMode::Linear,
    };
    let pan = |members: Vec<Target>| {
        let show = test_show();
        let compiled = CompiledShow::build(&show);
        let values = [explicit(WHOLE, fan.clone())];
        compiled.resolve_values(&show, &values, &members).0
    };
    // The fixture listed again beside its group does not widen the fan.
    assert_eq!(
        pan(vec![group(G_SPOTS), fixture(SPOT1)]),
        pan(vec![group(G_SPOTS)])
    );
}

#[test]
fn adding_a_light_gives_it_the_whole_scene_values() {
    let show = test_show();
    let scene = scene_of(vec![fixture(DIM1)], vec![full(WHOLE)]);
    let after = add_scene_members(&show, &scene, &[fixture(DIM2)]);
    assert_eq!(after.members, [fixture(DIM1), fixture(DIM2)]);
    assert_eq!(after.values, scene.values);
    let e = run(show, after);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 255));
}

#[test]
fn unless_the_scene_says_new_lights_stay_unset() {
    let show = test_show();
    let mut scene = scene_of(
        vec![fixture(DIM1), group(G_SPOTS)],
        vec![full(WHOLE), full(fixture(DIM1))],
    );
    scene.new_members_follow = false;
    let after = add_scene_members(&show, &scene, &[fixture(DIM2)]);
    assert_eq!(
        after.members,
        [fixture(DIM1), group(G_SPOTS), fixture(DIM2)]
    );
    // The value of the whole scene stays with the lights that had it, in place.
    assert_eq!(after.values, [full(group(G_SPOTS)), full(fixture(DIM1))]);
    let e = run(show.clone(), after);
    assert_eq!((dmx(&e, 1), dmx(&e, 2)), (255, 0));
    // Adding nothing new changes nothing.
    assert_eq!(add_scene_members(&show, &scene, &[fixture(DIM1)]), scene);
}

#[test]
fn lights_already_in_the_scene_are_not_added_again() {
    let show = test_show();
    let scene = scene_of(vec![group(G_DIMMERS)], Vec::new());
    let head = Target::Head { id: RGB, head: 0 };
    let after = add_scene_members(
        &show,
        &scene,
        &[fixture(DIM1), group(G_DIMMERS), head, fixture(RGB), WHOLE],
    );
    assert_eq!(after.members, [group(G_DIMMERS), fixture(RGB)]);
    // A group is added before its fixtures are looked at.
    let both = add_scene_members(&show, &Scene::default(), &[fixture(DIM1), group(G_DIMMERS)]);
    assert_eq!(both.members, [group(G_DIMMERS)]);
}

#[test]
fn removing_a_light_removes_its_values() {
    let show = test_show();
    let scene = scene_of(
        vec![group(G_SPOTS), fixture(RGB)],
        vec![
            full(WHOLE),
            full(group(G_SPOTS)),
            full(fixture(SPOT1)),
            full(fixture(RGB)),
            SceneValue {
                target: fixture(SPOT2),
                value: ValueSource::Channel {
                    channel: ChannelRef::Dmx {
                        attribute: Attribute::Pan,
                        index: 0,
                        fine: false,
                    },
                    value: 7,
                },
            },
        ],
    );
    let after = remove_scene_member(&show, &scene, group(G_SPOTS));
    assert_eq!(after.members, [fixture(RGB)]);
    assert_eq!(after.values, [full(WHOLE), full(fixture(RGB))]);
}

#[test]
fn a_light_still_in_the_scene_keeps_its_values() {
    let show = test_show();
    let scene = scene_of(
        vec![fixture(SPOT1), group(G_SPOTS)],
        vec![full(fixture(SPOT1)), full(fixture(SPOT2))],
    );
    let after = remove_scene_member(&show, &scene, group(G_SPOTS));
    assert_eq!(after.members, [fixture(SPOT1)]);
    assert_eq!(after.values, [full(fixture(SPOT1))]);
    // Taking out what was never added does nothing.
    assert_eq!(remove_scene_member(&show, &scene, fixture(DIM1)), scene);
}

#[test]
fn values_bring_their_lights_into_the_scene() {
    let show = test_show();
    let compiled = CompiledShow::build(&show);
    let scene = scene_of(vec![fixture(DIM1), fixture(DIM2)], Vec::new());
    let incoming = [
        full(WHOLE),
        full(fixture(DIM1)),
        full(group(G_DIMMERS)),
        full(fixture(RGB)),
    ];
    let after = add_scene_values(&show, &compiled, &scene, &incoming);
    // A group with a value is a member, so removing it can take the value away.
    assert_eq!(
        after.members,
        [fixture(DIM1), fixture(DIM2), group(G_DIMMERS), fixture(RGB)]
    );
    assert_eq!(after.values, incoming);
}

#[test]
fn scene_lights_lists_members_fixtures_and_groups() {
    let show = test_show();
    let scene = scene_of(
        vec![fixture(RGB), group(G_SPOTS), fixture(SPOT1)],
        Vec::new(),
    );
    let lights = scene_lights(&show, &scene);
    let members: Vec<(Target, Vec<FixtureId>)> = lights
        .members
        .into_iter()
        .map(|m| (m.target, m.fixtures))
        .collect();
    assert_eq!(
        members,
        [
            (fixture(RGB), vec![RGB]),
            (group(G_SPOTS), vec![SPOT1, SPOT2]),
            (fixture(SPOT1), vec![SPOT1]),
        ]
    );
    assert_eq!(lights.fixtures, [RGB, SPOT1, SPOT2]);
}

#[test]
fn the_channels_of_a_light_without_values_are_listed() {
    let mut show = test_show();
    show.functions
        .push(function(scene_of(vec![fixture(RGB)], Vec::new())));
    let compiled = CompiledShow::build(&show);
    let listed = scene_channels(&show, &compiled, FunctionId(1)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].fixture, RGB);
    assert!(listed[0]
        .channels
        .iter()
        .all(|c| c.origin == ChannelOrigin::Unset));
}

#[test]
fn storing_the_programmer_lists_its_lights() {
    let mut e = engine(test_show());
    for target in [fixture(DIM1), group(G_DIMMERS), fixture(RGB)] {
        ok(
            &mut e,
            0,
            Command::ProgrammerSet {
                target,
                value: intensity(1.0),
            },
        );
    }
    ok(
        &mut e,
        0,
        Command::StoreProgrammerAsScene {
            id: Some(FunctionId(1)),
            name: "p".into(),
        },
    );
    assert_eq!(stored(&e).members, [group(G_DIMMERS), fixture(RGB)]);
}

#[test]
fn deleted_lights_leave_the_scenes() {
    let members = vec![group(G_SPOTS), fixture(DIM1), fixture(RGB)];
    let mut e = run(test_show(), scene_of(members, Vec::new()));
    ok(&mut e, 0, Command::UnpatchFixture { id: DIM1 });
    assert_eq!(stored(&e).members, [group(G_SPOTS), fixture(RGB)]);
    ok(&mut e, 0, Command::RemoveGroup { id: G_SPOTS });
    assert_eq!(stored(&e).members, [fixture(RGB)]);
    ok(&mut e, 0, Command::RemoveUniverse { id: U1 });
    assert!(stored(&e).members.is_empty());
}

#[test]
fn only_scene_values_can_mean_the_whole_scene() {
    let mut e = engine(test_show());
    let invalid = |e: &mut klight_engine_core::Engine, c: Command| {
        assert!(matches!(err(e, 0, c), CommandError::InvalidValue { .. }));
    };
    invalid(
        &mut e,
        Command::ProgrammerSet {
            target: WHOLE,
            value: intensity(1.0),
        },
    );
    invalid(
        &mut e,
        Command::SetFunction {
            function: function(scene_of(vec![WHOLE], Vec::new())),
        },
    );
    let cue = Cue {
        number: 1.0,
        name: "q".into(),
        values: vec![full(WHOLE)],
        timing: Timing::default(),
        follow: CueFollow::default(),
        move_in_black: None,
    };
    invalid(
        &mut e,
        Command::SetFunction {
            function: Function {
                id: FunctionId(2),
                folder: None,
                name: "c".into(),
                kind: FunctionKind::CueList(CueList {
                    cues: vec![cue],
                    move_in_black: false,
                }),
            },
        },
    );
}
