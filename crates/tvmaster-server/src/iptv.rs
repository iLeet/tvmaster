//! IPTV outputs: M3U playlist and XMLTV guide.

use crate::plan::{now_ms, programs};
use crate::{ApiResult, AppState};
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
};
use tvmaster_core::xmltv_time;

/// Guide look-ahead.
const GUIDE_HOURS: i64 = 24;
/// Safety cap on programmes per channel (very short items).
const MAX_PROGRAMMES: usize = 1000;

fn base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8787");
    format!("{}://{host}", crate::request_scheme(headers))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn m3u_attr(s: &str) -> String {
    s.replace('"', "'").replace(['\n', '\r'], " ")
}

fn text_response(content_type: &'static str, body: String) -> Response {
    let mut res = body.into_response();
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    res
}

pub async fn playlist(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    let base = base_url(&headers);
    let tok = crate::auth::url_suffix(&s);
    let channels = tvmaster_db::list_channels(&s.db())?;
    let mut out = format!("#EXTM3U x-tvg-url=\"{base}/iptv/guide.xml{tok}\"\n");
    for c in channels {
        let name = m3u_attr(&c.name);
        out.push_str(&format!(
            "#EXTINF:-1 tvg-id=\"{id}\" tvg-chno=\"{n}\" tvg-name=\"{name}\" group-title=\"{group}\",{name}\n{base}/stream/{id}/index.m3u8{tok}\n",
            id = c.id,
            n = c.number,
            group = m3u_attr(c.group.as_deref().unwrap_or("TVMaster")),
        ));
    }
    Ok(text_response("audio/x-mpegurl; charset=utf-8", out))
}

pub async fn guide(State(s): State<AppState>) -> ApiResult<Response> {
    let channels = tvmaster_db::list_channels(&s.db())?;
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tv generator-info-name=\"TVMaster\">\n",
    );
    for c in &channels {
        xml.push_str(&format!(
            "  <channel id=\"{id}\"><display-name>{name}</display-name><display-name>{n}</display-name></channel>\n",
            id = xml_escape(&c.id),
            name = xml_escape(&c.name),
            n = c.number
        ));
    }
    let start = now_ms();
    let end = start + GUIDE_HOURS * 3_600_000;
    let info = tvmaster_db::list_show_info(&s.db())?;
    for c in channels {
        if c.kind == tvmaster_core::ChannelKind::Guide {
            // One long programme, so clients show the guide channel as "on".
            let hour = 3_600_000;
            let from = start.div_euclid(hour) * hour;
            xml.push_str(&format!(
                "  <programme start=\"{}\" stop=\"{}\" channel=\"{}\">\n    <title>{}</title>\n    <desc>Live listing of what is on every channel.</desc>\n  </programme>\n",
                xmltv_time(from),
                xmltv_time(end),
                xml_escape(&c.id),
                xml_escape(&format!("{} Guide", c.name)),
            ));
            continue;
        }
        let progs = programs(&s.db(), &c.id, start, end)?;
        for p in progs.into_iter().take(MAX_PROGRAMMES) {
            let main_title = p.show.clone().unwrap_or_else(|| p.title.clone());
            xml.push_str(&format!(
                "  <programme start=\"{}\" stop=\"{}\" channel=\"{}\">\n    <title>{}</title>\n",
                xmltv_time(p.start_ms),
                xmltv_time(p.end_ms),
                xml_escape(&c.id),
                xml_escape(&main_title),
            ));
            if p.show.is_some() && !p.title.is_empty() {
                xml.push_str(&format!(
                    "    <sub-title>{}</sub-title>\n",
                    xml_escape(&p.title)
                ));
            }
            if let Some(i) = p.show.as_ref().and_then(|n| info.get(&n.trim().to_lowercase())) {
                if !i.description.trim().is_empty() {
                    xml.push_str(&format!("    <desc>{}</desc>\n", xml_escape(i.description.trim())));
                }
                for t in &i.tags {
                    xml.push_str(&format!("    <category>{}</category>\n", xml_escape(t)));
                }
                if !i.rating.trim().is_empty() {
                    xml.push_str(&format!(
                        "    <rating><value>{}</value></rating>\n",
                        xml_escape(i.rating.trim())
                    ));
                }
            }
            if let Some(label) = &p.label {
                xml.push_str(&format!("    <category>{}</category>\n", xml_escape(label)));
            }
            if let (Some(sn), Some(ep)) = (p.season, p.episode) {
                xml.push_str(&format!(
                    "    <episode-num system=\"onscreen\">S{sn:02}E{ep:02}</episode-num>\n"
                ));
            }
            xml.push_str("  </programme>\n");
        }
    }
    xml.push_str("</tv>\n");
    Ok(text_response("application/xml; charset=utf-8", xml))
}

#[cfg(test)]
mod tests {
    use crate::test_util::call_raw;
    use crate::{router, AppState};
    use tvmaster_db::{add_library, upsert_media, NewMedia};

    async fn app() -> axum::Router {
        let state = AppState::in_memory();
        let lib = {
            let db = state.db();
            let lib = add_library(&db, "/tv", "shows").unwrap();
            upsert_media(
                &db,
                &NewMedia {
                    library_id: lib.id,
                    path: "/tv/a.mp4".into(),
                    show: Some("Tom & Jerry".into()),
                    season: Some(1),
                    episode: Some(2),
                    title: Some("The <Cat>".into()),
                    duration_ms: 600_000,
                    ..Default::default()
                },
            )
            .unwrap();
            lib.id
        };
        let app = router(state);
        call_raw(
            &app,
            "POST",
            "/api/v1/channels",
            &format!(
                r#"{{"id":"toons","number":8,"name":"Toons \"24/7\"","library_ids":[{lib}]}}"#
            ),
        )
        .await;
        app
    }

    #[tokio::test]
    async fn m3u_uses_request_host_and_channel_numbers() {
        let app = app().await;
        let (_, m3u) = call_raw(&app, "GET", "/iptv/playlist.m3u", "").await;
        assert!(m3u.starts_with("#EXTM3U x-tvg-url=\"http://tv.local:8787/iptv/guide.xml\""));
        assert!(m3u.contains("tvg-id=\"toons\" tvg-chno=\"8\""));
        assert!(m3u.contains("http://tv.local:8787/stream/toons/index.m3u8"));
        assert!(
            !m3u.contains("\"24/7\""),
            "quotes in names must be neutralised"
        );
    }

    #[tokio::test]
    async fn xmltv_escapes_and_lists_programmes() {
        let app = app().await;
        let (_, xml) = call_raw(&app, "GET", "/iptv/guide.xml", "").await;
        assert!(xml.contains("<channel id=\"toons\">"));
        assert!(xml.contains("<title>Tom &amp; Jerry</title>"));
        assert!(xml.contains("<sub-title>The &lt;Cat&gt;</sub-title>"));
        assert!(xml.contains("S01E02"));
        // 10-minute items over 24h: lots of programmes
        assert!(xml.matches("<programme ").count() >= 100);
        assert!(xml.trim_end().ends_with("</tv>"));
    }
}
