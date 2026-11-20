<script lang="ts">
  let { cursor, loading, error, next }: {
    cursor: number | null;
    loading: boolean;
    error: string;
    next: () => void;
  } = $props();

  const supportsObserver = typeof IntersectionObserver !== 'undefined';

  function observe(node: Element) {
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      observer.disconnect();
      next();
    }, { rootMargin: '0px 0px 240px 0px' });
    observer.observe(node);
    return () => observer.disconnect();
  }
</script>

{#if cursor !== null}
  <div class="more">
    {#if loading}
      <p class="muted" role="status">Loading more…</p>
    {:else if error}
      <p class="error" role="alert">{error}</p>
      <button onclick={next}>Retry loading more</button>
    {:else if supportsObserver}
      {#key cursor}
        <div class="cursor-sentinel" aria-hidden="true" {@attach observe}></div>
      {/key}
    {:else}
      <button onclick={next}>Load more</button>
    {/if}
  </div>
{/if}
