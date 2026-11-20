'use strict';
const Fmt = (() => {
  const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const pad = (n) => String(n).padStart(2, '0');
  const MIN = 60000;
  return {
    MIN,
    time(ms) {
      const d = new Date(ms);
      const h = d.getHours();
      return (h % 12 || 12) + ':' + pad(d.getMinutes()) + ' ' + (h >= 12 ? 'PM' : 'AM');
    },
    day(ms) {
      const d = new Date(ms);
      return DAYS[d.getDay()] + ' ' + MONTHS[d.getMonth()] + ' ' + d.getDate();
    },
    range(a, b) { return Fmt.time(a) + ' - ' + Fmt.time(b); },
    floorHalfHour(ms) { return Math.floor(ms / (30 * MIN)) * 30 * MIN; },
    pad,
  };
})();
