<script lang="ts">
  import { ArrowUpRight, Store } from '@lucide/svelte';
  import { money, searchHitPath, type Row } from './api';
  import ProductInitials from './ProductInitials.svelte';
  import ResponsiveImage from './ResponsiveImage.svelte';
  import SearchKindBadge from './SearchKindBadge.svelte';
  let { items, showKinds = false }: { items: Row[]; showKinds?: boolean } = $props();
</script>
<div class="product-grid">
  {#each items as product (searchHitPath({kind: product.kind ?? 'product', id: product.id}))}
    {@const kind = product.kind === 'shop' ? 'shop' : 'product'}
    <a class="product-card" href={searchHitPath({kind, id: product.id})}>
      <div class="product-image" class:shop-result={kind === 'shop'}>
        {#if kind === 'shop'}<Store size={42} aria-hidden="true"/>
        {:else if product.image}
          <ResponsiveImage src={product.image} alt={product.name}
            sizes="(max-width: 620px) calc(50vw - 17px), (max-width: 900px) calc(25vw - 16px), (max-width: 1320px) calc(25vw - 28px), 302px" />
        {:else}<ProductInitials name={product.name}/>{/if}
        {#if showKinds}<SearchKindBadge {kind}/>{/if}
        <span class="product-arrow"><ArrowUpRight size={17}/></span>
      </div>
      <div class="product-info">
        {#if kind === 'product'}<small><Store size={13}/> {product.shop}</small>{/if}
        <strong>{product.name}</strong>
        {#if kind === 'product'}<b>{money(product.lowest_price)}</b>{/if}
      </div>
    </a>
  {/each}
</div>
