'use strict';
// Tiny logger: console plus a ring buffer shown on screen with the Yellow key.
const Log = (() => {
  const lines = [];
  const el = () => document.getElementById('debug');
  function push(level, args) {
    const s = new Date().toTimeString().slice(0, 8) + ' ' + level + ' ' +
      Array.prototype.map.call(args, (a) => (typeof a === 'string' ? a : (a && a.message) || JSON.stringify(a))).join(' ');
    lines.push(s);
    if (lines.length > 28) lines.shift();
    try { console.log(s); } catch (e) { /* no console */ }
    const d = el();
    if (d && !d.classList.contains('hide')) d.textContent = lines.join('\n');
  }
  return {
    d() { push('D', arguments); },
    e() { push('E', arguments); },
    toggle() {
      const d = el();
      d.classList.toggle('hide');
      d.textContent = lines.join('\n');
    },
  };
})();
