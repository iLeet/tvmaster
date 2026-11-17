//! Headlines for the ticker: news feeds (RSS or Atom) fetched in the background every so often, so
//! playout only ever reads what is already here.

use crate::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const REFRESH_MS: i64 = 15 * 60_000;
const PER_FEED: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub fetched_ms: i64,
    /// The feed addresses these were fetched for (a change of feeds fetches again).
    pub feeds: Vec<String>,
    pub items: Vec<String>,
}

pub type Book = Arc<Mutex<HashMap<String, Entry>>>;

fn entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match tail.find(';').filter(|j| *j <= 8) {
            Some(j) => {
                let name = &tail[1..j];
                let rep = match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16)
                        .ok()
                        .and_then(char::from_u32),
                    n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32),
                    _ => None,
                };
                match rep {
                    Some(c) => {
                        out.push(c);
                        rest = &tail[j + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &tail[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// The titles of the items (RSS) or entries (Atom) in a feed, newest first as the feed lists them.
pub fn parse_feed(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = xml.to_lowercase();
    let mut at = 0;
    while out.len() < PER_FEED {
        // The next item or entry (the feed's own title comes before the first one and is skipped).
        let next = [lower[at..].find("<item"), lower[at..].find("<entry")]
            .into_iter()
            .flatten()
            .min();
        let Some(i) = next else { break };
        let start = at + i;
        let end = lower[start..]
            .find("</item>")
            .or_else(|| lower[start..].find("</entry>"))
            .map_or(lower.len(), |e| start + e);
        let block = &xml[start..end];
        let lb = &lower[start..end];
        if let (Some(a), Some(b)) = (lb.find("<title"), lb.find("</title>")) {
            if let Some(gt) = lb[a..b].find('>') {
                let raw = block[a + gt + 1..b].trim();
                let raw = raw
                    .strip_prefix("<![CDATA[")
                    .and_then(|r| r.strip_suffix("]]>"))
                    .unwrap_or(raw);
                let t = entities(&strip_tags(raw))
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                if !t.is_empty() {
                    out.push(t.chars().take(160).collect());
                }
            }
        }
        at = end.max(start + 5);
    }
    out
}

fn fetch(url: &str) -> Option<String> {
    let out = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "10",
            "--max-filesize",
            "3000000",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "-A",
            "TVMaster",
            "--",
            url,
        ])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// Fetch the feeds of every channel whose ticker follows some, when they are old or have changed.
pub fn refresh(s: &AppState) {
    let now = crate::plan::now_ms();
    let wanted: Vec<(String, Vec<String>)> = tvmaster_db::list_channels(&s.db())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            // An info channel follows its own feeds; any other channel, the ones its ticker names.
            if c.kind == tvmaster_core::ChannelKind::Info {
                return (!c.info.feeds.is_empty()).then_some((c.id, c.info.feeds));
            }
            (c.options.overlay.ticker.enabled && !c.options.overlay.ticker.feeds.is_empty())
                .then_some((c.id, c.options.overlay.ticker.feeds))
        })
        .collect();
    for (id, feeds) in wanted {
        let due = s
            .headlines
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .is_none_or(|e| e.feeds != feeds || now - e.fetched_ms >= REFRESH_MS);
        if !due {
            continue;
        }
        let mut items = Vec::new();
        for f in &feeds {
            if let Some(xml) = fetch(f) {
                items.extend(parse_feed(&xml));
            }
        }
        let mut book = s.headlines.lock().unwrap_or_else(|p| p.into_inner());
        // A feed that could not be reached keeps what was there rather than leaving the ticker empty.
        let keep = items.is_empty() && book.get(&id).is_some_and(|e| e.feeds == feeds);
        let e = book.entry(id).or_default();
        e.fetched_ms = now;
        e.feeds = feeds;
        if !keep {
            e.items = items;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_come_out_of_rss_and_atom_feeds_with_entities_and_cdata_handled() {
        let rss = r#"<?xml version="1.0"?><rss><channel><title>The Daily Paper</title>
            <item><title>Town &amp; country: pie contest</title><link>x</link></item>
            <item><title><![CDATA[Mayor <b>opens</b> bridge]]></title></item>
            <item><title>  Spaces   everywhere  </title></item>
            <item><link>no title here</link></item></channel></rss>"#;
        assert_eq!(
            parse_feed(rss),
            [
                "Town & country: pie contest",
                "Mayor opens bridge",
                "Spaces everywhere"
            ]
        );
        let atom = r#"<feed><title>Blog</title><entry><title type="html">First &#38; best</title></entry><entry><title>Second</title></entry></feed>"#;
        assert_eq!(parse_feed(atom), ["First & best", "Second"]);
        assert!(parse_feed("not a feed at all").is_empty());
        assert!(parse_feed("<rss><channel><title>Only a title</title></channel></rss>").is_empty());
        let many: String = (0..30)
            .map(|i| format!("<item><title>n{i}</title></item>"))
            .collect();
        assert_eq!(parse_feed(&many).len(), PER_FEED);
    }

    #[test]
    fn entities_are_decoded_and_odd_ampersands_survive() {
        assert_eq!(
            entities("a &lt;b&gt; &quot;c&quot; &#x41; &unknown; AT&T & co"),
            "a <b> \"c\" A &unknown; AT&T & co"
        );
    }
}
