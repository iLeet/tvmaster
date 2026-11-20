'use strict';
// Ties it together: tuning, channel surfing, number entry and the remote's keys.
const App = (() => {
  let cur = null;     // channel id selected (may still be waiting out the surf delay)
  let playing = null; // channel id actually on the player
  let prev = null;    // the channel before this one (for the Left key)
  let tuneTimer = 0;
  const SURF_DELAY = 300; // ms to wait while CH+/CH- is pressed repeatedly, so we do not tune every channel passed

  function tune(ch, immediate) {
    if (!ch) return;
    if (cur && cur !== ch.id) prev = cur;
    cur = ch.id;
    Cfg.set('lastChannel', ch.id);
    Banner.show(ch);
    clearTimeout(tuneTimer);
    const go = () => {
      if (playing === ch.id) return;
      playing = ch.id;
      Player.tune(ch);
    };
    if (immediate) go(); else tuneTimer = setTimeout(go, SURF_DELAY);
  }

  function surf(dir) {
    const list = Store.channels();
    if (!list.length) return;
    let i = list.findIndex((c) => c.id === cur);
    i = i < 0 ? 0 : (i + dir + list.length) % list.length;
    tune(list[i], false);
  }

  function goToNumber(n) {
    const ch = Store.byNumber(n);
    if (!ch) { Toast.show('Channel ' + n + ' is not available'); return; }
    tune(ch, true);
  }

  function lastChannel() {
    const ch = prev && Store.byId(prev);
    if (ch) tune(ch, true);
  }

  function exitApp() {
    try { tizen.application.getCurrentApplication().exit(); } catch (e) { Log.d('exit unavailable here'); }
  }

  // The server address changed (Settings): start over against the new server.
  function reconnect() {
    Store.clearSchedules();
    Player.stop();
    cur = null;
    playing = null;
    prev = null;
    Status.show('TVMaster Guide', 'Connecting to ' + Cfg.get('server') + ' ...');
    Api.resetWs();
    Store.reload();
  }

  function onKey(k, l) {
    if (Settings.isOpen()) return Settings.key(k, l);
    if (k === 'RED') { Guide.close(); Numpad.cancel(); Settings.open(); return true; }
    if (Guide.isOpen()) { Guide.key(k, l); return true; }
    switch (k) {
      case 'DIGIT': Numpad.digit(l.d, goToNumber); break;
      case 'CH_UP': case 'UP': surf(1); break;
      case 'CH_DOWN': case 'DOWN': surf(-1); break;
      case 'ENTER': if (Numpad.active()) Numpad.commitNow(); else openGuide(); break;
      case 'GUIDE': case 'CH_LIST': openGuide(); break;
      case 'INFO': case 'RIGHT': { const ch = Store.byId(cur); if (ch) Banner.show(ch, 6000); break; }
      case 'LEFT': lastChannel(); break;
      case 'GREEN': Theme.next(); break;
      case 'YELLOW': Log.toggle(); break;
      case 'BLUE': toggleDemo(); break;
      case 'BACK': if (Numpad.active()) Numpad.cancel(); else if (Banner.visible()) Banner.hide(); else exitApp(); break;
      default: break;
    }
  }

  function openGuide() {
    if (!Guide.open()) Toast.show('No channels to show');
  }

  function toggleDemo() {
    Cfg.set('demo', !Cfg.get('demo'));
    Store.clearSchedules();
    playing = null;
    cur = null;
    Player.stop();
    Toast.show(Cfg.get('demo') ? 'Demo lineup on' : 'Demo lineup off');
    Store.reload();
  }

  function onPlayer(state, ch) {
    if (!ch || ch.id !== cur) return;
    $('testcard').classList.toggle('hide', state !== 'demo');
    if (state === 'error') Status.show('No signal', 'Channel ' + ch.number + ' ' + ch.name + ' is not responding. Retrying...');
    else Status.hide();
  }

  function onStore(ev) {
    if (ev === 'offline') {
      Status.show('Cannot reach TVMaster', Cfg.get('server') + '\nCheck that the server is running. Retrying...\n\nPress Red to change the server address.');
      return;
    }
    if (ev !== 'channels') { Banner.refresh(); return; }
    const list = Store.channels();
    if (!list.length) {
      Player.stop();
      playing = null;
      Status.show('No channels yet', 'Add channels in the TVMaster admin.\nPress Blue to try a demo lineup, Red for server settings.');
      return;
    }
    Status.hide();
    if (!cur || !Store.byId(cur)) {
      const first = Store.byId(Cfg.get('lastChannel')) || list[0];
      cur = null;
      playing = null;
      tune(first, true);
    }
  }

  function boot() {
    Screen.init();
    Theme.init();
    Clock.init();
    Player.onState(onPlayer);
    Guide.init({ current: () => cur, watch: (ch) => tune(ch, true) });
    Settings.init(reconnect);
    Keys.init(onKey);
    Store.on(onStore);
    Status.show('TVMaster Guide', 'Connecting to ' + Cfg.get('server') + ' ...\n\nPress Red for server settings.');
    Store.start();
    Log.d('started, server ' + Cfg.get('server') + (Cfg.get('demo') ? ' (demo)' : ''));
  }
  return { boot };
})();

window.addEventListener('load', App.boot);
