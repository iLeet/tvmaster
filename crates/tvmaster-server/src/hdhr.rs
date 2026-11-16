//! HDHomeRun-compatible endpoints, so Plex, Jellyfin and Emby can add
//! TVMaster as a network tuner and show its channels in Live TV. The HTTP API
//! imitated here is the one those servers already speak; channels are served
//! as MPEG-TS at `/stream/{id}.ts`.

use crate::{ApiResult, AppState};
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

fn base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8787");
    format!("{}://{host}", crate::request_scheme(headers))
}

/// A stable 8-digit hex device id, created on first use.
fn device_id(s: &AppState) -> String {
    let db = s.db();
    if let Ok(Some(id)) = tvmaster_db::meta_get(&db, "hdhr_device_id") {
        return id;
    }
    let seed = format!("{}-{}", crate::plan::now_ms(), std::process::id());
    let id = format!("{:08X}", tvmaster_core::fnv1a(&seed) as u32);
    let _ = tvmaster_db::meta_set(&db, "hdhr_device_id", &id);
    id
}

pub async fn discover(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let base = base_url(&headers);
    let tok = crate::auth::url_suffix(&s);
    let tuners = tvmaster_db::list_channels(&s.db())?.len().clamp(1, 16);
    Ok(Json(json!({
        "FriendlyName": "TVMaster",
        "Manufacturer": "TVMaster",
        "ModelNumber": "HDHR3-US",
        "FirmwareName": "hdhomerun3_atsc",
        "FirmwareVersion": "20240101",
        "DeviceID": device_id(&s),
        "DeviceAuth": "tvmaster",
        "TunerCount": tuners,
        "BaseURL": base,
        "LineupURL": format!("{base}/lineup.json{tok}"),
    })))
}

pub async fn lineup(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let base = base_url(&headers);
    let tok = crate::auth::url_suffix(&s);
    let entries: Vec<Value> = tvmaster_db::list_channels(&s.db())?
        .into_iter()
        .map(|c| {
            json!({
                "GuideNumber": c.number.to_string(),
                "GuideName": c.name,
                "URL": format!("{base}/stream/{}.ts{tok}", c.id),
                "HD": 1,
            })
        })
        .collect();
    Ok(Json(Value::Array(entries)))
}

pub async fn lineup_status() -> Json<Value> {
    Json(json!({
        "ScanInProgress": 0,
        "ScanPossible": 1,
        "Source": "Cable",
        "SourceList": ["Cable"],
    }))
}

/// Servers ask for a channel scan; there is nothing to scan.
pub async fn lineup_post() -> &'static str {
    ""
}

pub async fn device_xml(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let base = base_url(&headers);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root xmlns=\"urn:schemas-upnp-org:device-1-0\">\n  <specVersion><major>1</major><minor>0</minor></specVersion>\n  <URLBase>{base}</URLBase>\n  <device>\n    <deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType>\n    <friendlyName>TVMaster</friendlyName>\n    <manufacturer>TVMaster</manufacturer>\n    <modelName>HDHR3-US</modelName>\n    <modelNumber>HDHR3-US</modelNumber>\n    <serialNumber></serialNumber>\n    <UDN>uuid:{}</UDN>\n  </device>\n</root>\n",
        device_id(&s)
    );
    let mut res = xml.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml"),
    );
    res
}

#[cfg(test)]
mod tests {
    use crate::test_util::{call, call_raw};
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn tuner_discovery_and_lineup_follow_the_channels() {
        let app = router(AppState::in_memory());
        for (id, n, name) in [("retro", 4, "Retro TV"), ("toons", 8, "Toon Town")] {
            call(
                &app,
                "POST",
                "/api/v1/channels",
                &format!(r#"{{"id":"{id}","number":{n},"name":"{name}"}}"#),
            )
            .await;
        }
        let (st, d) = call(&app, "GET", "/discover.json", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(d["TunerCount"], 2);
        assert_eq!(d["BaseURL"], "http://tv.local:8787");
        assert_eq!(d["LineupURL"], "http://tv.local:8787/lineup.json");
        let id = d["DeviceID"].as_str().unwrap().to_string();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        // the id is stable
        let (_, d2) = call(&app, "GET", "/discover.json", "").await;
        assert_eq!(d2["DeviceID"], id.as_str());

        let (_, lineup) = call(&app, "GET", "/lineup.json", "").await;
        let l = lineup.as_array().unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0]["GuideNumber"], "4");
        assert_eq!(l[0]["GuideName"], "Retro TV");
        assert_eq!(l[0]["URL"], "http://tv.local:8787/stream/retro.ts");

        let (_, status) = call(&app, "GET", "/lineup_status.json", "").await;
        assert_eq!(status["ScanInProgress"], 0);
        assert_eq!(
            call_raw(&app, "POST", "/lineup.post?scan=start", "")
                .await
                .0,
            StatusCode::OK
        );
        let (st, xml) = call_raw(&app, "GET", "/device.xml", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(xml.contains("<friendlyName>TVMaster</friendlyName>") && xml.contains(&id));
    }

    #[tokio::test]
    async fn an_empty_server_still_reports_one_tuner() {
        let app = router(AppState::in_memory());
        let (_, d) = call(&app, "GET", "/discover.json", "").await;
        assert_eq!(d["TunerCount"], 1);
        let (_, l) = call(&app, "GET", "/lineup.json", "").await;
        assert_eq!(l.as_array().unwrap().len(), 0);
    }
}
