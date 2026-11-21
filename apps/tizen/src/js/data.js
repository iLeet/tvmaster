'use strict';
// Channel list + schedules, kept fresh.
//   channel list   every 30 s (new/removed channels, on-air state), 5 s while the server is unreachable
//   now / next     pushed over the WebSocket the moment a programme changes
//   the schedule   the whole lineup from /iptv/guide.xml in one request, when first needed, again after
//                  10 min, and at once when the server says a schedule changed
const Store = (() => {
  const CHANNEL_POLL = 30000;
  const OFFLINE_POLL = 5000;
  const SCHEDULE_TTL = 10 * 60000;
  let channels = [];
  let online = null; // null = unknown yet
  const sched = new Map(); // id -> { at, items }
  const inflight = new Map();
  const subs = new Set();
  let pollTimer = 0;

  const emit = (ev, arg) => subs.forEach((f) => { try { f(ev, arg); } catch (e) { Log.e(e); } });

  function normalize(c) {
    return { id: c.id, number: c.number, name: c.name || c.id, kind: c.kind || 'programming', group: c.group || '',
      on_air: !!c.on_air, now: c.now || null, next: c.next || null };
  }

  async function loadChannels() {
    clearTimeout(pollTimer);
    try {
      const raw = Cfg.get('demo') ? Demo.channels() : await Api.channels();
      const list = raw.map(normalize)
        .filter((c) => Cfg.get('showGuideChannels') || c.kind !== 'guide')
        .sort((a, b) => a.number - b.number);
      const changed = JSON.stringify(list.map((c) => [c.id, c.number, c.name, c.kind])) !==
        JSON.stringify(channels.map((c) => [c.id, c.number, c.name, c.kind]));
      const was = online;
      channels = list;
      online = true;
      if (changed || was !== true) { emit('channels'); loadGuide(changed && was === true); }
      else emit('nownext');
    } catch (e) {
      Log.e('channels', e);
      if (online !== false) { online = false; emit('offline'); }
    }
    pollTimer = setTimeout(loadChannels, online ? CHANNEL_POLL : OFFLINE_POLL);
  }

  let guideAt = 0;
  let guideJob = null;
  const guideFresh = () => guideAt && Date.now() - guideAt < SCHEDULE_TTL && channels.every((c) => sched.has(c.id));

  function loadGuide(force) {
    if (!force && guideFresh()) return Promise.resolve();
    if (guideJob) return guideJob;
    guideJob = (async () => {
      try {
        let map;
        if (Cfg.get('demo')) { map = {}; channels.forEach((c) => { map[c.id] = Demo.schedule(c.id, 8); }); }
        else map = await Api.guide();
        channels.forEach((c) => {
          sched.set(c.id, { items: (map[c.id] || []).sort((a, b) => a.start_ms - b.start_ms) });
        });
        guideAt = Date.now();
      } catch (e) {
        Log.e('guide', e);
        channels.forEach((c) => { if (!sched.has(c.id)) sched.set(c.id, { items: [] }); });
        guideAt = Date.now() - SCHEDULE_TTL + 15000; // try again in 15 s
      } finally { guideJob = null; }
      emit('schedule', null); // null = every channel
    })();
    return guideJob;
  }

  const ensureSchedule = (id, force) => loadGuide(force).then(() => (sched.has(id) ? sched.get(id).items : []));

  function onSocket(type, m) {
    if (type === 'channel.now') {
      const c = channels.find((x) => x.id === m.channel);
      if (c) { c.now = m.now || null; c.next = m.next || null; emit('nownext'); }
    } else if (type === 'schedule.changed') {
      loadGuide(true);
    } else if (type === 'tv.state' && m.tv && m.tv.id === Cfg.get('tvId')) {
      emit('tv', m.tv);
    }
  }

  return {
    on(f) { subs.add(f); },
    start() { Api.connectWs(onSocket); return loadChannels(); },
    reload() { return loadChannels(); },
    channels: () => channels,
    online: () => online,
    byId: (id) => channels.find((c) => c.id === id) || null,
    byNumber: (n) => channels.find((c) => c.number === n) || null,
    ensureSchedule,
    // What is on now and next for a channel: the schedule when loaded, else the list's own now/next.
    nowNext(ch, t) {
      t = t || Date.now();
      const items = sched.has(ch.id) ? sched.get(ch.id).items : null;
      if (items && items.length) {
        const i = items.findIndex((p) => p.start_ms <= t && t < p.end_ms);
        if (i >= 0) return { now: items[i], next: items[i + 1] || null };
        return { now: null, next: items.find((p) => p.start_ms > t) || null };
      }
      return { now: ch.now, next: ch.next };
    },
    scheduleOf: (id) => (sched.has(id) ? sched.get(id).items : null),
    loaded: (id) => sched.has(id),
    clearSchedules() { sched.clear(); guideAt = 0; },
  };
})();
