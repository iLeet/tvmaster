'use strict';
// Video. On a Samsung TV this is AVPlay (hardware HLS, drawn behind the web layer);
// in a desktop browser it is <video> + hls.js, so the app can be tried without a TV.
const Player = (() => {
  const FULL = { x: 0, y: 0, w: 1920, h: 1080 };
  const avp = () => (typeof webapis !== 'undefined' && webapis.avplay ? webapis.avplay : null);
  let rect = FULL;
  let token = 0;
  let hls = null;
  let retryTimer = 0;
  let current = null;
  let handler = () => {};
  let opened = false;

  const vid = () => document.getElementById('vid');

  function applyRect() {
    const a = avp();
    if (a && opened) {
      try { a.setDisplayRect(rect.x, rect.y, rect.w, rect.h); } catch (e) { Log.e('rect', e); }
    }
    const v = vid();
    v.style.left = rect.x + 'px';
    v.style.top = rect.y + 'px';
    v.style.width = rect.w + 'px';
    v.style.height = rect.h + 'px';
  }

  function stopAll() {
    const a = avp();
    if (a) {
      try { a.stop(); } catch (e) { /* not playing */ }
      try { a.close(); } catch (e) { /* not open */ }
      opened = false;
    }
    if (hls) { try { hls.destroy(); } catch (e) { /* gone */ } hls = null; }
    const v = vid();
    try { v.pause(); v.removeAttribute('src'); v.load(); } catch (e) { /* no video */ }
  }

  function fail(my, why) {
    if (my !== token) return;
    Log.e('player', why);
    handler('error', current);
    clearTimeout(retryTimer);
    retryTimer = setTimeout(() => { if (my === token && current) tune(current); }, 4000);
  }

  function tuneAv(a, url, my) {
    try {
      a.open(url);
      opened = true;
      a.setListener({
        onbufferingstart() { if (my === token) handler('loading', current); },
        onbufferingcomplete() { if (my === token) handler('playing', current); },
        onstreamcompleted() { fail(my, 'stream completed'); },
        onerror(e) { fail(my, 'avplay error ' + e); },
        onevent() {}, onerrormsg() {}, ondrmevent() {}, onsubtitlechange() {},
      });
      applyRect();
      a.setDisplayMethod('PLAYER_DISPLAY_MODE_LETTER_BOX');
      a.prepareAsync(() => {
        if (my !== token) return;
        try { a.play(); } catch (e) { fail(my, e); return; }
        handler('playing', current);
      }, (e) => fail(my, 'prepare ' + e));
    } catch (e) { fail(my, e); }
  }

  function tuneHls(url, my) {
    const v = vid();
    applyRect();
    v.onplaying = () => { if (my === token) handler('playing', current); };
    v.onerror = () => fail(my, 'video error');
    if (window.Hls && Hls.isSupported()) {
      hls = new Hls({ maxBufferLength: 10, liveSyncDurationCount: 3 });
      hls.on(Hls.Events.ERROR, (ev, d) => { if (d.fatal) fail(my, d.details); });
      hls.loadSource(url);
      hls.attachMedia(v);
    } else { v.src = url; }
    const p = v.play();
    if (p && p.catch) p.catch(() => { /* autoplay may need a key press first */ });
  }

  function tune(ch) {
    const my = ++token;
    current = ch;
    clearTimeout(retryTimer);
    stopAll();
    handler('loading', ch);
    if (Cfg.get('demo')) { handler('demo', ch); return; }
    const url = Api.streamUrl(ch.id);
    Log.d('tune', ch.number, url);
    const a = avp();
    if (a) tuneAv(a, url, my); else tuneHls(url, my);
  }

  return {
    onState(f) { handler = f; },
    tune,
    stop() { token++; clearTimeout(retryTimer); stopAll(); },
    setRect(r) { rect = r || FULL; applyRect(); },
    FULL,
  };
})();
