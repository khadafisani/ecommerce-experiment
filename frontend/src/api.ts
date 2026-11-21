import { prepareUpload } from './upload-media';

export type Row = Record<string, any>;
export type SearchHit = {
  id:number;
  name:string;
  kind:'product'|'shop';
  image?:string;
  shop?:string;
  lowest_price?:number|null;
};
type SearchDocument = Omit<SearchHit,'kind'>;
export type SearchResponse = { items: SearchHit[] } | { items: SearchDocument[]; shops: SearchDocument[] };
export type SearchGroups = { products: SearchHit[]; shops: SearchHit[] };

export function groupSearchHits(response: SearchResponse): SearchGroups {
  if ('shops' in response) {
    return {
      products: response.items.slice(0, 6).map(product => ({ ...product, kind: 'product' })),
      shops: response.shops.slice(0, 4).map(shop => ({ ...shop, kind: 'shop' })),
    };
  }
  // Older backend builds can return a single array with kind on each result.
  return {
    products: response.items.filter(hit => hit.kind === 'product').slice(0, 6),
    shops: response.items.filter(hit => hit.kind === 'shop').slice(0, 4),
  };
}
export const searchHitPath = (hit: Pick<SearchHit,'kind'|'id'>) => '/' + hit.kind + '/' + hit.id;
export type ItemPick = { id: number; quantity: number };
export const key = () => crypto.randomUUID();
export const money = (n: number | string | null | undefined) => n == null ? '—' : new Intl.NumberFormat('id-ID', { style: 'currency', currency: 'IDR', maximumFractionDigits: 0 }).format(Number(n));
export const date = (s: string) => s ? new Date(s.replace(' ', 'T') + (s.endsWith('Z') ? '' : 'Z')).toLocaleDateString('en-ID', { dateStyle: 'medium' }) : '—';
const labels: Record<string, string> = {waiting_payment:'Waiting for payment', waiting_shop:'Waiting shop response', pending:'In review', packing:'On packing', delivery:'On delivery', return_review:'Return in review', rejected:'Rejected', cancelled:'Cancelled', paid:'Paid', waiting:'Waiting for payment', return_accepted:'Return accepted', return_rejected:'Return rejected', return_completed:'Return completed'};
export const label = (s: string) => labels[s] ?? (s ? s.charAt(0).toUpperCase() + s.slice(1) : '—');
export const params = (values: Record<string, string | number | undefined | null>) => {
  const search = new URLSearchParams();
  for (const [name, value] of Object.entries(values)) if (value != null && value !== '') search.set(name, String(value));
  return search.size ? '?' + search : '';
};
export function localRedirect(value: string | null, fallback: string) {
  if (!value?.startsWith('/') || value.startsWith('//')) return fallback;
  try {
    const target = new URL(value, location.origin);
    return target.origin === location.origin && !target.username && !target.password ? target.href : fallback;
  } catch {
    return fallback;
  }
}

export function trackingUrl(value: unknown) {
  if (typeof value !== 'string') return undefined;
  try {
    const url = new URL(value);
    if ((url.protocol === 'https:' || url.protocol === 'http:') && !url.username && !url.password) return url.href;
  } catch { /* Skip invalid tracking URLs. */ }
}

const uuidPattern = '[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}';
const publicMedia = new RegExp('^/uploads/' + uuidPattern + '\\.(avif|mp4|webm)$');
const returnMedia = new RegExp('^/api/return-media/[1-9][0-9]*-' + uuidPattern + '\\.(avif|mp4|webm)$');
export const isPublicMedia = (value: unknown): value is string => typeof value === 'string' && publicMedia.test(value);

function currentSession() {
  const marker = document.cookie.split(';').map(cookie => cookie.trim())
    .find(cookie => cookie.startsWith('tosca_session='))?.slice('tosca_session='.length);
  const match = /^(guest|account)\.([a-z0-9_-]+)\.([a-z0-9_-]+)$/i.exec(marker ?? '');
  return match ? { kind: match[1], id: match[2], generation: match[3] } : undefined;
}
export const guestSession = () => currentSession()?.kind === 'guest';
export const signedIn = () => Boolean(currentSession());

const sessionChannel = 'tosca-session-identity';
const identityObservers = new Set<(id: string | undefined) => void>();
let localSessionChange = false;

function openSessionChannel() {
  try {
    return typeof BroadcastChannel === 'undefined' ? undefined : new BroadcastChannel(sessionChannel);
  } catch {
    return undefined;
  }
}

export function watchSessionIdentity(onChange: () => void) {
  let observed = currentSession()?.id;
  const update = (id: string | undefined) => { observed = id; };
  const check = () => {
    const id = currentSession()?.id;
    if (localSessionChange || observed === id) return;
    observed = id;
    onChange();
  };
  identityObservers.add(update);
  const channel = openSessionChannel();
  channel?.addEventListener('message', check);
  window.addEventListener('focus', check);
  window.addEventListener('pageshow', check);
  document.addEventListener('visibilitychange', check);
  return () => {
    identityObservers.delete(update);
    channel?.close();
    window.removeEventListener('focus', check);
    window.removeEventListener('pageshow', check);
    document.removeEventListener('visibilitychange', check);
  };
}

async function changeSession<T>(operation: () => Promise<T>) {
  const previous = currentSession()?.id;
  localSessionChange = true;
  try {
    return await operation();
  } finally {
    localSessionChange = false;
    const current = currentSession()?.id;
    if (previous !== current) {
      for (const notify of identityObservers) notify(current);
      const channel = openSessionChannel();
      channel?.postMessage('change');
      channel?.close();
    }
  }
}

let authQueue = Promise.resolve();
function withSessionLock<T>(operation: () => Promise<T>, signal: AbortSignal) {
  if (navigator.locks) return navigator.locks.request('tosca-session', { signal }, operation);
  const task = authQueue.then(() => {
    signal.throwIfAborted();
    return operation();
  });
  authQueue = task.then(() => {}, () => {});
  return task;
}

function send(path: string, method: string, body: unknown, signal: AbortSignal, keepalive = false) {
  const headers = new Headers();
  if (method !== 'GET' && method !== 'HEAD') headers.set('X-Tosca-Request', '1');
  if (body !== undefined && !(body instanceof FormData)) headers.set('Content-Type', 'application/json');
  const payload = body === undefined || body instanceof FormData ? body : JSON.stringify(body);
  return fetch('/api' + path, { method, headers, keepalive, signal, credentials: 'same-origin', body: payload });
}

async function checkResponse(response: Response) {
  if (response.ok) return response;
  const data = await response.json().catch(() => ({}));
  throw new Error(data.error ?? 'Could not connect to Tosca Market');
}

async function request(path: string, method = 'GET', body?: unknown, retry = true, keepalive = false, signal?: AbortSignal): Promise<Response> {
  method = method.toUpperCase();
  const session = currentSession();
  const timeout = AbortSignal.timeout(method === 'GET' || method === 'HEAD' ? 30_000 : 120_000);
  const requestSignal = signal ? AbortSignal.any([signal, timeout]) : timeout;
  const response = await send(path, method, body, requestSignal, keepalive);
  if (response.status === 401 && retry && session && !requestSignal.aborted) {
    const followup = await withSessionLock(async () => {
      const current = currentSession();
      if (current?.id !== session.id) return response;
      if (current.generation === session.generation) {
        const renewed = await changeSession(() => send('/auth/refresh', 'POST', undefined, AbortSignal.timeout(30_000)));
        void renewed.body?.cancel().catch(() => {});
        if (!renewed.ok || currentSession()?.id !== session.id) return response;
      }
      requestSignal.throwIfAborted();
      void response.body?.cancel().catch(() => {});
      return send(path, method, body, requestSignal, keepalive);
    }, requestSignal);
    return checkResponse(followup);
  }
  return checkResponse(response);
}
export async function api<T = Row>(path: string, method = 'GET', body?: unknown, retry = true, signal?: AbortSignal): Promise<T> {
  return (await request(path, method, body, retry, false, signal)).json() as Promise<T>;
}

export async function authenticate(mode: 'login' | 'register' | 'guest', body?: Row, { importGuestCart = false } = {}) {
  const signal = AbortSignal.timeout(120_000);
  await withSessionLock(async () => {
    const prior = currentSession();
    if (mode === 'guest' && prior) return;
    await changeSession(() => api('/auth/' + mode, 'POST', body, false, signal));
    const session = currentSession();
    if (!session || session.id === prior?.id || session.kind !== (mode === 'guest' ? 'guest' : 'account')) {
      throw new Error('Allow first-party cookies to sign in to Tosca Market.');
    }
    if (importGuestCart) await mergeGuestCart(signal).catch(() => {});
  }, signal);
}

function discardLegacyCredentials() {
  for (const readStorage of [() => localStorage, () => sessionStorage]) {
    try {
      const storage = readStorage();
      for (const name of ['access', 'refresh', 'tosca-session']) storage.removeItem(name);
    } catch { /* Storage can be unavailable even when cookies work. */ }
  }
}

export async function initializeSession() {
  try {
    if (!signedIn() && localStorage.getItem('refresh')) {
      const signal = AbortSignal.timeout(10_000);
      await withSessionLock(async () => {
        try {
          const refresh = localStorage.getItem('refresh');
          if (refresh && !signedIn()) {
            await changeSession(() => api('/auth/session-upgrade', 'POST', { refresh }, false, signal));
          }
        } finally {
          discardLegacyCredentials();
        }
      }, signal);
      return;
    }
  } catch { /* Continue as signed out if the old guest session cannot be upgraded. */ }
  discardLegacyCredentials();
}

type GuestSignal = { product_id: number; kind: 'view' | 'cart'; at: number };
const guestSignalsKey = 'tosca-recommendation-signals';
const guestViewsKey = 'tosca-recommendation-view-day';
const secondsPerDay = 86_400;
const guestSignalLimit = 100;
const nowInSeconds = () => Math.floor(Date.now() / 1000);

function saveGuestSignals(signals: GuestSignal[]) {
  try { localStorage.setItem(guestSignalsKey, JSON.stringify(signals)); }
  catch { /* Recommendations are optional when storage is unavailable. */ }
}

function guestSignals(now: number): GuestSignal[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(guestSignalsKey) ?? '[]');
    if (!Array.isArray(stored)) return [];
    const signals = stored.filter((value): value is GuestSignal =>
      value != null && Number.isSafeInteger(value.product_id) && value.product_id > 0
      && (value.kind === 'view' || value.kind === 'cart') && Number.isSafeInteger(value.at)
      && value.at <= now && value.at > now - 30 * secondsPerDay
    ).sort((left, right) => right.at - left.at).slice(0, guestSignalLimit);
    if (signals.length !== stored.length) saveGuestSignals(signals);
    return signals;
  } catch {
    return [];
  }
}

function recordGuestSignal(productId: number, kind: GuestSignal['kind']) {
  if (!Number.isSafeInteger(productId) || productId <= 0) return;
  const now = nowInSeconds();
  const day = Math.floor(now / secondsPerDay);
  const signals = guestSignals(now);
  if (signals.some(signal => signal.product_id === productId && signal.kind === kind
    && Math.floor(signal.at / secondsPerDay) === day)) return;
  saveGuestSignals([{ product_id: productId, kind, at: now }, ...signals].slice(0, guestSignalLimit));
}

function claimGuestView(productId: number) {
  const day = Math.floor(nowInSeconds() / secondsPerDay);
  try {
    const previous = JSON.parse(localStorage.getItem(guestViewsKey) ?? 'null');
    const products: number[] = previous?.day === day && Array.isArray(previous.products) ? previous.products.slice(0, guestSignalLimit) : [];
    if (products.length >= guestSignalLimit || products.includes(productId)) return false;
    localStorage.setItem(guestViewsKey, JSON.stringify({ day, products: [...products, productId] }));
    return true;
  } catch {
    return false;
  }
}

export async function recordProductView(productId: number): Promise<void> {
  if (!Number.isSafeInteger(productId) || productId <= 0) return;
  if (!signedIn() || guestSession()) {
    recordGuestSignal(productId, 'view');
    if (!claimGuestView(productId)) return;
  }
  await request('/recommendation-events/view', 'POST', { product_id: productId }, true, true).catch(() => {});
}

function currentGuestSignals() {
  return signedIn() && !guestSession() ? undefined : guestSignals(nowInSeconds());
}

export async function recommendedProducts(productId?: number, signal?: AbortSignal): Promise<Row[]> {
  const { enabled, items } = await api<{ enabled: boolean; items: Row[] }>('/recommendations', 'POST', {
    product_id: productId,
    signals: currentGuestSignals(),
  }, true, signal);
  return enabled ? items : [];
}

export function exploreRecommendations() {
  const signals = currentGuestSignals();
  return (cursor: string | null, signal: AbortSignal) => api<{ enabled: boolean; items: (Row & { id: number })[]; cursor: string | null }>(
    '/recommendations/explore', 'POST', cursor === null ? { signals } : { cursor }, true, signal,
  );
}

export async function logout() {
  const session = currentSession()?.id;
  const signal = AbortSignal.timeout(30_000);
  await withSessionLock(async () => {
    if (currentSession()?.id === session) {
      await changeSession(() => api('/auth/logout', 'POST', undefined, false, signal));
    }
  }, signal);
}
export async function upload(file: File, privateMedia = false, signal?: AbortSignal) {
  const prepared = await prepareUpload(file, signal);
  const form = new FormData();
  form.append('file', prepared);
  return (await api('/upload'+(privateMedia?'?private=true':''),'POST',form,true,signal)).url as string;
}
export async function downloadReturnMedia(url: string) {
  const extension = returnMedia.exec(url)?.[1];
  if (!extension) throw new Error('Attachment unavailable');
  const response = await request(url.slice(4));
  const address = URL.createObjectURL(await response.blob());
  const link = document.createElement('a');
  link.href = address;
  link.download = 'return-evidence.' + extension;
  try { link.click(); }
  finally { setTimeout(() => URL.revokeObjectURL(address), 1000); }
}
type GuestItem = Row & { id:number; quantity:number; created_at:string };
const cartLimit = 100;
export function guestCart(): GuestItem[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem('tosca-cart') ?? '[]');
    if (!Array.isArray(stored)) return [];
    const valid = stored.filter((item): item is GuestItem => item != null && Number.isSafeInteger(item.id)
      && item.id > 0 && Number.isInteger(item.quantity) && item.quantity > 0 && item.quantity <= 999
      && typeof item.created_at === 'string');
    return [...new Map(valid.map(row => [row.id, row])).values()];
  } catch { return []; }
}
export function setGuest(id: number, quantity: number, item?: Row) {
  if (!Number.isInteger(quantity) || quantity < 0 || quantity > 999) throw new Error('Enter a quantity from 1 to 999');
  const cart = guestCart();
  const old = cart.find(row => row.id === id);
  if (quantity && (item || old)) cart.push({ ...old, ...item, id, quantity, created_at: old?.created_at ?? new Date().toISOString() });
  localStorage.setItem('tosca-cart',JSON.stringify(cart.filter((row,index) => row.id !== id || (quantity > 0 && index === cart.length - 1))));
  if (quantity > (old?.quantity ?? 0) && (!signedIn() || guestSession())) {
    recordGuestSignal(item?.product_id ?? old?.product_id, 'cart');
  }
}
export async function checkedGuestCart(signal?: AbortSignal) {
  const rows = guestCart();
  const updates = new Map<number, Row>();
  for (let start = 0; start < rows.length; start += cartLimit) {
    const batch = rows.slice(start, start + cartLimit).map(({ id, quantity }) => ({ id, quantity }));
    const result = await api<Row[]>('/cart/availability', 'POST', batch, true, signal);
    for (const item of result) updates.set(item.id, item);
  }
  return rows.map(row => {
    const update = updates.get(row.id);
    return { ...row, reason: update ? update.reason : 'Unavailable', price: update?.price ?? row.price };
  });
}
async function mergeGuestCart(signal: AbortSignal) {
  const guest = guestCart();
  if (!guest.length) return;
  const current = await api<Row[]>('/cart', 'GET', undefined, false, signal);
  for (const item of guest) {
    signal.throwIfAborted();
    const quantity = Math.max(item.quantity, current.find(row => row.id === item.id)?.quantity ?? 0);
    try { await api('/cart/'+item.id,'PUT',{quantity,imported:true},false,signal); setGuest(item.id,0); } catch { /* Keep unavailable items in the guest cart. */ }
  }
}
export const message = (e: unknown) => e instanceof Error ? e.message : String(e);
