///<reference types="svelte" />
;
;type $$ComponentProps =  {
    cursor: number | null;
    loading: boolean;
    error: string;
    next: () => void;
  };function $$render() {

  let { cursor, loading, error, next }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();

  const supportsObserver = typeof IntersectionObserver !== 'undefined';

  function observe(node: Element) {
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      observer.disconnect();
      next();
    }, { rootMargin: '0px 0px 240px 0px' });
    observer.observe(node);
    return () => observer.disconnect();
  }
;
async () => {

if(cursor !== null){
   { svelteHTML.createElement("div", { "class":`more`,});
    if(loading){
       { svelteHTML.createElement("p", {   "class":`muted`,"role":`status`,});  }
    } else if (error){
       { svelteHTML.createElement("p", {   "class":`error`,"role":`alert`,});error; }
       { svelteHTML.createElement("button", { "onclick":next,});   }
    } else if (supportsObserver){
      cursor; {
         { svelteHTML.createElement("div", {    "class":`cursor-sentinel`,"aria-hidden":`true`,[Symbol("@attach")]:observe,}); }
      }
    }else{
       { svelteHTML.createElement("button", { "onclick":next,});  }
    }
   }
}
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const CursorMore__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type CursorMore__SvelteComponent_ = ReturnType<typeof CursorMore__SvelteComponent_>;
/*Ωignore_endΩ*/export default CursorMore__SvelteComponent_;