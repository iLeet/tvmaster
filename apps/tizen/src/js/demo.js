'use strict';
// Sample lineup for trying the guide when the server has no channels (Blue key, or ?demo=1).
const Demo = (() => {
  const NAMES = ['Retro TV', 'Cartoon Club', 'Movie Vault', 'Sitcom Central', 'Kids Zone', 'Anime Night', 'Sports Desk',
    'News Now', 'Music Mix', 'Classic Westerns', 'Mystery Hour', 'Comedy Corner', 'Nature Docs', 'Late Show'];
  const SHOWS = [
    ['The Lazy Detective', 'Night Shift Down Town', 'A Case Of Cold Soup', 'The Long Elevator'],
    ['Captain Pickle', 'Pickle Goes To Space', 'The Great Cheese Heist', 'Rainy Day Rescue'],
    ['Dust And Thunder', 'Moonlight Express', 'The Last Lighthouse', 'Paper Planes'],
    ['The Neighbours', 'Block Party', 'The Wrong Casserole', 'Garage Sale'],
    ['Bubble Street', 'Learning To Whistle', 'The Lost Mitten', 'Rainbow Soup'],
    ['Star Drifter', 'Episode 12: Signal', 'Episode 13: Return', 'Episode 14: Orbit'],
  ];
  const hash = (s) => { let h = 7; for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0; return h; };

  function channels() {
    return NAMES.map((n, i) => ({
      id: 'demo' + (i + 2), number: i + 2, name: n, kind: i === 7 ? 'info' : 'programming',
      group: i < 4 ? 'Favourites' : '', on_air: true, now: null, next: null,
    }));
  }
  function schedule(id, hours) {
    const h = hash(id);
    const slot = (h % 2 === 0 ? 30 : 60) * 60000;
    const shows = SHOWS[h % SHOWS.length];
    const start = Math.floor((Date.now() - 4 * 3600000) / slot) * slot;
    const out = [];
    for (let t = start, i = 0; t < Date.now() + hours * 3600000; t += slot, i++) {
      const dur = (i % 5 === 3) ? slot * 2 : slot;
      const k = (i + h) % shows.length;
      out.push({ media_id: i, show: shows[0], title: shows[k], season: 1 + (i % 3), episode: 1 + (i % 12),
        start_ms: t, end_ms: t + dur, runtime_end_ms: t + dur - 2 * 60000, label: null });
      if (dur > slot) t += slot;
    }
    return out;
  }
  return { channels, schedule };
})();
