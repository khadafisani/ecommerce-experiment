<script lang="ts">
  import AreaChart from './AreaChart.svelte';
  let { title, points, pie = false, currency = false }: { title: string; points: { label: string; value: number | string }[]; pie?: boolean; currency?: boolean } = $props();
  const palette = ['#137664','#ffbb66','#cf6354','#87968e','#74c6b5'];
  let total = $derived(points.reduce((sum,p)=>sum+Number(p.value),0));
  let gradient = $derived.by(()=>{
    let end=0;
    return points.map((p,i)=>{const start=end; end+=100*Number(p.value)/total; return palette[i%palette.length]+' '+start+'% '+end+'%';}).join(',');
  });
</script>
<section class="panel chart">
  <h3>{title}</h3>
  {#if !points.length}<div class="inline-empty">No data for this period.</div>
  {:else if pie}
    <div class="pie-layout"><div class="pie" style:background={'conic-gradient('+gradient+')'} aria-hidden="true"></div><ul>{#each points as p,i (p.label)}<li><i style:background={palette[i%palette.length]}></i>{p.label} <strong>{p.value}</strong></li>{/each}</ul></div>
  {:else}
    <AreaChart {title} {points} {currency}/>
  {/if}
</section>
