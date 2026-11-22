'use strict';
// The interactive guide: channel names down the left, two hours of programmes across,
// the live picture in a window top-left, details of the highlighted programme beside it.
const Guide = (() => {
  const MIN = Fmt.MIN;
  const G = { rowH: 66, rows: 7, chW: 240, gridW: 1512, minutes: 120, gap: 4, ahead: 7.5 * 60 * MIN };
  const PPM = G.gridW / (G.minutes * MIN); // pixels per millisecond
  const PIP_LEFT = { x: 80, y: 80, w: 640, h: 360 };
  const PIP_RIGHT = { x: 1200, y: 80, w: 640, h: 360 };
  // Cable box and glass layouts put the picture at the top right and the info card at the left.
  const pip = () => (Theme.layout() === 'default' ? PIP_LEFT : PIP_RIGHT);
  function relayout() {
    const r = pip();
    $('guide').querySelector('.g-bg').style.clipPath = 'path(evenodd, "M0 0H1920V1080H0Z M' + r.x + ' ' + r.y + 'H' + (r.x + r.w) + 'V' + (r.y + r.h) + 'H' + r.x + 'Z")';
    if (isOpen) Player.setRect(r);
  }

  let isOpen = false;
  let chs = [];
  let lo = 0; // earliest time reachable (the half hour we are in)
  let hi = 0; // latest
  let winStart = 0;
  let rowOff = 0;
  let selRow = 0;
  let sel = null; // the highlighted cell
  const keys = []; // per row: what is drawn, so unchanged rows are left alone
  const host = { current: () => null, watch: () => {} };

  const rowsEl = () => $('g-rows');
  const placeholder = (ch) => (ch.kind === 'info' ? 'Information channel' : ch.kind === 'test' ? 'Test pattern' : ch.kind === 'guide' ? 'Guide channel' : 'No program information');

  // ---- cells -------------------------------------------------------------------------------
  function cellsFor(ch, from, to) {
    const items = Store.scheduleOf(ch.id);
    if (!items) return [{ start: from, end: to, title: 'Loading...', loading: true }];
    if (!items.length) return [{ start: from, end: to, title: placeholder(ch), gap: true }];
    const out = [];
    let t = from;
    for (const p of items) {
      if (p.end_ms <= t) continue;
      if (p.start_ms >= to) break;
      // a blank stretch that is already over is not shown; only one that is still to come or running now
      if (p.start_ms > t && p.start_ms > Date.now()) out.push({ start: t, end: p.start_ms, title: placeholder(ch), gap: true });
      out.push({ start: p.start_ms, end: p.end_ms, prog: p, title: p.show || p.title || '' });
      t = p.end_ms;
    }
    if (t < to) out.push({ start: t, end: to, title: placeholder(ch), gap: true });
    return out;
  }
  const cellAt = (ch, t) => cellsFor(ch, lo, hi).find((c) => c.start <= t && t < c.end) || null;

  // ---- drawing -----------------------------------------------------------------------------
  function build() {
    keys.length = 0;
    rowsEl().innerHTML = chs.map((c, i) =>
      '<div class="row" style="top:' + (i * G.rowH) + 'px"><div class="chl"><b>' + c.number + '</b><span>' + esc(c.name) +
      '</span></div><div class="cells" id="cells' + i + '"></div></div>').join('');
  }

  function rowKey(i) {
    const ch = chs[i];
    return [winStart, Store.loaded(ch.id) ? 1 : 0, Math.floor(Date.now() / MIN), i === selRow && sel ? sel.start : '-'].join('|');
  }

  function renderRow(i) {
    const ch = chs[i];
    const winEnd = winStart + G.minutes * MIN;
    const now = Date.now();
    const out = cellsFor(ch, winStart, winEnd).map((c) => {
      const l = (Math.max(c.start, winStart) - winStart) * PPM;
      const r = (Math.min(c.end, winEnd) - winStart) * PPM;
      const w = Math.max(8, r - l - G.gap);
      const cls = ['cell'];
      if (c.gap || c.loading) cls.push('gap');
      if (c.start <= now && now < c.end) cls.push('now');
      if (i === selRow && sel && c.start === sel.start) cls.push('sel');
      const left = c.start < winStart ? '<i class="ar">&#9664;</i>' : '';
      const right = c.end > winEnd ? '<i class="ar r">&#9654;</i>' : '';
      return '<div class="' + cls.join(' ') + '" style="left:' + l + 'px;width:' + w + 'px">' + left +
        '<span>' + esc(c.title) + '</span>' + right + '</div>';
    });
    $('cells' + i).innerHTML = out.join('');
    keys[i] = rowKey(i);
  }

  function renderRows() {
    const from = Math.max(0, rowOff - 2);
    const to = Math.min(chs.length - 1, rowOff + G.rows + 2);
    for (let i = from; i <= to; i++) {
      Store.ensureSchedule(chs[i].id);
      if (keys[i] !== rowKey(i)) renderRow(i);
    }
    rowsEl().style.transform = 'translateY(' + (-rowOff * G.rowH) + 'px)';
  }

  function renderTime() {
    let h = '<div class="tday">' + esc(Fmt.day(winStart)) + '</div>';
    for (let t = winStart; t < winStart + G.minutes * MIN; t += 30 * MIN) {
      h += '<div class="tm" style="left:' + (G.chW + 8 + (t - winStart) * PPM) + 'px">' + esc(Fmt.time(t)) + '</div>';
    }
    $('g-timebar').innerHTML = h;
  }

  function renderInfo() {
    const ch = chs[selRow];
    if (!ch || !sel) return;
    const p = sel.prog;
    $('gi-ch').innerHTML = '<b>' + ch.number + '</b> ' + esc(ch.name);
    $('gi-title').textContent = p ? (p.show || p.title) : sel.title;
    const ep = p && p.show && p.title && p.title !== p.show ? p.title : '';
    const se = p && p.season != null && p.episode != null ? 'S' + p.season + ' E' + p.episode : '';
    $('gi-sub').textContent = [ep, se].filter(Boolean).join('   ') || (p && p.desc) || '';
    $('gi-time').textContent = p ? Fmt.range(p.start_ms, p.end_ms) : '';
    const now = Date.now();
    $('gi-tag').textContent = !p ? '' : (p.start_ms <= now && now < p.end_ms ? 'ON NOW' : (p.start_ms > now ? 'LATER' : ''));
    $('gi-tag').style.visibility = p ? 'visible' : 'hidden';
  }

  function refresh() { renderTime(); renderRows(); renderInfo(); }

  // ---- movement ----------------------------------------------------------------------------
  function scrollToRow() {
    if (selRow < rowOff) rowOff = selRow;
    if (selRow >= rowOff + G.rows) rowOff = selRow - G.rows + 1;
    rowOff = Math.max(0, Math.min(rowOff, Math.max(0, chs.length - G.rows)));
  }

  function fitWindow() {
    const before = winStart;
    if (sel.start < winStart) winStart = Math.max(lo, Fmt.floorHalfHour(sel.start));
    while (sel.start >= winStart + (G.minutes - 20) * MIN && winStart + 30 * MIN <= hi) winStart += 30 * MIN;
    return winStart !== before;
  }

  function setRow(r) {
    r = Math.max(0, Math.min(chs.length - 1, r));
    if (r === selRow) return;
    const anchor = Math.max(sel.start, winStart);
    const old = selRow;
    selRow = r;
    sel = cellAt(chs[r], anchor) || cellAt(chs[r], Math.max(anchor, Date.now())) ||
      { start: anchor, end: anchor + 30 * MIN, title: placeholder(chs[r]), gap: true };
    keys[old] = ''; // redraw the row we left so its highlight goes
    scrollToRow();
    fitWindow();
    refresh();
  }

  function moveH(dir) {
    const ch = chs[selRow];
    let c = null;
    if (dir < 0) { if (sel.start <= lo) return; c = cellAt(ch, sel.start - 1); }
    else { if (sel.end >= hi) return; c = cellAt(ch, sel.end); }
    if (!c) return;
    sel = c;
    keys[selRow] = '';
    fitWindow();
    refresh();
  }

  function goToNumber(n) {
    const i = chs.findIndex((c) => c.number === n);
    if (i < 0) { Toast.show('Channel ' + n + ' is not available'); return; }
    setRow(i);
  }

  function watch() {
    const ch = chs[selRow];
    close();
    if (ch) host.watch(ch);
  }

  // ---- open / close ------------------------------------------------------------------------
  function open() {
    chs = Store.channels();
    if (!chs.length) return false;
    const now = Date.now();
    lo = Fmt.floorHalfHour(now);
    hi = lo + G.ahead;
    winStart = lo;
    const cur = host.current();
    selRow = Math.max(0, chs.findIndex((c) => c.id === cur));
    build();
    // Draw once with whatever schedule we have, then again as schedules arrive.
    sel = cellAt(chs[selRow], now) || { start: lo, end: lo + 30 * MIN, title: placeholder(chs[selRow]), gap: true };
    rowOff = 0;
    scrollToRow();
    rowOff = Math.max(0, Math.min(selRow - 3, Math.max(0, chs.length - G.rows)));
    isOpen = true;
    Banner.hide();
    relayout();
    $('guide').classList.remove('hide');
    refresh();
    return true;
  }

  function close() {
    if (!isOpen) return;
    isOpen = false;
    Numpad.cancel();
    $('guide').classList.add('hide');
    Player.setRect(Player.FULL);
  }

  // A schedule arrived or the channel list changed while the guide is up.
  function onStore(ev, arg) {
    if (!isOpen) return;
    if (ev === 'channels') {
      const id = chs[selRow] ? chs[selRow].id : null;
      chs = Store.channels();
      if (!chs.length) { close(); return; }
      build();
      selRow = Math.max(0, chs.findIndex((c) => c.id === id));
      sel = cellAt(chs[selRow], Math.max(sel.start, winStart)) || sel;
      scrollToRow();
      refresh();
    } else if (ev === 'schedule') {
      // arg is a channel id, or null when the whole lineup was reloaded
      for (let i = 0; i < chs.length; i++) if (arg == null || chs[i].id === arg) keys[i] = '';
      sel = cellAt(chs[selRow], Math.max(sel.start, winStart)) || cellAt(chs[selRow], Date.now()) || sel;
      refresh();
    }
  }

  function key(k, l) {
    switch (k) {
      case 'LEFT': moveH(-1); break;
      case 'RIGHT': moveH(1); break;
      case 'UP': setRow(selRow - 1); break;
      case 'DOWN': setRow(selRow + 1); break;
      case 'CH_UP': setRow(selRow - G.rows); break;
      case 'CH_DOWN': setRow(selRow + G.rows); break;
      case 'ENTER': if (Numpad.active()) Numpad.commitNow(); else watch(); break;
      case 'DIGIT': Numpad.digit(l.d, goToNumber); break;
      case 'GREEN': Theme.next(); break;
      case 'YELLOW': Log.toggle(); break;
      case 'BACK': case 'GUIDE': case 'INFO': case 'CH_LIST': if (Numpad.active()) Numpad.cancel(); else close(); break;
      default: break;
    }
  }

  return {
    init(h) { Object.assign(host, h); Store.on(onStore); },
    open, close, key, relayout,
    isOpen: () => isOpen,
  };
})();
