<script lang="ts" module>
  const tiers = ['sm', 'md', 'lg', 'xlg'] as const;
  const uploadDensities = {
    thumbnail: ['sm'],
    card: ['sm', 'md', 'lg'],
    detail: ['md', 'xlg'],
  } as const;

  function staticWidths(src: string) {
    if (src === '/catalog/hero.jpg') return [320, 640, 960, 1200];
    if (/^\/catalog\/(bag|beauty|coffee|headphones|lamp|mug|running|sneakers)\.jpg$/.test(src)) return [160, 320, 480, 580];
    if (/^\/payment\/(bca|bri|mandiri|qris)\.svg$/.test(src)) return [96, 192, 288, 384];
  }

  function variant(src: string, tier: typeof tiers[number]) {
    return `${src.slice(0, src.lastIndexOf('.'))}-${tier}.avif`;
  }
</script>

<script lang="ts">
  let {
    src, alt, sizes, class: className, uploadSize = 'card', loading = 'lazy', fetchpriority,
  }: {
    src: string;
    alt: string;
    sizes: string;
    class?: string;
    uploadSize?: keyof typeof uploadDensities;
    loading?: 'lazy' | 'eager';
    fetchpriority?: 'high' | 'low' | 'auto';
  } = $props();

  const staticSrcset = $derived(staticWidths(src)?.map((width, index) => `${variant(src, tiers[index])} ${width}w`).join(', '));
  const uploaded = $derived(/^\/uploads\/[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}\.avif$/i.test(src));
  const uploadTiers = $derived(uploadDensities[uploadSize]);
  const uploadSrcset = $derived(uploaded && uploadTiers.length > 1
    ? uploadTiers.map((tier, index) => `${variant(src, tier)} ${index + 1}x`).join(', ')
    : undefined);
</script>

{#if staticSrcset}
  <picture>
    <source type="image/avif" srcset={staticSrcset} {sizes} />
    <img {src} {alt} {loading} {fetchpriority} class={className} />
  </picture>
{:else}
  <img src={uploaded ? variant(src, uploadTiers[0]) : src} srcset={uploadSrcset} {alt} {loading} {fetchpriority} class={className} />
{/if}

<style>
  picture { display: contents; }
</style>
