'use strict';
// Settings screen (Red key): the TVMaster server address and, if the server needs one, its access token.
// On a Samsung TV, OK on a field opens the on-screen keyboard.
const Settings = (() => {
  const IDS = ['set-server', 'set-token', 'set-save', 'set-reset', 'set-cancel'];
  let isOpen = false;
  let idx = 0;
  let busy = false;
  let onChanged = () => {};

  const el = (id) => $(id);
  const msg = (t, bad) => { el('set-msg').textContent = t || ''; el('set-msg').classList.toggle('bad', !!bad); };

  function focus(i) {
    idx = Math.max(0, Math.min(IDS.length - 1, i));
    IDS.forEach((id, n) => el(id).classList.toggle('on', n === idx));
    const e = el(IDS[idx]);
    if (idx < 2) e.focus(); else { try { document.activeElement.blur(); } catch (x) { /* nothing focused */ } }
  }

  // "10.0.0.251" -> http://10.0.0.251:8787 ; "host:9000" -> http://host:9000 ; full URLs are kept.
  function normalize(s) {
    s = (s || '').trim();
    if (!s) return '';
    if (!/^https?:\/\//i.test(s)) s = 'http://' + s;
    const m = s.match(/^(https?:\/\/)([^\/:]+)(:\d+)?(\/.*)?$/i);
    if (!m) return s.replace(/\/+$/, '');
    const port = m[3] || (/^http:/i.test(m[1]) ? ':8787' : '');
    return (m[1] + m[2] + port).toLowerCase();
  }

  function open() {
    isOpen = true;
    busy = false;
    el('set-server').value = Cfg.get('server');
    el('set-token').value = Cfg.get('token') || '';
    msg('Type the address of your TVMaster server, e.g. 10.0.0.251');
    el('settings').classList.remove('hide');
    focus(0);
  }

  function close() {
    isOpen = false;
    try { document.activeElement.blur(); } catch (x) { /* nothing focused */ }
    el('settings').classList.add('hide');
  }

  async function save() {
    if (busy) return;
    const url = normalize(el('set-server').value);
    const token = el('set-token').value.trim();
    if (!url) { msg('Enter an address first.', true); return; }
    busy = true;
    msg('Connecting to ' + url + ' ...');
    try {
      const j = await Api.probe(url, token);
      Cfg.setServer(url, token);
      busy = false;
      close();
      Toast.show('Connected to TVMaster ' + (j.version || ''));
      onChanged();
    } catch (e) {
      busy = false;
      msg('Could not reach TVMaster at ' + url + ' (' + (e && e.message ? e.message : e) + ').', true);
    }
  }

  function reset() {
    Cfg.resetServer();
    close();
    Toast.show('Using ' + Cfg.builtIn());
    onChanged();
  }

  // Returns false to let the key through to the text field (typing, caret, on-screen keyboard).
  function key(k) {
    const inField = idx < 2;
    switch (k) {
      case 'UP': focus(idx - 1); return true;
      case 'DOWN': focus(idx + 1); return true;
      case 'ENTER':
        if (inField) return false;
        if (idx === 2) save(); else if (idx === 3) reset(); else close();
        return true;
      case 'BACK': if (!busy) close(); return true;
      default: return !inField; // digits, letters, left/right go to the text field
    }
  }

  return { init(cb) { onChanged = cb; }, open, close, key, isOpen: () => isOpen };
})();
