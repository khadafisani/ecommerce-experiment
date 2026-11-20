///<reference types="svelte" />
;
import type { SearchHit } from './api';

;type $$ComponentProps =  { kind: SearchHit['kind'] };function $$render() {

  

  let { kind }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
;
async () => {

 { svelteHTML.createElement("span", {  "class":`search-kind`,});kind === 'shop';kind === 'shop' ? 'Shop' : 'Product'; }
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const SearchKindBadge__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type SearchKindBadge__SvelteComponent_ = ReturnType<typeof SearchKindBadge__SvelteComponent_>;
/*Ωignore_endΩ*/export default SearchKindBadge__SvelteComponent_;