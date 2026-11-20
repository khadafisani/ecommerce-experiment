///<reference types="svelte" />
;
;type $$ComponentProps =  { name: string };function $$render() {

  let { name }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
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
;
async () => {

 { svelteHTML.createElement("span", {     "class":`product-initials`,"aria-hidden":`true`,});__sveltets_2_ensureType(String, Number, color[0]);__sveltets_2_ensureType(String, Number, color[1]);initials; }
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const ProductInitials__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type ProductInitials__SvelteComponent_ = ReturnType<typeof ProductInitials__SvelteComponent_>;
/*Ωignore_endΩ*/export default ProductInitials__SvelteComponent_;