<script lang="ts">
  import { tick, untrack, type Snippet } from 'svelte';
  import { api, downloadReturnMedia, label, message, money, params, type Row } from './api';
  import { CursorPager } from './cursor-pager.svelte';

  type Value = string | number | boolean | string[] | null;
  type Version = {
    id: string;
    record_id: number;
    valid_from: string;
    valid_until: string | null;
    snapshot: Record<string, Value>;
  };
  let { path, title, kind, revision, actions, anchor }: {
    path: string;
    title: string;
    kind?: string;
    revision: number;
    actions?: Snippet<[Row]>;
    anchor?: string;
  } = $props();
  const dialogTitle = $props.id();

  const pager = new CursorPager<Version, string>();
  let current = $derived(pager.items[0]?.valid_until === null ? pager.items[0] : undefined);
  let selected = $state.raw<Version | null>(null);
  let dialog: HTMLDialogElement;
  let downloadError = $state('');
  let previous = $derived(selected && pager.items.find(version =>
    version.record_id === selected?.record_id && version.valid_until === selected.valid_from
    && version.id !== selected.id));

  function load(endpoint: string, category?: string) {
    return pager.reset((cursor, signal) => api(endpoint + params({ kind: category, cursor }), 'GET', undefined, true, signal));
  }

  $effect(() => {
    const endpoint = path;
    const category = kind;
    void revision;
    untrack(() => void load(endpoint, category));
    return () => pager.cancel();
  });

  const timestamps = new Intl.DateTimeFormat('en-ID', { dateStyle: 'medium', timeStyle: 'medium' });
  const formatTime = (value: string) => timestamps.format(new Date(value));
  const fieldLabels: Record<string, string> = {
    actor_id: 'Recorded actor ID', active: 'Shop access', admin: 'Administrator',
    updated_by: 'Updated by (user ID)', paid_by: 'Paid by (user ID)', deleted: 'Hidden',
    enabled: 'Available', grams: 'Weight', url: 'Tracking URL', media: 'Attachments',
    checkout_no: 'Checkout number', created_at: 'Created', updated_at: 'Recorded update',
  };
  const fieldLabel = (field: string) => fieldLabels[field]
    ?? label(field.replace(/_id$/, ' ID').replaceAll('_', ' '));
  const moneyFields = new Set(['price', 'subtotal', 'shipping', 'tax', 'total']);
  const booleanFields = new Set(['active', 'admin', 'deleted', 'enabled', 'guest', 'suspended']);
  function formatField(field: string, value: Value) {
    if (value === null || value === '' || (Array.isArray(value) && !value.length)) return '—';
    if (moneyFields.has(field)) return money(Number(value));
    if (booleanFields.has(field)) return value ? 'Yes' : 'No';
    if (field === 'status' || field === 'payment' || field === 'kind') return label(String(value));
    if (field.endsWith('_at') && typeof value === 'string') return formatTime(value.replace(' ', 'T') + 'Z');
    if (field === 'grams') return value + ' g';
    return String(value);
  }
  function summary(version: Version) {
    const row = version.snapshot;
    if (row.price != null) return [row.name, row.subvariant, money(Number(row.price)), row.stock + ' in stock', row.enabled ? '' : 'Disabled'].filter(Boolean).join(' · ');
    if (row.rating != null) return row.rating + '/5' + (row.deleted ? ' · Hidden' : '');
    if (row.payment) return label(String(row.payment)) + ' · ' + money(Number(row.total));
    if (row.status) return [row.name, label(String(row.status))].filter(Boolean).join(' · ');
    if (row.active != null) return row.active ? 'Active access' : 'Inactive access';
    if (row.suspended != null) return [row.name, row.suspended ? 'Suspended' : 'Active'].join(' · ');
    return row.name ?? 'Return evidence';
  }
  async function view(version: Version) {
    selected = version;
    downloadError = '';
    await tick();
    dialog.showModal();
  }
</script>

{#snippet fields(version: Version, baseline?: Version)}
  <dl class="snapshot-fields">
    {#each Object.entries(version.snapshot) as [field, value] (field)}
      <dt>{fieldLabel(field)}</dt>
      <dd>
        {#if Array.isArray(value) && value.length}
          <ul>
            {#each value as url, index (index)}
              <li>
                {#if url.startsWith('/api/return-media/')}
                  <button type="button" onclick={() => {
                    downloadError = '';
                    void downloadReturnMedia(url).catch(error => downloadError = message(error));
                  }}>Download attachment {index + 1}</button>
                {:else}
                  {url}
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          {formatField(field, value)}
        {/if}
        {#if baseline && JSON.stringify(value) !== JSON.stringify(baseline.snapshot[field])}
          <small class="changed">Previously {formatField(field, baseline.snapshot[field])}</small>
        {/if}
      </dd>
    {/each}
  </dl>
{/snippet}

{#if actions && current}
  <section class="panel section">
    <div class="split">
      <h2>{current.snapshot.name || 'Current details'}</h2>
      <div class="actions">{@render actions({ ...current.snapshot, id: current.record_id })}</div>
    </div>
    {@render fields(current)}
  </section>
{/if}

<section id={anchor} class="panel section">
  <h2>{title}</h2>
  {#if pager.loading && !pager.items.length}
    <p class="inline-empty" role="status">Loading history…</p>
  {:else if !pager.items.length && !pager.error}
    <p class="inline-empty">No versions recorded.</p>
  {/if}

  {#if pager.items.length}
    <div class="table-scroll">
      <table aria-label={title}>
        <thead><tr><th scope="col">Record</th><th scope="col">Summary</th><th scope="col">Valid from</th><th scope="col">Valid until</th><th scope="col">Actions</th></tr></thead>
        <tbody>
          {#each pager.items as version (version.id)}
            <tr>
              <td>#{version.record_id}</td>
              <td>{summary(version)}</td>
              <td><time datetime={version.valid_from} title={version.valid_from}>{formatTime(version.valid_from)}</time></td>
              <td>
                {#if version.valid_until}
                  <time datetime={version.valid_until} title={version.valid_until}>{formatTime(version.valid_until)}</time>
                {:else}
                  <span class="badge">Current</span>
                {/if}
              </td>
              <td class="row-actions"><button type="button" onclick={() => view(version)}>View version</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  {#if pager.error}
    <p class="error" role="alert">{pager.error}</p>
    <button type="button" onclick={() => pager.cursor ? pager.next() : load(path, kind)}>Retry</button>
  {:else if pager.cursor}
    <button type="button" disabled={pager.loading} onclick={() => pager.next()}>{pager.loading ? 'Loading…' : 'Load older versions'}</button>
  {/if}
</section>

<dialog class="dialog history-dialog" aria-labelledby={dialogTitle} bind:this={dialog} onclose={() => selected = null}>
  {#if selected}
    <div class="split">
      <h2 id={dialogTitle}>Version of record #{selected.record_id}</h2>
      <button type="button" onclick={() => dialog.close()}>Close</button>
    </div>
    <p class="muted">
      <time datetime={selected.valid_from}>{formatTime(selected.valid_from)}</time>
      {#if selected.valid_until}
        to <time datetime={selected.valid_until}>{formatTime(selected.valid_until)}</time>
      {:else}
        · Current
      {/if}
    </p>
    {@render fields(selected, previous ?? undefined)}
    {#if downloadError}<p class="error" role="alert">{downloadError}</p>{/if}
  {/if}
</dialog>

<style>
  .history-dialog { width: min(92vw, 640px); }
  .history-dialog::backdrop { background: #03201977; }
  time { white-space: nowrap; }
  .snapshot-fields { display: grid; grid-template-columns: minmax(7rem, 1fr) minmax(0, 3fr); gap: 0; margin: 12px 0 0; }
  dt, dd { padding: 8px 4px; border-top: 1px solid var(--line); }
  dt { font-weight: 700; }
  dd { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  dd ul { margin: 0; padding-left: 18px; }
  .changed { display: block; margin-top: 4px; }
</style>
