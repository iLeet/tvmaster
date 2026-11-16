// Addresses for other apps and devices, built from what the server says about
// itself. Pure (no React, no window) so it is unit tested.

import type { Network } from './types';

export type Scheme = 'http' | 'https';

export interface ShareOptions {
  scheme: Scheme;
  host: string;
  /** Added to links apps use directly; blank = none. */
  token: string;
}

export interface ShareLink {
  id: string;
  label: string;
  value: string;
  hasToken?: boolean;
}

const LOOPBACK = ['localhost', '127.0.0.1', '::1', '[::1]'];
export const isLoopback = (host: string) => LOOPBACK.includes(host.toLowerCase());

/** Hosts someone else could use: the one this page was opened on (unless loopback), then the server's own names and addresses. */
export function shareHosts(net: Network | undefined, pageHost: string): string[] {
  const out: string[] = [];
  if (pageHost && !isLoopback(pageHost)) out.push(pageHost);
  for (const h of net?.hosts ?? []) if (!out.includes(h)) out.push(h);
  if (out.length === 0 && pageHost) out.push(pageHost);
  return out;
}

export function shareSchemes(net: Network | undefined, pageScheme: Scheme): Scheme[] {
  if (!net) return [pageScheme];
  const s: Scheme[] = [];
  if (net.http_port !== null) s.push('http');
  if (net.https_port !== null) s.push('https');
  return s.length ? s : [pageScheme];
}

export function portFor(net: Network | undefined, scheme: Scheme, pagePort: string): number | null {
  const fromServer = scheme === 'http' ? net?.http_port : net?.https_port;
  if (fromServer != null) return fromServer;
  return pagePort ? Number(pagePort) : null;
}

const bracket = (h: string) => (h.includes(':') && !h.startsWith('[') ? `[${h}]` : h);

export function origin(net: Network | undefined, o: ShareOptions, pagePort = ''): string {
  const port = portFor(net, o.scheme, pagePort);
  return `${o.scheme}://${bracket(o.host)}${port ? `:${port}` : ''}`;
}

const q = (token: string) => (token.trim() ? `?token=${encodeURIComponent(token.trim())}` : '');

/** `host:port` as Plex, Jellyfin and Emby ask for it: always the plain-HTTP port. */
export function tunerAddress(net: Network | undefined, o: ShareOptions, pagePort = ''): string {
  const port = portFor(net, 'http', pagePort) ?? 8787;
  return `${bracket(o.host)}:${port}`;
}

export function shareLinks(net: Network | undefined, o: ShareOptions, pagePort = ''): { iptv: ShareLink[]; tuner: ShareLink[]; web: ShareLink[]; smarttv: ShareLink[] } {
  const base = origin(net, o, pagePort);
  const t = q(o.token);
  const withToken = !!t;
  return {
    iptv: [
      { id: 'playlist', label: 'Playlist (M3U)', value: `${base}/iptv/playlist.m3u${t}`, hasToken: withToken },
      { id: 'guide', label: 'Programme guide (XMLTV)', value: `${base}/iptv/guide.xml${t}`, hasToken: withToken },
    ],
    tuner: [{ id: 'tuner', label: 'Tuner address', value: tunerAddress(net, o, pagePort) }],
    // The page a TV's own browser opens: full screen, flipped with the remote. It needs the token itself.
    smarttv: [{ id: 'smarttv', label: 'Smart TV browser', value: `${base}/?ui=tv${t ? `&${t.slice(1)}` : ''}`, hasToken: withToken }],
    web: [
      { id: 'player', label: 'Web player', value: `${base}/` },
      { id: 'admin', label: 'Web admin', value: `${base}/admin/` },
      { id: 'remote', label: 'Phone remote', value: `${base}/admin/#/remote` },
      { id: 'tv', label: 'Make a screen a TV', value: `${base}/?tv=living-room` },
    ],
  };
}
