<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import { ArrowRight, Search, Store } from '@lucide/svelte';
  import { groupSearchHits, money, params, searchHitPath, type SearchGroups, type SearchHit, type SearchResponse } from './api';
  import SearchKindBadge from './SearchKindBadge.svelte';


  let { initialQuery = '' }: { initialQuery?: string } = $props();
  let query = $derived(initialQuery);
  let groups = $state.raw<SearchGroups>({ products: [], shops: [] });
  let matches = $derived([...groups.products, ...groups.shops]);
  let phase = $state<'idle' | 'loading' | 'ready' | 'error'>('idle');
  let focused = $state(false);
  let activeIndex = $state(-1);
  let visible = $derived(focused && phase !== 'idle');
  let hasOptions = $derived(visible && matches.length > 0);
  const listId = 'market-search-suggestions';
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: AbortController | undefined;

  function cancel() {
    clearTimeout(timer);
    pending?.abort();
    pending = undefined;
  }

  function close() {
    cancel();
    phase = 'idle';
    activeIndex = -1;
  }

  onDestroy(cancel);

  function queueSearch() {
    cancel();
    const term = query.trim();
    activeIndex = -1;
    groups = { products: [], shops: [] };
    if (!focused || term.length < 2) {
      phase = 'idle';
      return;
    }
    phase = 'loading';
    timer = setTimeout(() => fetchMatches(term), 220);
  }

  async function fetchMatches(term: string) {
    const controller = new AbortController();
    pending = controller;
    try {
      const response = await fetch('/api/search' + params({ q: term }), { signal: controller.signal });
      if (!response.ok) throw new Error('Search unavailable');
      const result: SearchResponse = await response.json();
      if (controller.signal.aborted) return;
      groups = groupSearchHits(result);
      phase = 'ready';
    } catch {
      if (!controller.signal.aborted) phase = 'error';
    } finally {
      if (pending === controller) pending = undefined;
    }
  }

  function handleKey(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (event.key === 'Escape' && visible) {
      event.preventDefault();
      close();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      if (!visible) {
        queueSearch();
        return;
      }
      if (!matches.length) return;
      event.preventDefault();
      if (activeIndex < 0) activeIndex = event.key === 'ArrowDown' ? 0 : matches.length - 1;
      else activeIndex = (activeIndex + (event.key === 'ArrowDown' ? 1 : -1) + matches.length) % matches.length;
      void tick().then(() => document.getElementById(optionId(activeIndex))?.scrollIntoView({ block: 'nearest' }));
    } else if (event.key === 'Enter' && hasOptions && activeIndex >= 0) {
      event.preventDefault();
      location.assign(searchHitPath(matches[activeIndex]));
    }
  }

  function handleFocusOut(event: FocusEvent) {
    if (event.relatedTarget instanceof Node && event.currentTarget instanceof Node && event.currentTarget.contains(event.relatedTarget)) return;
    focused = false;
    close();
  }

  function optionId(index: number) { return listId + '-' + index; }
</script>

{#snippet suggestionGroup(label: string, hits: SearchHit[], offset: number)}
  {#if hits.length}
    <div role="group" aria-label={label}>
      <div class="suggestion-heading" aria-hidden="true">{label}</div>
      {#each hits as match, index (searchHitPath(match))}
        {@const position = offset + index}
        <a href={searchHitPath(match)} id={optionId(position)} role="option" aria-selected={activeIndex === position}
           tabindex="-1" class="search-match" onmousedown={(event) => { if (event.button === 0) event.preventDefault(); }}>
          {#if match.kind === 'shop'}<Store size={16} aria-hidden="true"/>{/if}
          <span class="search-match-name"><strong>{match.name}</strong>{#if match.shop}<small>{match.shop}</small>{/if}</span>
          <span class="search-match-details">
            <SearchKindBadge kind={match.kind}/>
            {#if match.lowest_price != null}<small>{money(match.lowest_price)}</small>{/if}
          </span>
        </a>
      {/each}
    </div>
  {/if}
{/snippet}

<form class="search-container" role="search" action="/search" onfocusout={handleFocusOut}>
  <div class="search">
    <Search size={19} aria-hidden="true"/>
    <input name="q" type="search" role="combobox" aria-label="Search products and shops"
      aria-autocomplete="list" aria-expanded={hasOptions} aria-controls={hasOptions ? listId : undefined}
      aria-activedescendant={hasOptions && activeIndex >= 0 ? optionId(activeIndex) : undefined}
      placeholder="Search products and shops…" value={query} maxlength={150} required autocomplete="off"
      onfocus={() => { focused = true; queueSearch(); }}
      oninput={(event) => { query = event.currentTarget.value; if (!(event instanceof InputEvent && event.isComposing)) queueSearch(); }}
      oncompositionstart={close}
      oncompositionend={(event) => { query = event.currentTarget.value; queueSearch(); }}
      onkeydown={handleKey}
    >
    <button aria-label="Submit search"><ArrowRight size={17}/></button>
  </div>
  {#if visible}
    <div class="search-popup">
      {#if hasOptions}
        <div id={listId} role="listbox" aria-label="Search suggestions">
          {@render suggestionGroup('Products', groups.products, 0)}
          {@render suggestionGroup('Shops', groups.shops, groups.products.length)}
        </div>
      {:else}
        <p class="search-status" role="status">{phase === 'loading' ? 'Searching…' : phase === 'error' ? 'Suggestions unavailable. Press Enter to search.' : 'No matching products or shops.'}</p>
      {/if}
    </div>
  {/if}
</form>

<style>
  .suggestion-heading { padding: 6px 9px 2px; color: var(--muted); font-size: 12px; font-weight: 700; }
</style>
