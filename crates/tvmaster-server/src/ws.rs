//! WebSocket: live updates and remote control.
//!
//! Server to client: `snapshot` on connect, then `tv.state`, `channel.now` and
//! `schedule.changed` as they happen. Client to server:
//!
//! - `{"type":"hello","tv":"living-room"}`: this socket is that TV's player
//!   (the TV shows as online while it stays connected).
//! - `{"type":"tv","id":"living-room","channel":4}` (also `step`, `power`,
//!   `volume`, `muted`, in any combination): control a TV, like the REST calls.
//! - `{"type":"ping"}`: answered with `{"type":"pong"}`.

use crate::tvs;
use crate::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;

pub async fn handler(ws: WebSocketUpgrade, State(s): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| session(socket, s))
}

async fn send(socket: &mut WebSocket, v: Value) -> bool {
    socket.send(Message::Text(v.to_string())).await.is_ok()
}

fn error(message: impl Into<String>) -> Value {
    json!({ "type": "error", "message": message.into() })
}

/// Apply a `tv` command. Returns an error message to send back, if any.
fn apply_tv_command(s: &AppState, v: &Value) -> Result<(), String> {
    let id = v["id"].as_str().ok_or("tv command needs an \"id\"")?;
    let mut touched = false;
    let fail = |e: crate::ApiError| e.message().to_string();
    if v.get("channel").is_some() || v.get("step").is_some() {
        let number = v["channel"].as_u64().map(|n| n as u32);
        tvs::set_channel_inner(s, id, number, v["step"].as_str()).map_err(fail)?;
        touched = true;
    }
    if let Some(on) = v["power"].as_bool() {
        tvs::set_power_inner(s, id, on).map_err(fail)?;
        touched = true;
    }
    let level = v["volume"].as_u64().map(|l| l.min(255) as u8);
    let muted = v["muted"].as_bool();
    if level.is_some() || muted.is_some() {
        tvs::set_volume_inner(s, id, level, muted).map_err(fail)?;
        touched = true;
    }
    if touched {
        Ok(())
    } else {
        Err("nothing to do: send channel, step, power, volume or muted".into())
    }
}

async fn session(mut socket: WebSocket, s: AppState) {
    let mut rx = s.events.subscribe();
    let mut player_of: Option<String> = None;

    let snapshot = {
        let db = s.db();
        let tvs: Vec<_> = tvmaster_db::list_tvs(&db)
            .unwrap_or_default()
            .into_iter()
            .map(|t| s.tv_view(t))
            .collect();
        let channels: Vec<Value> = tvmaster_db::list_channels(&db)
            .unwrap_or_default()
            .into_iter()
            .map(|c| json!({ "id": c.id, "number": c.number, "name": c.name, "kind": c.kind, "group": c.group }))
            .collect();
        json!({ "type": "snapshot", "tvs": tvs, "channels": channels })
    };
    if !send(&mut socket, snapshot).await {
        return;
    }

    loop {
        tokio::select! {
            msg = socket.recv() => {
                let Some(Ok(msg)) = msg else { break };
                match msg {
                    Message::Text(text) => {
                        let Ok(v) = serde_json::from_str::<Value>(&text) else {
                            if !send(&mut socket, error("messages must be JSON")).await { break }
                            continue;
                        };
                        let reply = match v["type"].as_str() {
                            Some("ping") => Some(json!({ "type": "pong" })),
                            Some("hello") => {
                                match v["tv"].as_str() {
                                    Some(id) if tvs::valid_tv_id(id) => {
                                        if let Some(old) = player_of.take() {
                                            s.presence_dec(&old);
                                        }
                                        // A player may announce itself before anyone registered it.
                                        let known = tvmaster_db::get_tv(&s.db(), id).ok().flatten().is_some();
                                        if !known {
                                            let _ = tvs::register_inner(&s, id, id);
                                        }
                                        s.presence_inc(id);
                                        player_of = Some(id.to_string());
                                        None
                                    }
                                    _ => Some(error(r#"hello needs a valid "tv" id"#)),
                                }
                            }
                            Some("tv") => apply_tv_command(&s, &v).err().map(error),
                            _ => Some(error("unknown message type")),
                        };
                        if let Some(r) = reply {
                            if !send(&mut socket, r).await { break }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            ev = rx.recv() => match ev {
                Ok(text) => {
                    if socket.send(Message::Text(text)).await.is_err() { break }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            },
        }
    }
    if let Some(id) = player_of {
        s.presence_dec(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::call;
    use crate::{router, AppState};
    use futures_util::{SinkExt, StreamExt};
    use std::time::Duration;
    use tokio_tungstenite::{connect_async, tungstenite::Message as Msg};

    type Client = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    /// A real server on a free port, plus a second router on the same state for REST calls.
    async fn serve() -> (String, axum::Router) {
        let state = AppState::in_memory();
        let rest = router(state.clone());
        for (id, n) in [("one", 3), ("two", 5)] {
            call(
                &rest,
                "POST",
                "/api/v1/channels",
                &format!(r#"{{"id":"{id}","number":{n},"name":"{id}"}}"#),
            )
            .await;
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(state);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("ws://{addr}/api/v1/ws"), rest)
    }

    async fn next(ws: &mut Client) -> Value {
        loop {
            let m = tokio::time::timeout(Duration::from_secs(3), ws.next())
                .await
                .expect("timed out waiting for a message")
                .expect("socket closed")
                .expect("socket error");
            if let Msg::Text(t) = m {
                return serde_json::from_str(&t).unwrap();
            }
        }
    }

    /// Read messages until one matches.
    async fn until(ws: &mut Client, pred: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..20 {
            let v = next(ws).await;
            if pred(&v) {
                return v;
            }
        }
        panic!("no matching message");
    }

    async fn say(ws: &mut Client, v: Value) {
        ws.send(Msg::Text(v.to_string())).await.unwrap();
    }

    #[tokio::test]
    async fn a_new_client_gets_a_snapshot_and_live_tv_changes() {
        let (url, rest) = serve().await;
        call(&rest, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        let (mut ws, _) = connect_async(&url).await.unwrap();
        let snap = next(&mut ws).await;
        assert_eq!(snap["type"], "snapshot");
        assert_eq!(snap["tvs"][0]["id"], "den");
        assert_eq!(snap["channels"].as_array().unwrap().len(), 2);

        // a REST change (the phone remote) arrives instantly
        call(&rest, "POST", "/api/v1/tvs/den/channel", r#"{"number":5}"#).await;
        let ev = until(&mut ws, |v| v["type"] == "tv.state").await;
        assert_eq!(ev["tv"]["id"], "den");
        assert_eq!(ev["tv"]["channel"], 5);
        assert_eq!(ev["tv"]["power"], true);

        call(&rest, "POST", "/api/v1/tvs/den/volume", r#"{"level":30}"#).await;
        let ev = until(&mut ws, |v| v["type"] == "tv.state").await;
        assert_eq!(ev["tv"]["volume"], 30);
    }

    #[tokio::test]
    async fn commands_over_the_socket_reach_other_clients() {
        let (url, rest) = serve().await;
        call(&rest, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        let (mut remote, _) = connect_async(&url).await.unwrap();
        let (mut player, _) = connect_async(&url).await.unwrap();
        next(&mut remote).await;
        next(&mut player).await;

        say(
            &mut remote,
            json!({"type":"tv","id":"den","channel":3,"volume":40}),
        )
        .await;
        let ev = until(&mut player, |v| {
            v["type"] == "tv.state" && v["tv"]["volume"] == 40
        })
        .await;
        assert_eq!(ev["tv"]["channel"], 3);
        // step up from 3 goes to 5
        say(&mut remote, json!({"type":"tv","id":"den","step":"up"})).await;
        let ev = until(&mut player, |v| {
            v["type"] == "tv.state" && v["tv"]["channel"] == 5
        })
        .await;
        assert_eq!(ev["tv"]["power"], true);
        say(&mut remote, json!({"type":"tv","id":"den","power":false})).await;
        until(&mut player, |v| {
            v["type"] == "tv.state" && v["tv"]["power"] == false
        })
        .await;
    }

    #[tokio::test]
    async fn players_are_online_while_connected() {
        let (url, rest) = serve().await;
        let (mut watcher, _) = connect_async(&url).await.unwrap();
        next(&mut watcher).await;

        let (mut player, _) = connect_async(&url).await.unwrap();
        next(&mut player).await;
        // an unregistered TV that says hello is created
        say(&mut player, json!({"type":"hello","tv":"kitchen"})).await;
        let ev = until(&mut watcher, |v| {
            v["type"] == "tv.state" && v["tv"]["online"] == true
        })
        .await;
        assert_eq!(ev["tv"]["id"], "kitchen");
        let (_, tv) = call(&rest, "GET", "/api/v1/tvs/kitchen", "").await;
        assert_eq!(tv["online"], true);
        let (_, all) = call(&rest, "GET", "/api/v1/tvs", "").await;
        assert_eq!(all[0]["online"], true);

        drop(player);
        let ev = until(&mut watcher, |v| {
            v["type"] == "tv.state" && v["tv"]["online"] == false
        })
        .await;
        assert_eq!(ev["tv"]["id"], "kitchen");
        let (_, tv) = call(&rest, "GET", "/api/v1/tvs/kitchen", "").await;
        assert_eq!(tv["online"], false);
    }

    #[tokio::test]
    async fn a_tv_stays_online_until_its_last_player_leaves() {
        let (url, rest) = serve().await;
        let (mut a, _) = connect_async(&url).await.unwrap();
        let (mut b, _) = connect_async(&url).await.unwrap();
        next(&mut a).await;
        next(&mut b).await;
        say(&mut a, json!({"type":"hello","tv":"den"})).await;
        say(&mut b, json!({"type":"hello","tv":"den"})).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        drop(a);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let (_, tv) = call(&rest, "GET", "/api/v1/tvs/den", "").await;
        assert_eq!(tv["online"], true, "one player is still connected");
        drop(b);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let (_, tv) = call(&rest, "GET", "/api/v1/tvs/den", "").await;
        assert_eq!(tv["online"], false);
    }

    #[tokio::test]
    async fn bad_messages_get_errors_and_ping_gets_pong() {
        let (url, rest) = serve().await;
        call(&rest, "POST", "/api/v1/tvs", r#"{"id":"den","name":"Den"}"#).await;
        let (mut ws, _) = connect_async(&url).await.unwrap();
        next(&mut ws).await;
        ws.send(Msg::Text("not json".into())).await.unwrap();
        assert_eq!(next(&mut ws).await["type"], "error");
        say(&mut ws, json!({"type":"nope"})).await;
        assert_eq!(next(&mut ws).await["type"], "error");
        say(&mut ws, json!({"type":"tv","id":"den","channel":99})).await;
        let e = next(&mut ws).await;
        assert_eq!(e["type"], "error");
        assert!(e["message"].as_str().unwrap().contains("99"));
        say(&mut ws, json!({"type":"tv","id":"ghost","channel":3})).await;
        assert_eq!(next(&mut ws).await["type"], "error");
        say(&mut ws, json!({"type":"tv","id":"den"})).await;
        assert_eq!(next(&mut ws).await["type"], "error");
        say(&mut ws, json!({"type":"hello","tv":"Bad Id"})).await;
        assert_eq!(next(&mut ws).await["type"], "error");
        say(&mut ws, json!({"type":"ping"})).await;
        assert_eq!(next(&mut ws).await["type"], "pong");
    }

    #[tokio::test]
    async fn schedule_changes_are_announced() {
        let (url, rest) = serve().await;
        let (mut ws, _) = connect_async(&url).await.unwrap();
        next(&mut ws).await;
        call(&rest, "POST", "/api/v1/channels/one/reroll", "").await;
        let ev = until(&mut ws, |v| v["type"] == "schedule.changed").await;
        assert_eq!(ev["channel"], "one");
    }
}
