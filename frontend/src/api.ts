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
let storage: Storage = sessionStorage.getItem('access') ? sessionStorage : localStorage.getItem('refresh') ? localStorage : sessionStorage;
export const guestSession = () => storage === localStorage && Boolean(storage.getItem('access'));
export const signedIn = () => Boolean(storage.getItem('access'));
export function save(tokens: Row, guest = guestSession()) {
  storage = guest ? localStorage : sessionStorage;
  storage.setItem('access', tokens.access);
  storage.setItem('refresh', tokens.refresh);
  if (!guest) { localStorage.removeItem('access'); localStorage.removeItem('refresh'); }
}
export function clear() { storage.removeItem('access'); storage.removeItem('refresh'); }
let refreshing: Promise<boolean> | undefined;
async function renew() {
  const refresh = storage.getItem('refresh');
  if (!refresh) return false;
  refreshing ??= fetch('/api/auth/refresh', {method:'POST', headers:{'Content-Type':'application/json'},body:JSON.stringify({refresh})})
    .then(async response => {
      if (response.ok) {save(await response.json(),storage === localStorage); return true;}
      if (response.status === 401) clear();
      return false;
    }).finally(() => { refreshing = undefined; });
  return refreshing;
}
async function request(path: string, method = 'GET', body?: unknown, retry = true, keepalive = false, signal?: AbortSignal): Promise<Response> {
  const headers = new Headers();
  const token = storage.getItem('access');
  if (token && !path.startsWith('/auth/login') && !path.startsWith('/auth/register')) headers.set('Authorization', 'Bearer ' + token);
  if (body !== undefined && !(body instanceof FormData)) headers.set('Content-Type', 'application/json');
  const response = await fetch('/api' + path, {method, headers, keepalive, signal, body: body instanceof FormData ? body : body === undefined ? undefined : JSON.stringify(body)});
  if (response.status === 401 && retry && !path.startsWith('/auth/login') && await renew()) return request(path, method, body, false, keepalive, signal);
  if (!response.ok) {
    const data = await response.json().catch(() => ({}));
    throw new Error(data.error ?? 'Could not connect to Tosca Market');
  }
  return response;
}
export async function api<T = Row>(path: string, method = 'GET', body?: unknown, retry = true, signal?: AbortSignal): Promise<T> {
  return (await request(path, method, body, retry, false, signal)).json() as Promise<T>;
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
    const products: number[] = previous?.day === day && Array.isArray(previous.products) ? previous.products : [];
    if (products.includes(productId)) return false;
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

export async function recommendedProducts(productId?: number): Promise<Row[]> {
  const { enabled, items } = await api<{ enabled: boolean; items: Row[] }>('/recommendations', 'POST', {
    product_id: productId,
    signals: currentGuestSignals(),
  });
  return enabled ? items : [];
}

export function exploreRecommendations() {
  const signals = currentGuestSignals();
  return (cursor: string | null) => api<{ enabled: boolean; items: (Row & { id: number })[]; cursor: string | null }>(
    '/recommendations/explore', 'POST', cursor === null ? { signals } : { cursor },
  );
}

export async function logout() {
  const refresh = storage.getItem('refresh');
  if (refresh) await api('/auth/logout','POST',{refresh},false);
  clear();
}
export async function upload(file: File, privateMedia = false) {
  const form = new FormData(); form.append('file',file);
  return (await api('/upload'+(privateMedia?'?private=true':''),'POST',form)).url as string;
}
export async function downloadReturnMedia(url: string) {
  if (!url.startsWith('/api/return-media/')) throw new Error('Attachment unavailable');
  const response = await request(url.slice(4));
  const address = URL.createObjectURL(await response.blob());
  const link = document.createElement('a');
  link.href = address;
  link.download = 'return-evidence.' + url.slice(url.lastIndexOf('.')+1);
  link.click();
  setTimeout(() => URL.revokeObjectURL(address), 1000);
}
type GuestItem = Row & { id:number; quantity:number; created_at:string };
export function guestCart(): GuestItem[] { try { return JSON.parse(localStorage.getItem('tosca-cart') ?? '[]'); } catch { return []; } }
export function setGuest(id: number, quantity: number, item?: Row) {
  const cart = guestCart();
  const old = cart.find(row => row.id === id);
  if (quantity && (item || old)) cart.push({ ...old, ...item, id, quantity, created_at: old?.created_at ?? new Date().toISOString() });
  localStorage.setItem('tosca-cart',JSON.stringify(cart.filter((row,index) => row.id !== id || (quantity > 0 && index === cart.length - 1))));
  if (quantity > (old?.quantity ?? 0) && (!signedIn() || guestSession())) {
    recordGuestSignal(item?.product_id ?? old?.product_id, 'cart');
  }
}
export async function checkedGuestCart() {
  const rows = guestCart();
  if (!rows.length) return rows;
  const updates = await api<Row[]>('/cart/availability','POST',rows.map(({id,quantity})=>({id,quantity})));
  return rows.map(row => {
    const update=updates.find(item => item.id === row.id);
    return {...row,reason:update?.reason ?? null,price:update?.price ?? row.price};
  });
}
export async function mergeGuestCart() {
  const guest = guestCart();
  if (!guest.length) return;
  const current = await api<Row[]>('/cart');
  for (const item of guest) {
    const quantity = Math.max(item.quantity, current.find(row => row.id === item.id)?.quantity ?? 0);
    try { await api('/cart/'+item.id,'PUT',{quantity,imported:true}); setGuest(item.id,0); } catch { /* Keep unavailable items in the guest cart. */ }
  }
}
export const message = (e: unknown) => e instanceof Error ? e.message : String(e);
