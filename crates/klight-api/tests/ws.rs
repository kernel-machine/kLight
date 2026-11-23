#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use klight_api::*;
use klight_engine_core::{Engine, EngineConfig, MonotonicClock, TickLoop};
use klight_model::*;
use tokio_tungstenite::tungstenite::Message;

async fn next_msg<S>(ws: &mut S) -> ServerMessage
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timeout")
            .unwrap()
            .unwrap();
        if let Message::Text(t) = m {
            return serde_json::from_str(t.as_str()).unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connect_command_ack_and_updates() {
    let mut show = Show::new("api");
    show.universes.push(Universe {
        id: UniverseId(1),
        name: "U1".into(),
        output: None,
    });
    let engine = Engine::new(show, EngineConfig::default());
    let shared = engine.shared();
    let (ctx, crx) = crossbeam_channel::bounded(64);
    let (rtx, rrx) = crossbeam_channel::bounded(64);
    let tick = TickLoop::spawn(
        engine,
        Arc::new(MonotonicClock::new()),
        crx,
        rtx,
        Default::default(),
    )
    .unwrap();
    let config = ApiConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        static_dir: None,
        tokens: vec![],
        local_permission: Permission::Full,
        library: Arc::new(vec![]),
    };
    let hooks = ApiHooks {
        status: Arc::new(SystemStatus::default),
        midi_in: None,
        learn: None,
        ports: None,
        input_profiles: None,
    };
    let server = serve(config, shared, ctx, rrx, hooks).await.unwrap();
    let url = format!("ws://{}/ws", server.local_addr);
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();

    ws.send(Message::Text(r#"{"type":"hello","name":"test"}"#.into()))
        .await
        .unwrap();
    assert!(matches!(
        next_msg(&mut ws).await,
        ServerMessage::Welcome {
            permission: Permission::Full,
            ..
        }
    ));
    let ServerMessage::Snapshot { seq, show, .. } = next_msg(&mut ws).await else {
        panic!("expected snapshot")
    };
    assert_eq!(seq, 1);
    assert_eq!(show.name, "api");

    // Malformed input is reported, not fatal.
    ws.send(Message::Text("{not json".into())).await.unwrap();
    assert!(matches!(
        next_msg(&mut ws).await,
        ServerMessage::Error {
            error: ApiError::Malformed { .. },
            ..
        }
    ));

    let cmd = ClientMessage::Command {
        id: 42,
        command: Command::SetGrandMaster { level: 0.5 },
    };
    ws.send(Message::Text(serde_json::to_string(&cmd).unwrap().into()))
        .await
        .unwrap();
    let mut got_ack = false;
    let mut got_runtime = false;
    for _ in 0..20 {
        match next_msg(&mut ws).await {
            ServerMessage::Ack { id: 42, .. } => got_ack = true,
            ServerMessage::Runtime { runtime, .. }
                if (runtime.state.grand_master - 0.5).abs() < 1e-6 =>
            {
                got_runtime = true
            }
            _ => {}
        }
        if got_ack && got_runtime {
            break;
        }
    }
    assert!(got_ack && got_runtime);

    // Typed command error.
    let cmd = ClientMessage::Command {
        id: 43,
        command: Command::StartFunction { id: FunctionId(9) },
    };
    ws.send(Message::Text(serde_json::to_string(&cmd).unwrap().into()))
        .await
        .unwrap();
    loop {
        if let ServerMessage::Error {
            id: Some(43),
            error: ApiError::Command { .. },
            ..
        } = next_msg(&mut ws).await
        {
            break;
        }
    }

    // DMX monitor.
    ws.send(Message::Text(
        r#"{"type":"subscribe_dmx","universes":[1]}"#.into(),
    ))
    .await
    .unwrap();
    loop {
        if let ServerMessage::Dmx { data, .. } = next_msg(&mut ws).await {
            assert_eq!(data.len(), 512);
            break;
        }
    }
    drop(ws);
    server.shutdown().await;
    tick.stop();
}
