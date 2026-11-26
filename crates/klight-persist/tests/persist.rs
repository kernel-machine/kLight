#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use klight_engine_core::{Engine, EngineConfig};
use klight_model::*;
use klight_persist::*;

fn sample_show() -> Show {
    let mut s = Show::new("sample");
    s.universes.push(Universe {
        id: UniverseId(1),
        name: "U1".into(),
        output: None,
    });
    s
}

#[test]
fn every_schema_version_example_loads() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            let show = parse_show_bytes(&std::fs::read(&path).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(show.schema_version, SCHEMA_VERSION);
            count += 1;
        }
    }
    // One example per version, from 0 up to the current one.
    assert_eq!(count, SCHEMA_VERSION as usize + 1);
}

#[test]
fn save_load_roundtrip_and_backups() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("show.klight.json");
    let mut show = sample_show();
    for i in 0..5 {
        show.name = format!("v{i}");
        save_show(&path, &show, 3).unwrap();
    }
    let loaded = load_show(&path).unwrap();
    assert_eq!(loaded.show.name, "v4");
    assert!(loaded.from_backup.is_none());
    let backups = list_backups(&path);
    assert_eq!(backups.len(), 3);
    let newest = parse_show_bytes(&std::fs::read(&backups[0]).unwrap()).unwrap();
    assert_eq!(newest.name, "v3");
    // No temp files left behind.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn damaged_file_falls_back_to_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("show.json");
    let mut show = sample_show();
    show.name = "good".into();
    save_show(&path, &show, 3).unwrap();
    show.name = "newer".into();
    save_show(&path, &show, 3).unwrap();
    std::fs::write(&path, b"{ this is not json").unwrap();
    let loaded = load_show(&path).unwrap();
    assert_eq!(loaded.show.name, "good");
    assert!(loaded.from_backup.is_some());
    assert!(!loaded.problems.is_empty());
}

#[test]
fn garbage_never_panics() {
    for input in [
        &b""[..],
        b"null",
        b"[]",
        b"{\"schema_version\": 999}",
        b"{\"schema_version\": -1}",
        b"\xff\xfe",
        b"{\"universes\": 5}",
    ] {
        assert!(parse_show_bytes(input).is_err());
    }
}

#[test]
fn older_scenes_list_the_lights_their_values_name() {
    // A group, one of its fixtures and a head of another one.
    let old = br#"{
        "schema_version": 3, "name": "old", "universes": [], "definitions": [], "fixtures": [],
        "groups": [{ "id": 1, "name": "G", "grid_width": 0, "grid_height": 0,
                     "members": [{ "fixture": 2, "head": 0, "grid": null }] }],
        "palettes": [],
        "functions": [{ "id": 1, "name": "S", "kind": { "type": "scene", "values": [
            { "target": { "type": "fixture", "id": 2 }, "value": { "type": "palette", "id": 1 } },
            { "target": { "type": "head", "id": 3, "head": 1 }, "value": { "type": "palette", "id": 1 } },
            { "target": { "type": "group", "id": 1 }, "value": { "type": "palette", "id": 1 } },
            { "target": { "type": "fixture", "id": 3 }, "value": { "type": "palette", "id": 1 } }
        ] } }],
        "virtual_console": { "widgets": [] }, "input_devices": [], "input_mappings": [],
        "settings": { "tick_hz": 44, "design_pin": null, "output_refresh_ms": 1000 }
    }"#;
    let show = parse_show_bytes(old).unwrap();
    let FunctionKind::Scene(scene) = &show.functions[0].kind else {
        panic!("not a scene");
    };
    // The group stands for its fixtures; each other fixture appears once.
    assert_eq!(
        scene.members,
        [
            Target::Group { id: GroupId(1) },
            Target::Fixture { id: FixtureId(3) }
        ]
    );
    assert!(scene.new_members_follow);
    assert_eq!(scene.values.len(), 4);
}

#[test]
fn future_versions_are_rejected() {
    let err = parse_show_bytes(br#"{"schema_version": 99, "name": "x"}"#).unwrap_err();
    assert!(matches!(err, PersistError::UnsupportedVersion { .. }));
}

#[test]
fn runtime_state_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = runtime_path(&dir.path().join("a.json"));
    let state = RuntimeState {
        grand_master: 0.25,
        active: vec![ActiveFunction {
            id: FunctionId(3),
            level: 0.5,
            position: Some(2),
            playing: Vec::new(),
            paused: false,
        }],
        ..RuntimeState::default()
    };
    save_runtime(&path, &state).unwrap();
    assert_eq!(load_runtime(&path).unwrap(), state);
}

#[test]
fn autosave_writes_after_debounce_and_on_stop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auto.json");
    let mut engine = Engine::new(sample_show(), EngineConfig::default());
    let shared = engine.shared();
    let mut cfg = AutosaveConfig::new(path.clone());
    cfg.debounce = Duration::from_millis(100);
    cfg.runtime_interval = Duration::from_millis(50);
    let saver = Autosaver::spawn(Arc::clone(&shared), cfg).unwrap();
    engine.apply(
        0,
        CommandEnvelope {
            id: 1,
            source: CommandSource::Internal,
            command: Command::SetShowName {
                name: "edited".into(),
            },
        },
    );
    engine.apply(
        0,
        CommandEnvelope {
            id: 2,
            source: CommandSource::Internal,
            command: Command::SetGrandMaster { level: 0.3 },
        },
    );
    engine.tick(0);
    let start = Instant::now();
    while saver.status().last_show_save_ms.is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "autosave did not happen: {:?}",
            saver.status()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(load_show(&path).unwrap().show.name, "edited");
    engine.apply(
        0,
        CommandEnvelope {
            id: 3,
            source: CommandSource::Internal,
            command: Command::SetShowName {
                name: "final".into(),
            },
        },
    );
    saver.stop();
    assert_eq!(
        load_show(&path).unwrap().show.name,
        "final",
        "flush on stop"
    );
    let rt = load_runtime(&runtime_path(&path)).unwrap();
    assert!((rt.grand_master - 0.3).abs() < 1e-6);
}
