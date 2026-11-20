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

const Theme = (() => {
  const LIST = ['classic', 'crimson', 'night'];
  const apply = () => { document.body.setAttribute('data-theme', Cfg.get('theme')); };
  return {
    init() { if (LIST.indexOf(Cfg.get('theme')) < 0) Cfg.set('theme', 'classic'); apply(); },
    next() {
      Cfg.set('theme', LIST[(LIST.indexOf(Cfg.get('theme')) + 1) % LIST.length]);
      apply();
      Toast.show('Theme: ' + Cfg.get('theme'));
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
