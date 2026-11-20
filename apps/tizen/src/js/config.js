'use strict';
// Settings.
// DEFAULT_SERVER is the address built into the app (scripts/build-install.sh --server URL rewrites it).
// A server address typed on the TV (Red key) is remembered, but only for the build it was typed into:
// if the app is rebuilt with a different built-in address, the built-in address wins again.
const Cfg = (() => {
  const DEFAULT_SERVER = 'http://10.0.0.251:8787'; /*SERVER*/
  const v = { server: DEFAULT_SERVER, token: '', theme: 'classic', demo: false, lastChannel: null, showGuideChannels: true };
  let override = null; // { url, token, base }
  try {
    const o = JSON.parse(localStorage.getItem('tvg.cfg') || '{}');
    ['theme', 'demo', 'lastChannel'].forEach((k) => { if (o[k] !== undefined) v[k] = o[k]; });
    if (o.override && o.override.base === DEFAULT_SERVER && o.override.url) {
      override = o.override;
      v.server = override.url;
      v.token = override.token || '';
    }
  } catch (e) { /* nothing stored yet */ }
  try { // ?server= ?token= ?theme= ?demo=1 : handy when trying the app in a desktop browser, never saved
    const q = new URLSearchParams(location.search);
    if (q.get('server')) v.server = q.get('server');
    if (q.get('token')) v.token = q.get('token');
    if (q.get('theme')) v.theme = q.get('theme');
    if (q.has('demo')) v.demo = q.get('demo') !== '0';
  } catch (e) { /* no query string */ }

  const save = () => {
    try {
      localStorage.setItem('tvg.cfg', JSON.stringify({ theme: v.theme, demo: v.demo, lastChannel: v.lastChannel, override }));
    } catch (e) { /* storage unavailable */ }
  };
  return {
    get: (k) => v[k],
    set(k, val) { v[k] = val; save(); },
    builtIn: () => DEFAULT_SERVER,
    setServer(url, token) { override = { url, token: token || '', base: DEFAULT_SERVER }; v.server = url; v.token = token || ''; save(); },
    resetServer() { override = null; v.server = DEFAULT_SERVER; v.token = ''; save(); },
  };
})();
