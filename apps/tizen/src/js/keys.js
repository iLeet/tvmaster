'use strict';
// Remote control -> logical keys. Also works with a computer keyboard for desktop testing:
//   arrows, Enter, 0-9, Esc = Back, PageUp/PageDown = CH+/CH-, g = Guide, i = Info,
//   r = Red (settings), f = Green (theme), y = Yellow (debug), b = Blue (demo).
const Keys = (() => {
  const TV = {
    37: 'LEFT', 38: 'UP', 39: 'RIGHT', 40: 'DOWN', 13: 'ENTER', 10009: 'BACK',
    427: 'CH_UP', 428: 'CH_DOWN', 457: 'INFO', 458: 'GUIDE', 10073: 'CH_LIST',
    403: 'RED', 404: 'GREEN', 405: 'YELLOW', 406: 'BLUE',
    415: 'PLAY', 19: 'PAUSE', 413: 'STOP', 417: 'FF', 412: 'RW',
  };
  const KEYBOARD = { 27: 'BACK', 33: 'CH_UP', 34: 'CH_DOWN', 71: 'GUIDE', 73: 'INFO',
    82: 'RED', 70: 'GREEN', 89: 'YELLOW', 66: 'BLUE' };
  const REGISTER = ['ChannelUp', 'ChannelDown', 'ChannelList', 'Info', 'Guide', 'ColorF0Red', 'ColorF1Green',
    'ColorF2Yellow', 'ColorF3Blue', 'MediaPlay', 'MediaPause', 'MediaStop', 'MediaRewind', 'MediaFastForward',
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

  function register() {
    try {
      if (typeof tizen === 'undefined' || !tizen.tvinputdevice) return;
      const have = tizen.tvinputdevice.getSupportedKeys().map((k) => k.name);
      Log.d('supported keys: ' + have.join(','));
      REGISTER.forEach((n) => {
        if (have.indexOf(n) >= 0) { try { tizen.tvinputdevice.registerKey(n); } catch (e) { Log.e('registerKey ' + n, e); } }
      });
    } catch (e) { Log.e('keys', e); }
  }

  function logical(e) {
    const c = e.keyCode;
    if (c >= 48 && c <= 57) return { k: 'DIGIT', d: c - 48 };
    if (c >= 96 && c <= 105) return { k: 'DIGIT', d: c - 96 };
    const k = TV[c] || KEYBOARD[c];
    return k ? { k } : null;
  }

  return {
    init(fn) {
      register();
      document.addEventListener('keydown', (e) => {
        const l = logical(e);
        if (!l) return;
        // A handler returns false to let the key through (typing into a text field).
        if (fn(l.k, l) !== false) e.preventDefault();
      });
    },
  };
})();
