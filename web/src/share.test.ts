import { describe, expect, it } from 'vitest';
import { origin, shareHosts, shareLinks, shareSchemes, tunerAddress, type ShareOptions } from './share';
import type { Network } from './types';

const net = (over: Partial<Network> = {}): Network => ({
  hostname: 'tv.local',
  hosts: ['tv.local', '10.0.0.5'],
  http_port: 8787,
  https_port: null,
  tls: { enabled: false, self_signed: true, fingerprint: null },
  ...over,
});
const opts = (over: Partial<ShareOptions> = {}): ShareOptions => ({ scheme: 'http', host: 'tv.local', token: '', ...over });

describe('share hosts and schemes', () => {
  it('never offers loopback when a real address is known', () => {
    expect(shareHosts(net(), 'localhost')).toEqual(['tv.local', '10.0.0.5']);
    expect(shareHosts(net(), '10.0.0.9')).toEqual(['10.0.0.9', 'tv.local', '10.0.0.5']);
    expect(shareHosts(undefined, '127.0.0.1')).toEqual(['127.0.0.1']);
  });
  it('offers https only when the server listens on it', () => {
    expect(shareSchemes(net(), 'http')).toEqual(['http']);
    expect(shareSchemes(net({ https_port: 8788 }), 'http')).toEqual(['http', 'https']);
    expect(shareSchemes(net({ http_port: null, https_port: 8788 }), 'https')).toEqual(['https']);
  });
});

describe('share links', () => {
  it('uses each scheme with its own port and brackets IPv6', () => {
    const n = net({ https_port: 8788 });
    expect(origin(n, opts())).toBe('http://tv.local:8787');
    expect(origin(n, opts({ scheme: 'https' }))).toBe('https://tv.local:8788');
    expect(origin(n, opts({ host: 'fe80::1' }))).toBe('http://[fe80::1]:8787');
  });
  it('falls back to the page port before the server has said anything', () => {
    expect(origin(undefined, opts(), '9000')).toBe('http://tv.local:9000');
  });
  it('puts the token only on links apps use directly, and encodes it', () => {
    const l = shareLinks(net(), opts({ token: 'abc 123' }));
    expect(l.iptv[0].value).toBe('http://tv.local:8787/iptv/playlist.m3u?token=abc%20123');
    expect(l.iptv.every((x) => x.hasToken)).toBe(true);
    expect(l.web.every((x) => !x.value.includes('token'))).toBe(true);
    expect(l.tuner[0].value).not.toContain('token');
    const none = shareLinks(net(), opts());
    expect(none.iptv.every((x) => !x.value.includes('token') && !x.hasToken)).toBe(true);
  });
  it('gives the tuner the plain-HTTP port even when HTTPS is selected', () => {
    expect(tunerAddress(net({ https_port: 8788 }), opts({ scheme: 'https', host: '10.0.0.5' }))).toBe('10.0.0.5:8787');
  });
});
