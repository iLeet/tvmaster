'use strict';
// TVMaster's HTTP + WebSocket API (docs/API.md). The server owns everything;
// this app only reads channels and schedules and plays the HLS streams.
const Api = (() => {
  const base = () => Cfg.get('server').replace(/\/+$/, '');
  const tok = () => Cfg.get('token');
  const withToken = (url) => (tok() ? url + (url.indexOf('?') < 0 ? '?' : '&') + 'token=' + encodeURIComponent(tok()) : url);

  async function get(path, asText) {
    const ctl = typeof AbortController !== 'undefined' ? new AbortController() : null;
    const timer = ctl ? setTimeout(() => ctl.abort(), 10000) : 0;
    try {
      const r = await fetch(withToken(base() + path), { cache: 'no-store', signal: ctl ? ctl.signal : undefined });
      if (!r.ok) throw new Error('HTTP ' + r.status + ' ' + path);
      return asText ? await r.text() : await r.json();
    } finally { clearTimeout(timer); }
  }
  const json = (path) => get(path, false);

  // XMLTV time: "20261120183805 +0000"
  function xmltvTime(s) {
    const m = /^(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})\s*(?:([+-])(\d{2})(\d{2}))?$/.exec((s || '').trim());
    if (!m) return NaN;
    const off = m[7] ? (parseInt(m[8], 10) * 60 + parseInt(m[9], 10)) * 60000 * (m[7] === '-' ? -1 : 1) : 0;
    return Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]) - off;
  }

  // The whole lineup's schedule in one request. /iptv/guide.xml is part of "open viewing", unlike the
  // per-channel schedule endpoint, and covers about 24 hours. Returns { channelId: [programme, ...] }.
  async function guide() {
    const doc = new DOMParser().parseFromString(await get('/iptv/guide.xml', true), 'application/xml');
    if (doc.getElementsByTagName('parsererror').length) throw new Error('guide.xml is not valid XML');
    const out = {};
    const text = (el, tag) => { const n = el.getElementsByTagName(tag)[0]; return n ? n.textContent.trim() : ''; };
    Array.prototype.forEach.call(doc.getElementsByTagName('programme'), (el) => {
      const start = xmltvTime(el.getAttribute('start'));
      const end = xmltvTime(el.getAttribute('stop'));
      const id = el.getAttribute('channel');
      if (!id || isNaN(start) || isNaN(end)) return;
      const show = text(el, 'title');
      const sub = text(el, 'sub-title');
      const se = /S(\d+)E(\d+)/i.exec(text(el, 'episode-num'));
      (out[id] = out[id] || []).push({ show, title: sub || show, season: se ? +se[1] : null, episode: se ? +se[2] : null,
        start_ms: start, end_ms: end, runtime_end_ms: end, label: null, desc: text(el, 'desc') });
    });
    return out;
  }

  let lastReport = null;
  function report(n) {
    lastReport = n;
    if (ws && ws.readyState === 1) ws.send(JSON.stringify({ type: 'tv', id: Cfg.get('tvId'), channel: n, power: true }));
  }
  let ws = null;
  let wsTimer = 0;
  let wsHandler = null;
  function connectWs(handler) {
    wsHandler = handler || wsHandler;
    clearTimeout(wsTimer);
    if (Cfg.get('demo') || typeof WebSocket === 'undefined') return;
    try {
      ws = new WebSocket(withToken(base().replace(/^http/, 'ws') + '/api/v1/ws'));
    } catch (e) { wsTimer = setTimeout(() => connectWs(), 5000); return; }
    ws.onopen = () => {
      // Show up under TVs in TVMaster: register (or rename) this TV, then say this socket is its player.
      const id = Cfg.get('tvId');
      fetch(withToken(base() + '/api/v1/tvs'), { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ id, name: Cfg.get('tvName') }) })
        .catch(() => { /* the hello below creates it anyway */ })
        .then(() => {
          if (ws && ws.readyState === 1) { ws.send(JSON.stringify({ type: 'hello', tv: id })); if (lastReport !== null) report(lastReport); }
        });
    };
    ws.onmessage = (m) => {
      try { const j = JSON.parse(m.data); if (wsHandler) wsHandler(j.type || j.event, j); } catch (e) { Log.e('ws', e); }
    };
    ws.onclose = () => { ws = null; wsTimer = setTimeout(() => connectWs(), 5000); };
    ws.onerror = () => { try { ws.close(); } catch (e) { /* closed */ } };
  }

  // Try an address without saving it: is there a TVMaster there?
  async function probe(url, token) {
    const ctl = typeof AbortController !== 'undefined' ? new AbortController() : null;
    const timer = ctl ? setTimeout(() => ctl.abort(), 6000) : 0;
    try {
      const u = url.replace(/\/+$/, '') + '/api/v1/health';
      const r = await fetch(token ? u + '?token=' + encodeURIComponent(token) : u, { cache: 'no-store', signal: ctl ? ctl.signal : undefined });
      if (!r.ok) throw new Error('HTTP ' + r.status);
      const j = await r.json();
      if (j.service !== 'tvmaster') throw new Error('not TVMaster');
      return j;
    } finally { clearTimeout(timer); }
  }

  function resetWs() {
    clearTimeout(wsTimer);
    if (ws) { ws.onclose = null; try { ws.close(); } catch (e) { /* closed */ } ws = null; }
    connectWs();
  }

  return {
    probe,
    resetWs,
    health: () => json('/api/v1/health'),
    channels: () => json('/api/v1/channels'),
    json,
    guide,
    streamUrl: (id) => withToken(base() + '/stream/' + encodeURIComponent(id) + '/index.m3u8'),
    connectWs,
    // Tell the server what this TV is watching, so TVs and the remote show it.
    report,
  };
})();
