//! Turns database rows into a channel's timeline and playout spec.

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use tvmaster_core::{
    order_sequential, order_unsorted, Channel, MediaRef, Slot, Strategy, Timeline,
};
use tvmaster_db::MediaRow;
use tvmaster_playout::{ChannelSpec, MediaFile};

pub struct Plan {
    pub channel: Channel,
    pub timeline: Timeline,
    pub media: HashMap<i64, MediaRow>,
}

pub fn build_plan(conn: &Connection, channel: Channel) -> rusqlite::Result<Option<Plan>> {
    let rows = tvmaster_db::list_media_in_libraries(conn, &channel.library_ids)?;
    let refs: Vec<MediaRef> = rows
        .iter()
        .map(|m| MediaRef {
            id: m.id,
            show: m.show.clone(),
            season: m.season.map(|s| s as u32),
            episode: m.episode.map(|e| e as u32),
            sort_key: m.path.clone(),
            duration_ms: m.duration_ms.max(0) as u64,
        })
        .collect();
    let shuffle = matches!(channel.strategy, Strategy::Shuffle | Strategy::Chaos);
    // M2: block grids behave like sequential until the scheduler lands (M3).
    let entries = if shuffle {
        order_unsorted(&refs)
    } else {
        order_sequential(&refs)
    };
    let Some(timeline) = Timeline::new(entries, channel.epoch_ms, channel.seed, shuffle) else {
        return Ok(None);
    };
    let media = rows.into_iter().map(|m| (m.id, m)).collect();
    Ok(Some(Plan {
        channel,
        timeline,
        media,
    }))
}

pub fn build_spec(conn: &Connection, channel_id: &str) -> rusqlite::Result<Option<ChannelSpec>> {
    let Some(channel) = tvmaster_db::get_channel(conn, channel_id)? else {
        return Ok(None);
    };
    let always_on = channel.always_on;
    let Some(plan) = build_plan(conn, channel)? else {
        return Ok(None);
    };
    let files = plan
        .media
        .values()
        .map(|m| {
            (
                m.id,
                MediaFile {
                    path: PathBuf::from(&m.path),
                    has_audio: m.acodec.is_some(),
                },
            )
        })
        .collect();
    Ok(Some(ChannelSpec {
        id: plan.channel.id,
        always_on,
        timeline: plan.timeline,
        files,
    }))
}

/// A programme as shown to clients (guide, now/next, schedule).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Program {
    pub media_id: i64,
    pub show: Option<String>,
    pub title: String,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub start_ms: i64,
    pub end_ms: i64,
}

pub fn program(slot: &Slot, media: &HashMap<i64, MediaRow>) -> Program {
    let m = media.get(&slot.media_id);
    let title = m
        .and_then(|m| m.title.clone())
        .filter(|t| !t.is_empty())
        .or_else(|| m.and_then(|m| m.show.clone()))
        .unwrap_or_else(|| "Untitled".to_string());
    Program {
        media_id: slot.media_id,
        show: m.and_then(|m| m.show.clone()),
        title,
        season: m.and_then(|m| m.season),
        episode: m.and_then(|m| m.episode),
        start_ms: slot.start_ms,
        end_ms: slot.end_ms,
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
