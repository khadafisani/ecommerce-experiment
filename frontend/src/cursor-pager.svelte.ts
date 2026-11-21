import { message } from './api';

type CursorPage<T, C> = { items: T[]; cursor: C | null };
type FetchPage<T, C> = (cursor: C | null) => Promise<CursorPage<T, C>>;

export class CursorPager<T extends { id: number | string }, C extends number | string = number> {
  items = $state.raw<T[]>([]);
  cursor = $state<C | null>(null);
  loading = $state(false);
  error = $state('');
  private fetchPage: FetchPage<T, C> | undefined;
  private revision = 0;

  reset(fetchPage: FetchPage<T, C>) {
    this.fetchPage = fetchPage;
    this.items = [];
    this.cursor = null;
    return this.load(fetchPage, null, ++this.revision);
  }

  next() {
    if (this.loading || this.cursor === null || !this.fetchPage) return;
    return this.load(this.fetchPage, this.cursor, this.revision);
  }

  private async load(fetchPage: FetchPage<T, C>, cursor: C | null, revision: number) {
    this.loading = true;
    this.error = '';
    try {
      const result = await fetchPage(cursor);
      if (revision !== this.revision) return;
      const previousIds = new Set(this.items.map(item => item.id));
      this.items = [...this.items, ...result.items.filter(item => !previousIds.has(item.id))];
      this.cursor = result.cursor === cursor ? null : result.cursor;
    } catch (error) {
      if (revision === this.revision) this.error = message(error);
    } finally {
      if (revision === this.revision) this.loading = false;
    }
  }
}
