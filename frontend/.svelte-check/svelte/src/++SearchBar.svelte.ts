///<reference types="svelte" />
;
import { onDestroy, tick } from 'svelte';
import { ArrowRight, Search, Store } from '@lucide/svelte';
import { groupSearchHits, money, params, searchHitPath, type SearchGroups, type SearchHit, type SearchResponse } from './api';
import SearchKindBadge from './SearchKindBadge.svelte';

;type $$ComponentProps =  { initialQuery?: string };function $$render() {
 const suggestionGroup/*Ωignore_positionΩ*/ = (label: string, hits: SearchHit[], offset: number)/*Ωignore_startΩ*/: ReturnType<import('svelte').Snippet>/*Ωignore_endΩ*/ => { async ()/*Ωignore_positionΩ*/ => {
  if(hits.length){
     { svelteHTML.createElement("div", {   "role":`group`,"aria-label":label,});
       { svelteHTML.createElement("div", {   "class":`suggestion-heading`,"aria-hidden":`true`,});label; }
          for(let match of __sveltets_2_ensureArray(hits)){let index = 1;searchHitPath(match);
        const position = offset + index;
         { svelteHTML.createElement("a", {             "href":searchHitPath(match),"id":optionId(position),"role":`option`,"aria-selected":activeIndex === position,"tabindex":-1,"class":`search-match`,"onmousedown":(event) => { if (event.button === 0) event.preventDefault(); },});
          if(match.kind === 'shop'){  { const $$_erotS2C = __sveltets_2_ensureComponent(Store); new $$_erotS2C({ target: __sveltets_2_any(), props: {   "size":16,"aria-hidden":`true`,}});}}
           { svelteHTML.createElement("span", { "class":`search-match-name`,}); { svelteHTML.createElement("strong", {});match.name; }if(match.shop){ { svelteHTML.createElement("small", {});match.shop; }} }
           { svelteHTML.createElement("span", { "class":`search-match-details`,});
              { const $$_egdaBdniKhcraeS3C = __sveltets_2_ensureComponent(SearchKindBadge); new $$_egdaBdniKhcraeS3C({ target: __sveltets_2_any(), props: { "kind":match.kind,}});}
            if(match.lowest_price != null){ { svelteHTML.createElement("small", {});money(match.lowest_price); }}
           }
         }
      }
     }
  }
};return __sveltets_2_any(0)};
  
  
  
  


  let { initialQuery = '' }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
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
      const response = await fetch('/api/search' + params({ q: term, suggest: 'true' }), { signal: controller.signal });
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
;
async () => {



 { svelteHTML.createElement("form", {       "class":`search-container`,"role":`search`,"action":`/search`,"onfocusout":handleFocusOut,});
   { svelteHTML.createElement("div", { "class":`search`,});
      { const $$_hcraeS2C = __sveltets_2_ensureComponent(Search); new $$_hcraeS2C({ target: __sveltets_2_any(), props: {   "size":19,"aria-hidden":`true`,}});}
     { svelteHTML.createElement("input", {                                  "name":`q`,"type":`search`,"role":`combobox`,"aria-label":`Search products and shops`,"aria-autocomplete":`list`,"aria-expanded":hasOptions,"aria-controls":hasOptions ? listId : undefined,"aria-activedescendant":hasOptions && activeIndex >= 0 ? optionId(activeIndex) : undefined,"placeholder":`Search products and shops…`,"value":query,"maxlength":150,"required":true,"autocomplete":`off`,"onfocus":() => { focused = true; queueSearch(); },"oninput":(event) => { query = event.currentTarget.value; if (!(event instanceof InputEvent && event.isComposing)) queueSearch(); },"oncompositionstart":close,"oncompositionend":(event) => { query = event.currentTarget.value; queueSearch(); },"onkeydown":handleKey,});}
     { svelteHTML.createElement("button", { "aria-label":`Submit search`,});  { const $$_thgiRworrA3C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA3C({ target: __sveltets_2_any(), props: { "size":17,}});} }
   }
  if(visible){
     { svelteHTML.createElement("div", { "class":`search-popup`,});
      if(hasOptions){
         { svelteHTML.createElement("div", {     "id":listId,"role":`listbox`,"aria-label":`Search suggestions`,});
          ;__sveltets_2_ensureSnippet(suggestionGroup('Products', groups.products, 0));
          ;__sveltets_2_ensureSnippet(suggestionGroup('Shops', groups.shops, groups.products.length));
         }
      }else{
         { svelteHTML.createElement("p", {   "class":`search-status`,"role":`status`,});phase === 'loading' ? 'Searching…' : phase === 'error' ? 'Suggestions unavailable. Press Enter to search.' : 'No matching products or shops.'; }
      }
     }
  }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const SearchBar__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type SearchBar__SvelteComponent_ = ReturnType<typeof SearchBar__SvelteComponent_>;
/*Ωignore_endΩ*/export default SearchBar__SvelteComponent_;