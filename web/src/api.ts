// A tiny typed client for the daemon's REST API. Browsers authenticate with
// the cookie that /api/v1/auth/login sets, so no token handling is needed here.

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

/** Fired when the server says the session is not authorised. */
export const unauthorized = new EventTarget();

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`/api/v1${path}`, {
      method,
      headers: body === undefined ? undefined : { 'content-type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'same-origin',
    });
  } catch {
    throw new ApiError(0, 'Cannot reach the TVMaster server');
  }
  if (res.status === 401) {
    unauthorized.dispatchEvent(new Event('unauthorized'));
  }
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  let data: unknown = undefined;
  try {
    data = text ? JSON.parse(text) : undefined;
  } catch {
    /* not JSON */
  }
  if (!res.ok) {
    const msg = (data as { error?: string } | undefined)?.error ?? `${res.status} ${res.statusText}`;
    throw new ApiError(res.status, msg);
  }
  return data as T;
}

export const api = {
  get: <T,>(path: string) => request<T>('GET', path),
  post: <T,>(path: string, body?: unknown) => request<T>('POST', path, body ?? {}),
  put: <T,>(path: string, body: unknown) => request<T>('PUT', path, body),
  patch: <T,>(path: string, body: unknown) => request<T>('PATCH', path, body),
  del: <T = void,>(path: string) => request<T>('DELETE', path),
};

export interface Asset {
  id: string;
  /** Where the file is on the server: what a channel's picture option stores. */
  path: string;
  /** Preview address (needs the same sign-in as the rest of the API). */
  url: string;
  bytes: number;
}

/** Upload a picture (png, jpg, gif, webp; up to 12 MB). */
export async function uploadAsset(file: File): Promise<Asset> {
  let res: Response;
  try {
    res = await fetch(`/api/v1/assets?name=${encodeURIComponent(file.name)}`, {
      method: 'POST',
      headers: { 'content-type': file.type || 'application/octet-stream' },
      body: file,
      credentials: 'same-origin',
    });
  } catch {
    throw new ApiError(0, 'Cannot reach the TVMaster server');
  }
  if (res.status === 401) unauthorized.dispatchEvent(new Event('unauthorized'));
  const data = (await res.json().catch(() => undefined)) as (Asset & { error?: string }) | undefined;
  if (!res.ok) throw new ApiError(res.status, data?.error ?? (res.status === 413 ? 'That picture is over 12 MB' : `${res.status} ${res.statusText}`));
  return data as Asset;
}

/** The preview address of an uploaded picture, from the path stored in a channel's options. */
export function assetUrlForPath(path: string | null | undefined): string | null {
  if (!path) return null;
  const m = /[\\/]assets[\\/]([A-Za-z0-9_.-]+\.(?:png|jpg|gif|webp))$/.exec(path);
  return m ? `/api/v1/assets/${m[1]}` : null;
}

export const errorMessage = (e: unknown): string =>
  e instanceof Error ? e.message : String(e);
