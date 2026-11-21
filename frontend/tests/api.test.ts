import { afterEach, expect, test } from 'bun:test';

class MemoryStorage {
  private values = new Map<string, string>();
  getItem(key: string) { return this.values.get(key) ?? null; }
  setItem(key: string, value: string) { this.values.set(key, value); }
  removeItem(key: string) { this.values.delete(key); }
  clear() { this.values.clear(); }
}

const local = new MemoryStorage();
const session = new MemoryStorage();
const browserDocument = Object.assign(new EventTarget(), { cookie: '' });
const browserWindow = new EventTarget();
Object.defineProperties(globalThis, {
  document: { value: browserDocument, configurable: true },
  window: { value: browserWindow, configurable: true },
  localStorage: { value: local, configurable: true },
  sessionStorage: { value: session, configurable: true },
  location: { value: new URL('https://market.example'), configurable: true },
});

const { api, authenticate, guestCart, guestSession, initializeSession, isPublicMedia, localRedirect, logout, signedIn, trackingUrl, watchSessionIdentity } = await import('../src/api');
const realFetch = globalThis.fetch;
const expired = () => Response.json({ error: 'Session expired' }, { status: 401 });

afterEach(() => {
  globalThis.fetch = realFetch;
  local.clear();
  session.clear();
  browserDocument.cookie = '';
});

test('redirects account flows to the market origin', () => {
  expect(localRedirect('/checkout?mode=guest', '/')).toBe('https://market.example/checkout?mode=guest');
  expect(localRedirect('https://example.org', '/merchant')).toBe('/merchant');
  expect(localRedirect('//example.org', '/merchant')).toBe('/merchant');
});

test('restricts customer attachment and tracking link destinations', () => {
  expect(isPublicMedia('/uploads/287a2b89-187d-4e92-8607-8fa89faef04f.avif')).toBe(true);
  expect(isPublicMedia('/catalog/hero.jpg')).toBe(false);
  expect(trackingUrl('https://courier.example/track/42')).toBe('https://courier.example/track/42');
  expect(trackingUrl('ftp://courier.example/track/42')).toBeUndefined();
});

test('ignores malformed and duplicate legacy cart entries', () => {
  local.setItem('tosca-cart', JSON.stringify([
    null, { id: 2, quantity: -1, created_at: '' },
    { id: 5, quantity: 4, created_at: '2026-01-01T00:00:00Z' },
    { id: 5, quantity: 6, created_at: '2026-01-01T00:00:00Z' },
  ]));
  expect(guestCart().map(({ id, quantity }) => ({ id, quantity }))).toEqual([{ id: 5, quantity: 6 }]);
});

test('does not refresh or replay a request after switching accounts', async () => {
  browserDocument.cookie = 'tosca_session=account.first.first-generation';
  const firstResponse = Promise.withResolvers<Response>();
  let reads = 0;
  const requestPaths: string[] = [];
  globalThis.fetch = (async (input: RequestInfo | URL, options?: RequestInit) => {
    requestPaths.push(String(input));
    if (input === '/api/auth/login') {
      expect(new Headers(options?.headers).get('X-Tosca-Request')).toBe('1');
      expect(options?.credentials).toBe('same-origin');
      browserDocument.cookie = 'tosca_session=account.second.next-generation';
      return Response.json({ ok: true });
    }
    reads++;
    return firstResponse.promise;
  }) as typeof fetch;

  const result = api('/cart').catch((error: Error) => error.message);
  await authenticate('login', { email: 'customer@example.com', password: 'example password' });
  firstResponse.resolve(expired());
  expect(await result).toBe('Session expired');
  expect(reads).toBe(1);
  expect(requestPaths).toEqual(['/api/cart', '/api/auth/login']);
});

test('concurrent expired requests share one cookie rotation and retry inside the same session', async () => {
  browserDocument.cookie = 'tosca_session=account.first.initial';
  let refreshes = 0;
  globalThis.fetch = (async (input: RequestInfo | URL, options?: RequestInit) => {
    const headers = new Headers(options?.headers);
    expect(headers.has('Authorization')).toBe(false);
    expect(options?.credentials).toBe('same-origin');
    if (input === '/api/auth/refresh') {
      refreshes++;
      expect(headers.get('X-Tosca-Request')).toBe('1');
      expect(options?.body).toBeUndefined();
      browserDocument.cookie = 'tosca_session=account.first.rotated';
      return Response.json({ ok: true });
    }
    expect(headers.has('X-Tosca-Request')).toBe(false);
    return browserDocument.cookie.endsWith('.initial') ? expired() : Response.json([]);
  }) as typeof fetch;

  expect(await Promise.all([api('/cart'), api('/addresses')])).toEqual([[], []]);
  expect(refreshes).toBe(1);
});

test('logout uses the origin-wide authentication lock and sends no token body', async () => {
  browserDocument.cookie = 'tosca_session=guest.first.initial';
  Object.defineProperty(navigator, 'locks', { configurable: true, value: {
    request: async (name: string, options: { signal: AbortSignal }, operation: () => Promise<unknown>) => {
      expect(name).toBe('tosca-session');
      expect(options.signal.aborted).toBe(false);
      return operation();
    },
  } });
  globalThis.fetch = (async (input: RequestInfo | URL, options?: RequestInit) => {
    expect(input).toBe('/api/auth/logout');
    expect(options?.body).toBeUndefined();
    expect(new Headers(options?.headers).get('X-Tosca-Request')).toBe('1');
    browserDocument.cookie = '';
    return Response.json({ ok: true });
  }) as typeof fetch;

  try {
    expect(guestSession()).toBe(true);
    await logout();
    expect(signedIn()).toBe(false);
  } finally {
    Reflect.deleteProperty(navigator, 'locks');
  }
});

test('exchanges the old guest refresh once and removes both legacy credential stores', async () => {
  local.setItem('access', 'legacy-guest-access');
  local.setItem('refresh', 'legacy-guest-refresh');
  session.setItem('access', 'legacy-account-access');
  session.setItem('refresh', 'legacy-account-refresh');
  let upgrades = 0;
  globalThis.fetch = (async (input: RequestInfo | URL, options?: RequestInit) => {
    expect(input).toBe('/api/auth/session-upgrade');
    expect(JSON.parse(String(options?.body))).toEqual({ refresh: 'legacy-guest-refresh' });
    expect(new Headers(options?.headers).get('X-Tosca-Request')).toBe('1');
    upgrades++;
    browserDocument.cookie = 'tosca_session=guest.migrated.initial';
    return Response.json({ ok: true });
  }) as typeof fetch;

  await initializeSession();
  await initializeSession();
  expect(upgrades).toBe(1);
  expect(guestSession()).toBe(true);
  expect(local.getItem('access')).toBeNull();
  expect(local.getItem('refresh')).toBeNull();
  expect(session.getItem('access')).toBeNull();
  expect(session.getItem('refresh')).toBeNull();
});

test('discards a rejected legacy guest credential without a second attempt', async () => {
  local.setItem('refresh', 'expired-legacy-guest-refresh');
  let attempts = 0;
  globalThis.fetch = (async () => {
    attempts++;
    return expired();
  }) as typeof fetch;

  await Promise.all([initializeSession(), initializeSession()]);
  expect(attempts).toBe(1);
  expect(local.getItem('refresh')).toBeNull();
  expect(signedIn()).toBe(false);
});

test('detects cross-tab identity changes without reacting to refreshes or its own sign-in', async () => {
  browserDocument.cookie = 'tosca_session=guest.first.initial';
  let changes = 0;
  const stopWatching = watchSessionIdentity(() => changes++);
  globalThis.fetch = (async () => {
    browserDocument.cookie = 'tosca_session=account.second.signed-in';
    browserWindow.dispatchEvent(new Event('focus'));
    return Response.json({ ok: true });
  }) as typeof fetch;

  try {
    browserDocument.cookie = 'tosca_session=guest.first.rotated';
    browserWindow.dispatchEvent(new Event('focus'));
    expect(changes).toBe(0);

    await authenticate('login', { email: 'customer@example.com', password: 'example password' });
    browserDocument.dispatchEvent(new Event('visibilitychange'));
    expect(changes).toBe(0);

    browserDocument.cookie = '';
    browserWindow.dispatchEvent(new Event('focus'));
    expect(changes).toBe(1);
  } finally {
    stopWatching();
  }
});
