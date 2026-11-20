<script lang="ts">
  let { name }: { name: string } = $props();
  const colors = [
    ['#dcefe8', '#146353'],
    ['#e7edf5', '#30577c'],
    ['#f4e9dc', '#825226'],
    ['#eae5f2', '#664884'],
    ['#f1e5e7', '#864959'],
  ];
  let words = $derived(name.match(/[\p{L}\p{N}]+/gu) ?? []);
  let initials = $derived((words.length > 1 ? words.slice(0, 3).map(word => Array.from(word)[0]).join('') : Array.from(words[0] ?? '—').slice(0, 3).join('')).toLocaleUpperCase('id-ID'));
  let color = $derived(colors[Array.from(name).reduce((total, character) => (total * 31 + character.codePointAt(0)!) % colors.length, 0)]);
</script>

<span class="product-initials" style:background={color[0]} style:color={color[1]} aria-hidden="true">{initials}</span>
