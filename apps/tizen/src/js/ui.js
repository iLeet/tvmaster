'use strict';
// Small screens: banner, channel-number entry, status card, toast, clock, themes, scaling.
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

const Screen = (() => {
  function fit() {
    const s = Math.min(window.innerWidth / 1920, window.innerHeight / 1080);
    const st = $('stage');
    st.style.transform = 'scale(' + s + ')';
    st.style.left = ((window.innerWidth - 1920 * s) / 2) + 'px';
    st.style.top = ((window.innerHeight - 1080 * s) / 2) + 'px';
  }
  return {
    init() {
      if (typeof webapis !== 'undefined' && webapis.avplay) document.body.classList.add('tv');
      fit();
      window.addEventListener('resize', fit);
    },
  };
})();

// Themes. "station" wears the look the TVMaster server has chosen (its palette, and its guide's layout);
// the others are the server's own list of palettes, or three built into the app for when it cannot be reached.
const Theme = (() => {
  const LOCAL = ['classic', 'crimson', 'night'];
  const VARS = ['--tint', '--panel-accent', '--ch-shadow', '--bg-top', '--bg-bot', '--panel', '--panel-edge', '--panel-ink', '--panel-muted', '--cell-a', '--cell-b', '--cell-edge', '--cell-now-a', '--cell-now-b',
    '--gap-a', '--gap-b', '--ch-a', '--ch-b', '--ch-num', '--ch-name', '--sel-a', '--sel-b', '--sel-edge', '--sel-text', '--text', '--muted', '--accent'];
  let remote = null; // { palettes, station: { theme, palette }, glass, cable }
  const rgb = (h) => { const n = parseInt(h.slice(1), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; };
  const hex = (c) => '#' + c.map((x) => ('0' + Math.max(0, Math.min(255, Math.round(x))).toString(16)).slice(-2)).join('');
  const mix = (a, b, t) => { const x = rgb(a); const y = rgb(b); return hex(x.map((v, i) => v + (y[i] - v) * t)); };
  const lum = (h) => { const c = rgb(h); return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]; };

  function paint(p) {
    const st = document.body.style;
    const on = (bg) => (lum(bg) > 150 ? '#1c2038' : '#ffffff');
    const set = {
      '--bg-top': p.bg_top, '--bg-bot': p.bg_bottom, '--panel': p.panel, '--panel-edge': p.rule,
      '--panel-ink': on(p.panel), '--panel-muted': lum(p.panel) > 150 ? '#4a5170' : p.header_sub,
      '--cell-a': mix(p.cell, '#ffffff', 0.08), '--cell-b': p.cell_alt, '--cell-edge': mix(p.cell_alt, '#000000', 0.45),
      '--cell-now-a': mix(p.cell_now, '#ffffff', 0.15), '--cell-now-b': p.cell_now,
      '--gap-a': p.empty, '--gap-b': mix(p.empty, '#000000', 0.25),
      '--ch-a': mix(p.label_bg, '#ffffff', 0.06), '--ch-b': p.label_bg, '--ch-num': p.label_num, '--ch-name': p.label_name,
      '--sel-a': mix(p.accent, '#ffffff', 0.55), '--sel-b': p.accent, '--sel-edge': '#ffffff', '--sel-text': on(p.accent),
      '--text': p.text, '--muted': mix(p.text, p.bg_top, 0.4), '--accent': p.accent,
      '--tint': p.tick, '--panel-accent': lum(p.panel) > 150 ? mix(p.accent, '#000000', 0.45) : p.accent,
      '--ch-shadow': lum(p.label_bg) > 150 ? 'none' : '2px 2px 0 #000',
    };
    Object.keys(set).forEach((k) => st.setProperty(k, set[k]));
  }

  function unpaint() { VARS.forEach((k) => document.body.style.removeProperty(k)); }

  const ids = () => ['station'].concat(LOCAL, remote ? Object.keys(remote.palettes) : []);
  const current = () => (ids().indexOf(Cfg.get('theme')) < 0 ? 'station' : Cfg.get('theme'));

  // The guide's layout: the server's guide channel decides (frosted glass, or the picture top right with the info card at the left, like a cable box); with no guide channel the theme does.
  function layout() {
    const id = current();
    const name = id === 'station' ? (remote && remote.station.theme) : id;
    if (remote && id === 'station') { if (remote.glass) return 'glass'; if (remote.cable) return 'cable'; }
    if (name === 'glass') return 'glass';
    if (name === 'cable') return 'cable';
    return 'default';
  }

  function apply() {
    const id = current();
    unpaint();
    const pal = id === 'station' ? (remote && remote.station.palette) : (remote && remote.palettes[id]);
    document.body.setAttribute('data-theme', pal ? 'palette' : (LOCAL.indexOf(id) >= 0 ? id : 'classic'));
    if (pal) paint(pal);
    document.body.setAttribute('data-layout', layout());
    if (typeof Guide !== 'undefined' && Guide.relayout) Guide.relayout();
  }

  return {
    init() { apply(); },
    layout,
    // Ask the server for its palettes and what its guide looks like; fine if it cannot be reached.
    async load() {
      try {
        const t = await Api.json('/api/v1/theme/palettes');
        let glass = false;
        let cable = false;
        try {
          const chs = await Api.json('/api/v1/channels');
          const g = (chs || []).filter((c) => c.kind === 'guide')[0];
          if (g && g.guide) { glass = !!(g.guide.look && g.guide.look.glass); cable = !!(g.guide.pip && g.guide.pip.placement === 'top_right' && g.guide.pip.info !== false); }
        } catch (e) { /* the list needs no more than this */ }
        remote = { palettes: t.palettes || {}, station: t.station, glass, cable };
        apply();
      } catch (e) { Log.d('theme: ' + e); }
    },
    next() {
      const list = ids();
      Cfg.set('theme', list[(list.indexOf(current()) + 1) % list.length]);
      apply();
      const id = current();
      Toast.show('Theme: ' + (id === 'station' ? 'the station\'s' : id));
    },
  };
})();

const Toast = (() => {
  let t = 0;
  return {
    show(text, ms) {
      const el = $('toast');
      el.textContent = text;
      el.classList.remove('hide');
      clearTimeout(t);
      t = setTimeout(() => el.classList.add('hide'), ms || 2200);
    },
  };
})();

const Status = (() => ({
  show(title, text) { $('st-title').textContent = title; $('st-text').textContent = text || ''; $('status').classList.remove('hide'); },
  hide() { $('status').classList.add('hide'); },
}))();

const Clock = (() => {
  function tick() {
    const s = Fmt.time(Date.now());
    $('b-clock').textContent = s;
    $('gi-clock').textContent = Fmt.day(Date.now()) + '   ' + s;
  }
  return { init() { tick(); setInterval(tick, 10000); } };
})();

const programLabel = (p) => (p ? (p.show && p.title && p.show !== p.title ? p.show + ': ' + p.title : (p.show || p.title || '')) : '');

const Banner = (() => {
  let timer = 0;
  let ch = null;
  const el = () => $('banner');

  function fill() {
    if (!ch) return;
    const nn = Store.nowNext(ch);
    const now = nn.now;
    $('b-num').textContent = ch.number;
    $('b-name').textContent = ch.name;
    $('b-title').textContent = now ? programLabel(now) : (ch.kind === 'info' ? 'Information channel' : (ch.kind === 'test' ? 'Test pattern' : (ch.kind === 'guide' ? 'Guide channel' : 'No program information')));
    $('b-time').textContent = now ? Fmt.range(now.start_ms, now.end_ms) : '';
    const pct = now ? Math.max(0, Math.min(100, ((Date.now() - now.start_ms) / (now.end_ms - now.start_ms)) * 100)) : 0;
    $('b-bar').style.width = pct + '%';
    $('b-next').textContent = nn.next ? ' ' + Fmt.time(nn.next.start_ms) + '  ' + programLabel(nn.next) : '';
    $('b-next').parentNode.style.visibility = nn.next ? 'visible' : 'hidden';
  }

  return {
    show(c, ms) {
      ch = c;
      fill();
      el().classList.remove('hide');
      clearTimeout(timer);
      timer = setTimeout(() => el().classList.add('hide'), ms || 4500);
      Store.ensureSchedule(c.id).then(() => { if (ch === c && Banner.visible()) fill(); });
    },
    refresh() { if (Banner.visible()) fill(); },
    hide() { clearTimeout(timer); el().classList.add('hide'); },
    visible: () => !el().classList.contains('hide'),
  };
})();

const Numpad = (() => {
  let buf = '';
  let timer = 0;
  let commit = () => {};
  const el = () => $('numpad');
  function end() { clearTimeout(timer); const n = buf ? parseInt(buf, 10) : null; buf = ''; el().classList.add('hide'); return n; }
  return {
    digit(d, onCommit) {
      commit = onCommit;
      if (buf.length >= 4) buf = '';
      buf += String(d);
      el().textContent = buf;
      el().classList.remove('hide');
      clearTimeout(timer);
      timer = setTimeout(() => { const n = end(); if (n !== null) commit(n); }, 1600);
    },
    active: () => buf.length > 0,
    commitNow() { const n = end(); if (n !== null) commit(n); },
    cancel() { end(); },
  };
})();
