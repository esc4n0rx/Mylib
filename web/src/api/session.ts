// Single owner of the access token. Components never read the token directly;
// they call the API client, which asks this module. Swapping to an HttpOnly
// cookie later means changing only this file + client.ts.

const STORAGE_KEY = 'mylib.session';

let accessToken: string | null = null;
const listeners = new Set<() => void>();

function notify() {
  listeners.forEach((fn) => fn());
}

export const session = {
  load(): void {
    try {
      accessToken = localStorage.getItem(STORAGE_KEY);
    } catch {
      accessToken = null;
    }
  },
  get token(): string | null {
    return accessToken;
  },
  get isAuthenticated(): boolean {
    return accessToken !== null;
  },
  set(token: string): void {
    accessToken = token;
    try {
      localStorage.setItem(STORAGE_KEY, token);
    } catch {
      /* persistence is best-effort */
    }
    notify();
  },
  clear(): void {
    accessToken = null;
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      /* ignore */
    }
    notify();
  },
  subscribe(fn: () => void): () => void {
    listeners.add(fn);
    return () => listeners.delete(fn);
  },
};
