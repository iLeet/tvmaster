//! The channel creator: the kinds of channel a station can add, each with a sensible start (a
//! name, the next free number on the dial, and the settings that make it work), so every client
//! offers the same choices and a new channel is on air in a couple of clicks.

use crate::{ApiResult, AppState};
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// News feeds worth offering as a start; any RSS or Atom address works.
const FEEDS: &[(&str, &str)] = &[
    (
        "CBC Top Stories",
        "https://www.cbc.ca/webfeed/rss/rss-topstories",
    ),
    ("BBC World News", "https://feeds.bbci.co.uk/news/rss.xml"),
    ("NPR News", "https://feeds.npr.org/1001/rss.xml"),
    ("Associated Press", "https://feedx.net/rss/ap.xml"),
];

struct Template {
    id: &'static str,
    /// Where it sits in the chooser.
    group: &'static str,
    kind: &'static str,
    name: &'static str,
    about: &'static str,
    /// What a new channel of this kind is called until you say otherwise.
    channel_name: &'static str,
    /// What still has to be filled in before it is worth airing.
    needs: &'static [&'static str],
    body: fn() -> Value,
}

fn slide(kind: &str, title: &str, lines: &[&str]) -> Value {
    json!({ "kind": kind, "title": title, "lines": lines })
}

const TEMPLATES: &[Template] = &[
    Template {
        id: "tv",
        group: "TV channels",
        kind: "programming",
        name: "TV channel",
        about: "Plays shows, movies and cartoons from your libraries, with a schedule, commercials and bumpers",
        channel_name: "",
        needs: &["libraries"],
        body: || json!({ "kind": "programming", "strategy": "sequential", "autopilot": true }),
    },
    Template {
        id: "guide",
        group: "Guide",
        kind: "guide",
        name: "Guide channel",
        about: "The scrolling listing of every channel, TV and info, with what is on now and next, and music",
        channel_name: "Guide",
        needs: &[],
        body: || json!({ "kind": "guide", "guide": {} }),
    },
    Template {
        id: "info_news",
        group: "Info channels",
        kind: "info",
        name: "News and headlines",
        about: "Headlines from the news feeds you choose, a ticker and the time",
        channel_name: "News",
        needs: &["feeds"],
        body: || {
            json!({ "kind": "info", "info": {
                "about": "News and headlines",
                "slides": [slide("headlines", "Headlines", &[]), slide("clock", "", &[])],
                "seconds": 14,
            } })
        },
    },
    Template {
        id: "info_tonight",
        group: "Info channels",
        kind: "info",
        name: "What's on tonight",
        about: "A poster of what is on now and next on every channel, page after page",
        channel_name: "On Tonight",
        needs: &[],
        body: || {
            json!({ "kind": "info", "info": {
                "about": "What's on tonight",
                "slides": [slide("tonight", "On tonight", &[])],
                "seconds": 12,
            } })
        },
    },
    Template {
        id: "info_bulletin",
        group: "Info channels",
        kind: "info",
        name: "Community bulletin",
        about: "Notices and announcements you write: birthdays, events, the family calendar, house rules",
        channel_name: "Bulletin",
        needs: &["pages"],
        body: || {
            json!({ "kind": "info", "info": {
                "about": "Notices and announcements",
                "slides": [slide("text", "Welcome", &["This is your bulletin board.", "Add notices, pictures and reminders."])],
                "seconds": 12,
            } })
        },
    },
    Template {
        id: "info_photos",
        group: "Info channels",
        kind: "info",
        name: "Picture show",
        about: "Your pictures one after another, full screen, with captions and music",
        channel_name: "Pictures",
        needs: &["pages", "music"],
        body: || {
            json!({ "kind": "info", "info": {
                "about": "Pictures and music",
                "slides": [],
                "seconds": 10,
                "clock": false,
                "ticker_enabled": false,
            } })
        },
    },
    Template {
        id: "info_clock",
        group: "Info channels",
        kind: "info",
        name: "Clock",
        about: "A large clock and the date, like a time-of-day channel",
        channel_name: "Clock",
        needs: &[],
        body: || {
            json!({ "kind": "info", "info": {
                "about": "Time and date",
                "slides": [slide("clock", "", &[])],
                "ticker_enabled": false,
                "clock": false,
            } })
        },
    },
    Template {
        id: "test",
        group: "Test patterns",
        kind: "test",
        name: "Test pattern",
        about: "Colour bars and a tone, snow, a card or a picture of your own, all day",
        channel_name: "Test Pattern",
        needs: &[],
        body: || {
            json!({ "kind": "test", "options": { "off_air": { "screen": "bars", "tone": true } } })
        },
    },
];

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let t = out.trim_matches('-');
    let t: String = t.chars().take(24).collect();
    if t.is_empty() {
        "channel".into()
    } else {
        t
    }
}

fn unused_id(base: &str, taken: &[String]) -> String {
    let mut id = base.to_string();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// The lowest number nobody has.
fn free_number(taken: &[u32]) -> u32 {
    (1..).find(|n| !taken.contains(n)).unwrap_or(1)
}

/// `GET /api/v1/channel-templates`: the kinds of channel to add, with a suggested id, number and name for each.
pub async fn get(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let channels = tvmaster_db::list_channels(&s.db())?;
    let ids: Vec<String> = channels.iter().map(|c| c.id.clone()).collect();
    let numbers: Vec<u32> = channels.iter().map(|c| c.number).collect();
    let mut taken = numbers.clone();
    let list: Vec<Value> = TEMPLATES
        .iter()
        .map(|t| {
            let name = if t.channel_name.is_empty() {
                format!("Channel {}", free_number(&taken))
            } else {
                t.channel_name.to_string()
            };
            let number = free_number(&taken);
            // Suggestions for different templates do not collide with each other.
            taken.push(number);
            let mut used = ids.clone();
            used.extend(
                TEMPLATES
                    .iter()
                    .take_while(|o| o.id != t.id)
                    .map(|o| slug(if o.channel_name.is_empty() { o.name } else { o.channel_name })),
            );
            json!({
                "id": t.id, "group": t.group, "kind": t.kind, "name": t.name, "about": t.about,
                "needs": t.needs,
                "suggested": { "id": unused_id(&slug(&name), &used), "number": number, "name": name },
                "body": (t.body)(),
            })
        })
        .collect();
    Ok(Json(json!({
        "templates": list,
        "feeds": FEEDS.iter().map(|(n, u)| json!({ "name": n, "url": u })).collect::<Vec<_>>(),
        "slides": tvmaster_core::SlideKind::ALL.iter().map(|(id, name, about)| json!({ "id": id, "name": name, "about": about })).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
mod tests {
    use crate::test_util::call;
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn the_creator_offers_every_kind_with_a_free_number_and_a_name() {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let (st, v) = call(&app, "GET", "/api/v1/channel-templates", "").await;
        assert_eq!(st, StatusCode::OK);
        let t = v["templates"].as_array().unwrap();
        let kinds: Vec<&str> = t.iter().map(|x| x["kind"].as_str().unwrap()).collect();
        for k in ["programming", "guide", "info", "test"] {
            assert!(kinds.contains(&k), "{k}");
        }
        // Numbers are different from one another, and from what is taken.
        let nums: Vec<u64> = t
            .iter()
            .map(|x| x["suggested"]["number"].as_u64().unwrap())
            .collect();
        let mut uniq = nums.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), nums.len());
        assert_eq!(nums[0], 1);
        let ids: Vec<&str> = t
            .iter()
            .map(|x| x["suggested"]["id"].as_str().unwrap())
            .collect();
        assert!(ids.iter().all(|i| !i.is_empty()));
        assert!(v["feeds"].as_array().unwrap().len() >= 3);
        // Every template makes a valid channel once a number, id and name are given (and its feeds).
        for tpl in t {
            let mut body = tpl["body"].clone();
            body["id"] = tpl["suggested"]["id"].clone();
            body["number"] = tpl["suggested"]["number"].clone();
            body["name"] = tpl["suggested"]["name"].clone();
            if tpl["needs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n == "feeds")
            {
                body["info"]["feeds"] = serde_json::json!(["https://example.com/feed.xml"]);
            }
            if tpl["kind"] == "programming" {
                continue;
            }
            let (st, r) = call(&app, "POST", "/api/v1/channels", &body.to_string()).await;
            assert_eq!(st, StatusCode::CREATED, "{}: {r}", tpl["id"]);
        }
        // The guide lists the info channels beside the TV ones, each with what it is.
        let data = crate::plan::build_guide(&state.db(), "guide", crate::plan::now_ms()).unwrap();
        let titles: Vec<String> = data.rows.iter().map(|r| r.cells[0].title.clone()).collect();
        assert_eq!(data.rows.len(), 5, "{titles:?}");
        assert!(titles.contains(&"News and headlines".to_string()));
        assert!(titles.contains(&"What's on tonight".to_string()));
        assert!(titles.contains(&"Time and date".to_string()));
        assert!(data
            .rows
            .iter()
            .all(|r| r.cells.len() == 1 && r.cells[0].range.is_empty()));
        // A page that cannot work is refused with a reason.
        let (st, r) = call(&app, "POST", "/api/v1/channels", r#"{"id":"bad","number":99,"name":"Bad","kind":"info","info":{"slides":[{"kind":"headlines"}]}}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(r["error"].as_str().unwrap().contains("feed"), "{r}");
        let (st, r) = call(&app, "POST", "/api/v1/channels", r#"{"id":"bad","number":99,"name":"Bad","kind":"info","info":{"slides":[{"kind":"photo","picture":"/no/such.jpg"}]}}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(r["error"].as_str().unwrap().contains("picture"), "{r}");
        // Taken numbers are skipped next time.
        let (_, v) = call(&app, "GET", "/api/v1/channel-templates", "").await;
        let used: Vec<u64> = nums[1..].to_vec();
        for tpl in v["templates"].as_array().unwrap() {
            assert!(!used.contains(&tpl["suggested"]["number"].as_u64().unwrap()));
        }
        let first_id = v["templates"][1]["suggested"]["id"].as_str().unwrap();
        assert_ne!(first_id, "guide", "the guide already exists");
    }
}
