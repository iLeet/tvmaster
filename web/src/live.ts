// Live updates over the daemon's WebSocket, plus a small data-fetching hook.

import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage } from './api';

type Listener = (event: { type: string; [k: string]: unknown }) => void;

const listeners = new Set<Listener>();
let socket: WebSocket | null = null;
let retry = 1000;
let wanted = 0;
let connected = false;
const statusListeners = new Set<(c: boolean) => void>();

function setConnected(c: boolean) {
  connected = c;
  statusListeners.forEach((l) => l(c));
}

function open() {
  if (socket || wanted === 0) return;
  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  const ws = new WebSocket(`${proto}://${location.host}/api/v1/ws`);
  socket = ws;
  ws.onopen = () => {
    retry = 1000;
    setConnected(true);
  };
  ws.onmessage = (e) => {
    try {
      const msg = JSON.parse(e.data);
      listeners.forEach((l) => l(msg));
    } catch {
      /* ignore */
    }
  };
  ws.onclose = () => {
    socket = null;
    setConnected(false);
    if (wanted > 0) {
      setTimeout(open, retry);
      retry = Math.min(retry * 2, 15000);
    }
  };
  ws.onerror = () => ws.close();
}

/** Run `cb` for each pushed event whose type is in `types` (all if empty). */
export function useLiveEvent(types: string[], cb: Listener) {
  const ref = useRef(cb);
  ref.current = cb;
  const key = types.join(',');
  useEffect(() => {
    wanted++;
    open();
    const l: Listener = (m) => {
      if (types.length === 0 || types.includes(m.type)) ref.current(m);
    };
    listeners.add(l);
    return () => {
      listeners.delete(l);
      wanted--;
      if (wanted === 0 && socket) socket.close();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
}

/** Whether the live connection is up. */
export function useLiveStatus(): boolean {
  const [c, setC] = useState(connected);
  useEffect(() => {
    statusListeners.add(setC);
    return () => {
      statusListeners.delete(setC);
    };
  }, []);
  return c;
}

export interface Query<T> {
  data: T | undefined;
  error: string | null;
  loading: boolean;
  reload: () => void;
}

/** Fetch on mount and whenever `deps` change; `reload()` fetches again. */
export function useQuery<T>(fn: () => Promise<T>, deps: unknown[] = []): Query<T> {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  const fnRef = useRef(fn);
  fnRef.current = fn;
  useEffect(() => {
    let live = true;
    setLoading(true);
    fnRef
      .current()
      .then((d) => {
        if (live) {
          setData(d);
          setError(null);
        }
      })
      .catch((e) => live && setError(errorMessage(e)))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { data, error, loading, reload };
}
