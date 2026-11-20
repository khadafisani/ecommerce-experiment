///<reference types="svelte" />
;
import AreaChart from './AreaChart.svelte';

;type $$ComponentProps =  { title: string; points: { label: string; value: number | string }[]; pie?: boolean; currency?: boolean };function $$render() {

  
  let { title, points, pie = false, currency = false }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
  const palette = ['#137664','#ffbb66','#cf6354','#87968e','#74c6b5'];
  let total = $derived(points.reduce((sum,p)=>sum+Number(p.value),0));
  let gradient = $derived.by(()=>{
    let end=0;
    return points.map((p,i)=>{const start=end; end+=100*Number(p.value)/total; return palette[i%palette.length]+' '+start+'% '+end+'%';}).join(',');
  });
;
async () => {
 { svelteHTML.createElement("section", { "class":`panel chart`,});
   { svelteHTML.createElement("h3", {});title; }
  if(!points.length){ { svelteHTML.createElement("div", { "class":`inline-empty`,});     }
  } else if (pie){
     { svelteHTML.createElement("div", { "class":`pie-layout`,}); { svelteHTML.createElement("div", {    "class":`pie`,"aria-hidden":`true`,});__sveltets_2_ensureType(String, Number, 'conic-gradient('+gradient+')'); } { svelteHTML.createElement("ul", {});   for(let p of __sveltets_2_ensureArray(points)){let i = 1;p.label; { svelteHTML.createElement("li", {}); { svelteHTML.createElement("i", { });__sveltets_2_ensureType(String, Number, palette[i%palette.length]); }p.label;  { svelteHTML.createElement("strong", {});p.value; } }} } }
  }else{
      { const $$_trahCaerA1C = __sveltets_2_ensureComponent(AreaChart); new $$_trahCaerA1C({ target: __sveltets_2_any(), props: {  title,points,currency,}});}
  }
 }
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Chart__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Chart__SvelteComponent_ = ReturnType<typeof Chart__SvelteComponent_>;
/*Ωignore_endΩ*/export default Chart__SvelteComponent_;