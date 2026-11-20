///<reference types="svelte" />
;
import { ArrowUpRight, Store } from '@lucide/svelte';
import { money, searchHitPath, type Row } from './api';
import ProductInitials from './ProductInitials.svelte';
import SearchKindBadge from './SearchKindBadge.svelte';

;type $$ComponentProps =  { items: Row[]; showKinds?: boolean };function $$render() {

  
  
  
  
  let { items, showKinds = false }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
;
async () => {
 { svelteHTML.createElement("div", { "class":`product-grid`,});
     for(let product of __sveltets_2_ensureArray(items)){searchHitPath({kind: product.kind ?? 'product', id: product.id});
    const kind = product.kind === 'shop' ? 'shop' : 'product';
     { svelteHTML.createElement("a", {   "class":`product-card`,"href":searchHitPath({kind, id: product.id}),});
       { svelteHTML.createElement("div", {  "class":`product-image`,});kind === 'shop';
        if(kind === 'shop'){  { const $$_erotS3C = __sveltets_2_ensureComponent(Store); new $$_erotS3C({ target: __sveltets_2_any(), props: {   "size":42,"aria-hidden":`true`,}});}
        } else if (product.image){ { svelteHTML.createElement("img", {      "src":product.image,"alt":product.name,"loading":`lazy`,});}
        }else{  { const $$_slaitinItcudorP3C = __sveltets_2_ensureComponent(ProductInitials); new $$_slaitinItcudorP3C({ target: __sveltets_2_any(), props: { "name":product.name,}});}}
        if(showKinds){  { const $$_egdaBdniKhcraeS3C = __sveltets_2_ensureComponent(SearchKindBadge); new $$_egdaBdniKhcraeS3C({ target: __sveltets_2_any(), props: {kind,}});}}
         { svelteHTML.createElement("span", { "class":`product-arrow`,});  { const $$_thgiRpUworrA4C = __sveltets_2_ensureComponent(ArrowUpRight); new $$_thgiRpUworrA4C({ target: __sveltets_2_any(), props: { "size":17,}});} }
       }
       { svelteHTML.createElement("div", { "class":`product-info`,});
        if(kind === 'product'){ { svelteHTML.createElement("small", {});  { const $$_erotS4C = __sveltets_2_ensureComponent(Store); new $$_erotS4C({ target: __sveltets_2_any(), props: { "size":13,}});} product.shop; }}
         { svelteHTML.createElement("strong", {});product.name; }
        if(kind === 'product'){ { svelteHTML.createElement("b", {});money(product.lowest_price); }}
       }
     }
  }
 }
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const ProductGrid__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type ProductGrid__SvelteComponent_ = ReturnType<typeof ProductGrid__SvelteComponent_>;
/*Ωignore_endΩ*/export default ProductGrid__SvelteComponent_;