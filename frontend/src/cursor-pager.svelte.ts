import { message } from './api';

type CursorPage<T, C> = { items: T[]; cursor: C | null };
type FetchPage<T, C> = (cursor: C | null, signal: AbortSignal) => Promise<CursorPage<T, C>>;

export class CursorPager<T extends { id: number | string }, C extends number | string = number> {
  items = $state.raw<T[]>([]);
  cursor = $state<C | null>(null);
  loading = $state(false);
  error = $state('');
  private fetchPage: FetchPage<T, C> | undefined;
  private controller: AbortController | undefined;

  reset(fetchPage: FetchPage<T, C>) {
    this.cancel();
    this.fetchPage = fetchPage;
    this.items = [];
    this.cursor = null;
    return this.load(fetchPage, null);
  }

  next() {
    if (this.loading || this.cursor === null || !this.fetchPage) return;
    return this.load(this.fetchPage, this.cursor);
  }

  cancel() {
    this.controller?.abort();
    this.controller = undefined;
    this.loading = false;
  }

  private async load(fetchPage: FetchPage<T, C>, cursor: C | null) {
    const controller = new AbortController();
    this.controller = controller;
    this.loading = true;
    this.error = '';
    try {
      const result = await fetchPage(cursor, controller.signal);
      if (controller.signal.aborted) return;
      const previousIds = new Set(this.items.map(item => item.id));
      const newItems = [];
      for (const item of result.items) {
        if (previousIds.has(item.id)) continue;
        previousIds.add(item.id);
        newItems.push(item);
      }
      this.items = [...this.items, ...newItems];
      this.cursor = result.cursor === cursor ? null : result.cursor;
    } catch (error) {
      if (!controller.signal.aborted) this.error = message(error);
    } finally {
      if (this.controller === controller) {
        this.controller = undefined;
        this.loading = false;
      }
    }
  }
}
