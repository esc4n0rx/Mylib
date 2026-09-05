import { ApiError, kindForStatus } from './errors';
import { session } from './session';

const BASE = '/api/v1';

interface RequestOptions {
  method?: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';
  body?: unknown;
  query?: Record<string, string | number | boolean | undefined>;
  headers?: Record<string, string>;
  /** Absolute path (not prefixed with /api/v1), e.g. "/health". */
  raw?: boolean;
  signal?: AbortSignal;
}

function buildUrl(path: string, opts: RequestOptions): string {
  const url = opts.raw ? path : `${BASE}${path}`;
  if (!opts.query) return url;
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(opts.query)) {
    if (value !== undefined) params.set(key, String(value));
  }
  const qs = params.toString();
  return qs ? `${url}?${qs}` : url;
}

export async function request<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const headers: Record<string, string> = { ...opts.headers };
  if (opts.body !== undefined) headers['Content-Type'] = 'application/json';
  if (session.token) headers['Authorization'] = `Bearer ${session.token}`;

  let response: Response;
  try {
    response = await fetch(buildUrl(path, opts), {
      method: opts.method ?? (opts.body !== undefined ? 'POST' : 'GET'),
      headers,
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
      signal: opts.signal,
    });
  } catch {
    throw new ApiError({
      status: 0,
      kind: 'network',
      message: 'Network request failed',
    });
  }

  if (response.status === 204) return undefined as T;

  const text = await response.text();
  const payload = text ? safeJson(text) : undefined;

  if (!response.ok) {
    const errorBody = (payload as { error?: { code?: string; message?: string } })?.error;
    if (response.status === 401 && session.isAuthenticated && errorBody?.code !== 'INVALID_PROFILE_PIN') {
      session.clear();
    }
    throw new ApiError({
      status: response.status,
      code: errorBody?.code,
      message: errorBody?.message ?? response.statusText,
      kind: kindForStatus(response.status),
      details: payload,
    });
  }

  return payload as T;
}

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}
