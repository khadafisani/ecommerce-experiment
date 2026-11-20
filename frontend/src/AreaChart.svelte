<script lang="ts">
  import { money } from './api';

  type Point = { label: string; value: number | string };
  let { title, points, currency = false }: { title: string; points: Point[]; currency?: boolean } = $props();
  const chartId = $props.id();
  const width = 1000;
  const height = 220;
  const count = new Intl.NumberFormat('id-ID');
  const compact = new Intl.NumberFormat('id-ID', { notation: 'compact', maximumFractionDigits: 1 });
  const dayFormat = new Intl.DateTimeFormat('id-ID', { day: 'numeric', month: 'short', timeZone: 'UTC' });
  const fullDay = new Intl.DateTimeFormat('id-ID', { dateStyle: 'medium', timeZone: 'UTC' });
  const monthFormat = new Intl.DateTimeFormat('id-ID', { month: 'short', year: '2-digit', timeZone: 'UTC' });
  const fullMonth = new Intl.DateTimeFormat('id-ID', { month: 'long', year: 'numeric', timeZone: 'UTC' });

  let selection = $state.raw<{ source: Point[]; index: number } | null>(null);
  let activeIndex = $derived(selection?.source === points ? selection.index : -1);
  let values = $derived(points.map(point => Number(point.value)));
  let step = $derived(tickSize(Math.max(0, ...values)));
  let ticks = $derived([4, 3, 2, 1, 0].map(part => part * step));
  let series = $derived.by(() => {
    const dates = points.map(point => bucketTime(point.label));
    const span = dates.at(-1)! - dates[0];
    return points.map((point, index) => ({
      ...point,
      value: values[index],
      x: points.length === 1 ? width / 2 : span > 0 ? (dates[index] - dates[0]) / span * width : index / (points.length - 1) * width,
      y: height * (1 - values[index] / (step * 4)),
    }));
  });
  let shown = $derived(series[activeIndex] ?? series.at(-1)!);
  let line = $derived(series.map(point => point.x + ',' + point.y).join(' '));
  let dates = $derived.by(() => {
    const first = series[0];
    const last = series.at(-1);
    if (!first || first === last) return series;
    const middle = series.reduce((closest, point) => Math.abs(point.x - width / 2) < Math.abs(closest.x - width / 2) ? point : closest, first);
    return middle.x > width * .28 && middle.x < width * .72 ? [first, middle, last!] : [first, last!];
  });

  function tickSize(max: number) {
    if (max <= 4) return 1;
    const target = max / 4;
    const place = 10 ** Math.floor(Math.log10(target));
    return Math.ceil(([1, 2, 2.5, 5, 10].find(step => step * place >= target) ?? 10) * place);
  }

  function bucketTime(label: string) {
    const week = /^(\d{4})-W(\d{2})$/.exec(label);
    if (week) {
      const january4 = new Date(Date.UTC(Number(week[1]), 0, 4));
      return january4.getTime() + ((Number(week[2]) - 1) * 7 - (january4.getUTCDay() + 6) % 7) * 86400000;
    }
    return Date.parse(label.length === 4 ? label + '-01-01' : label.length === 7 ? label + '-01' : label);
  }

  function dateLabel(label: string, full = false) {
    if (label.length === 4) return label;
    const time = bucketTime(label);
    if (!Number.isFinite(time)) return label;
    if (label.includes('-W') && full) return 'W' + label.slice(-2) + ' · ' + fullDay.format(time);
    return (label.length === 7 ? (full ? fullMonth : monthFormat) : (full ? fullDay : dayFormat)).format(time);
  }

  function valueLabel(value: number, axis = false) {
    if (currency) return axis && value >= 1000 ? 'Rp\u00a0' + compact.format(value) : money(value);
    return axis && value >= 10000 ? compact.format(value) : count.format(value);
  }

  function inspect(event: PointerEvent) {
    const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const x = (event.clientX - bounds.left) / bounds.width * width;
    const closest = series.reduce((index, point, next) =>
      Math.abs(point.x - x) < Math.abs(series[index].x - x) ? next : index, 0);
    if (closest !== activeIndex) selection = { source: points, index: closest };
  }

  function handleKey(event: KeyboardEvent) {
    const last = series.length - 1;
    const current = activeIndex < 0 ? last : activeIndex;
    let next: number;
    switch (event.key) {
      case 'ArrowLeft':
      case 'ArrowDown': next = Math.max(0, current - 1); break;
      case 'ArrowRight':
      case 'ArrowUp': next = Math.min(last, current + 1); break;
      case 'Home': next = 0; break;
      case 'End': next = last; break;
      case 'Escape': selection = null; return;
      default: return;
    }
    event.preventDefault();
    selection = { source: points, index: next };
  }
</script>

<div class="chart-readout" aria-hidden="true">
  <strong>{valueLabel(shown.value)}</strong>
  <span>{dateLabel(shown.label, true)}</span>
</div>
<div class="market-chart">
  <div class="market-chart-plot" role="slider" tabindex="0"
    aria-label={title + ' chart, use the arrow keys to inspect values'}
    aria-valuemin={1} aria-valuemax={series.length} aria-valuenow={Math.max(1, (activeIndex < 0 ? series.length - 1 : activeIndex) + 1)}
    aria-valuetext={dateLabel(shown.label, true) + ', ' + valueLabel(shown.value)}
    onkeydown={handleKey} onpointerdown={inspect} onpointermove={inspect}
    onpointerleave={(event) => { if (event.pointerType !== 'touch') selection = null; }}
    onblur={() => selection = null}
  >
    <svg viewBox={'0 0 '+width+' '+height} preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient id={chartId} x2="0" y2="1">
          <stop stop-color="#2da98a" stop-opacity=".28" />
          <stop offset="1" stop-color="#2da98a" stop-opacity=".02" />
        </linearGradient>
      </defs>
      {#each ticks as tick,index (tick)}
        <line class="market-chart-grid" x1="0" x2={width} y1={index * height / 4} y2={index * height / 4}/>
      {/each}
      {#each dates as point (point.label)}
        {#if point.x > 0 && point.x < width}<line class="market-chart-grid" x1={point.x} x2={point.x} y1="0" y2={height}/>{/if}
      {/each}
      {#if series.length > 1}
        <polygon points={series[0].x+','+height+' '+line+' '+series.at(-1)!.x+','+height} fill={'url(#'+chartId+')'}/>
        <polyline class="market-chart-line" points={line}/>
      {/if}
      {#if activeIndex >= 0}
        <line class="market-chart-guide" x1={shown.x} x2={shown.x} y1="0" y2={height}/>
        <line class="market-chart-guide" x1="0" x2={width} y1={shown.y} y2={shown.y}/>
      {/if}
    </svg>
    <span class="market-chart-dot" style:left={shown.x / width * 100 + '%'} style:top={shown.y / height * 100 + '%'}></span>
  </div>
  <div class="market-chart-values" aria-hidden="true">
    {#each ticks as tick,index (tick)}<span style:top={index * 25 + '%'}>{valueLabel(tick, true)}</span>{/each}
  </div>
  <div class="market-chart-dates" aria-hidden="true">
    {#each dates as point (point.label)}
      <span class:first={point.x===0} class:last={point.x===width} style:left={point.x / width * 100 + '%'}>{dateLabel(point.label)}</span>
    {/each}
  </div>
</div>
