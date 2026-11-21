<script lang="ts">
  import { onMount } from 'svelte';
  import { ChevronDown, Headphones, House, LayoutGrid, Search, Shirt, Sparkles, Volleyball } from '@lucide/svelte';
  import { api, params } from './api';

  let { category, storefrontHome }: { category: string | null; storefrontHome: boolean } = $props();

  const shortcutGroups = [
    { icon: Headphones, names: ['Electronics', 'Computers & Tablets', 'Cell Phones & Accessories'] },
    { icon: Shirt, names: ['Fashion', "Women's Clothing", "Men's Clothing"] },
    { icon: House, names: ['Home & Living', 'Kitchen & Dining', 'Home Décor Products'] },
    { icon: Sparkles, names: ['Beauty', 'Beauty & Personal Care'] },
    { icon: Volleyball, names: ['Sports', 'Sports & Fitness', 'Sports & Outdoors'] }
  ];

  let names = $state.raw<string[]>([]);
  let loading = $state(true);
  let failed = $state(false);
  let open = $state(false);
  let search = $state('');
  let picker: HTMLDetailsElement;
  let trigger: HTMLElement;
  let pending: AbortController | undefined;

  const shortcuts = $derived(shortcutGroups.flatMap(({ names: choices, icon }) => {
    const name = choices.find(choice => names.includes(choice));
    return name ? [{ icon, name }] : [];
  }));
  const query = $derived(search.trim().toLocaleLowerCase());
  const filtered = $derived(query ? names.filter(name => name.toLocaleLowerCase().includes(query)) : names);
  const letterGroups = $derived.by(() => {
    const groups = new Map<string, string[]>();
    for (const name of filtered) {
      const letter = name.trim().normalize('NFD').match(/^\p{L}/u)?.[0].toLocaleUpperCase() ?? '#';
      const group = groups.get(letter);
      if (group) group.push(name);
      else groups.set(letter, [name]);
    }
    return [...groups].sort(([a], [b]) => a.localeCompare(b));
  });

  async function loadCategories() {
    pending?.abort();
    const controller = new AbortController();
    pending = controller;
    loading = true;
    failed = false;
    try {
      const entries = await api<{ name: string; kind: string }[]>('/categories', 'GET', undefined, true, controller.signal);
      if (!controller.signal.aborted) names = [...new Set(entries.filter(entry => entry.kind === 'category').map(entry => entry.name))];
    } catch {
      if (!controller.signal.aborted) failed = true;
    } finally {
      if (pending === controller) {
        pending = undefined;
        loading = false;
      }
    }
  }

  function categoryLink(name: string) {
    return '/' + params({ category: name }) + '#catalog';
  }

  function dismissOutside(event: PointerEvent) {
    if (open && event.target instanceof Node && !picker.contains(event.target)) open = false;
  }

  function dismissOnEscape(event: KeyboardEvent) {
    if (open && event.key === 'Escape') {
      event.preventDefault();
      open = false;
      trigger.focus();
    }
  }

  onMount(() => {
    void loadCategories();
    return () => pending?.abort();
  });
</script>

<svelte:window onpointerdown={dismissOutside} onkeydown={dismissOnEscape} />

<nav class="category-bar" aria-label="Shop categories">
  <div class="wrap category-nav">
    <details class="category-picker" bind:this={picker} bind:open>
      <summary bind:this={trigger}>
        <LayoutGrid size={16} />
        Categories
        <ChevronDown size={14} class="category-chevron" />
      </summary>
      {#if open}
        <div class="category-menu">
          <label class="category-filter">
            <Search size={17} />
            <input type="search" aria-label="Filter categories" placeholder="Filter categories" bind:value={search} />
          </label>
          {#if loading}
            <p class="category-message" role="status">Loading categories…</p>
          {:else if failed}
            <p class="category-message" role="status">Categories could not load.</p>
            <button class="category-retry" onclick={loadCategories}>Try again</button>
          {:else if letterGroups.length}
            <div class="category-list">
              {#each letterGroups as [letter, entries] (letter)}
                <div class="category-group">
                  <h2 class="category-letter"><span>{letter}</span></h2>
                  <ul aria-label={letter === '#' ? 'Other categories' : 'Categories starting with ' + letter}>
                    {#each entries as name (name)}
                      <li><a href={categoryLink(name)} aria-current={name === category ? 'page' : undefined} onclick={() => open = false}>{name}</a></li>
                    {/each}
                  </ul>
                </div>
              {/each}
            </div>
          {:else}
            <p class="category-message" role="status">{query ? 'No matching categories.' : 'No categories available.'}</p>
          {/if}
        </div>
      {/if}
    </details>

    <div class="category-shortcuts">
      <a href="/" aria-current={storefrontHome ? 'page' : undefined}>All products</a>
      {#each shortcuts as entry (entry.name)}
        <a href={categoryLink(entry.name)} aria-current={entry.name === category ? 'page' : undefined}>
          <entry.icon size={16} />
          {entry.name}
        </a>
      {/each}
    </div>
  </div>
</nav>

<style>
  .category-bar { border-top: 1px solid #edf0ee; }
  .category-nav, .category-shortcuts {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .category-nav { min-height: 53px; padding-block: 5px; }
  .category-picker { position: relative; flex: none; }
  .category-picker > summary {
    display: flex;
    align-items: center;
    gap: 7px;
    list-style: none;
    margin: 0;
    padding: 8px 11px;
    border-radius: 8px;
    background: var(--mint);
  }
  .category-picker > summary::-webkit-details-marker { display: none; }
  .category-picker[open] :global(.category-chevron) { transform: rotate(180deg); }
  .category-shortcuts {
    min-width: 0;
    padding: 6px;
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scroll-padding-inline: 6px;
    scrollbar-width: thin;
  }
  .category-shortcuts a {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
    border-radius: 8px;
    padding: 8px 11px;
    color: #435a50;
    font-weight: 600;
    white-space: nowrap;
  }
  .category-shortcuts a:hover, .category-list a:hover, a[aria-current="page"] {
    color: var(--green);
    background: #e6f4ec;
  }
  .category-menu {
    position: absolute;
    z-index: 4;
    inset: calc(100% + 6px) auto auto 0;
    width: min(580px, calc(100vw - 34px));
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--paper);
    box-shadow: 0 12px 35px #091e1926;
  }
  .category-filter {
    display: flex;
    align-items: center;
    gap: 7px;
    padding-inline: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: #f4f7f5;
    color: var(--muted);
  }
  .category-filter:focus-within { outline: 2px solid #52a793; outline-offset: 2px; }
  .category-filter input {
    width: 100%;
    min-width: 0;
    padding-inline: 0;
    border: 0;
    outline: none;
    background: transparent;
  }
  .category-list {
    max-height: min(52dvh, 360px);
    overflow-y: auto;
    overscroll-behavior: contain;
    margin: 8px 0 0;
    padding: 3px;
  }
  .category-group + .category-group { margin-top: 8px; }
  .category-letter {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 4px 8px;
    background: var(--paper);
  }
  .category-letter span {
    display: grid;
    place-items: center;
    min-width: 34px;
    min-height: 34px;
    padding-inline: 4px;
    border-radius: 8px;
    background: var(--mint);
    color: var(--green);
    font-size: 20px;
    font-weight: 800;
    letter-spacing: normal;
    line-height: 1;
  }
  .category-letter::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--line);
  }
  .category-group ul {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 3px 7px;
    margin: 3px 0 0;
    padding: 0;
    list-style: none;
  }
  .category-list a {
    display: block;
    padding: 8px;
    border-radius: 7px;
    color: #435a50;
    overflow-wrap: anywhere;
  }
  .category-list a[aria-current="page"] { color: var(--green); font-weight: 700; }
  .category-message { color: var(--muted); margin: 12px 4px 4px; }
  .category-retry { margin-top: 6px; }

  @media (max-width: 1020px) {
    .category-shortcuts a { padding-inline: 8px; }
  }
  @media (max-width: 620px) {
    .category-nav { min-height: 44px; padding-block: 2px; }
    .category-picker > summary { gap: 5px; padding-inline: 8px; }
    .category-shortcuts { gap: 2px; }
    .category-shortcuts a { padding-inline: 6px; }
    .category-menu { width: min(580px, calc(100vw - 24px)); }
  }
  @media (max-width: 400px) {
    .category-shortcuts a :global(svg) { display: none; }
  }
</style>
