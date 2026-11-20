<script lang="ts">
  import type { SearchHit } from './api';

  let { kind }: { kind: SearchHit['kind'] } = $props();
</script>

<span class="search-kind" class:shop-kind={kind === 'shop'}>{kind === 'shop' ? 'Shop' : 'Product'}</span>
