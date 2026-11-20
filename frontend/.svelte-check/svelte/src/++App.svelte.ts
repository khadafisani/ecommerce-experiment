///<reference types="svelte" />
;
  const labelPortal=(portal:string)=>({customer:'Customer',merchant:'Merchant',admin:'Admin'})[portal]??portal;
;;

import { onMount } from 'svelte';
import { ArrowRight, Bell, ChevronDown, ChevronRight, Coffee, Headphones, House, LayoutGrid, Package, Shirt, ShoppingBag, Sparkles, Store, Truck, UserRound, Volleyball } from '@lucide/svelte';
import { api, clear, groupSearchHits, guestCart, guestSession, key, logout, mergeGuestCart, message, params, save, signedIn, type Row, type SearchGroups, type SearchResponse } from './api';
import { CursorPager } from './cursor-pager.svelte';
import CursorMore from './CursorMore.svelte';
import ProductGrid from './ProductGrid.svelte';
import SearchBar from './SearchBar.svelte';
import Fields, {type Field} from './Fields.svelte';
import Shopping from './Shopping.svelte';
import Workspace from './Workspace.svelte';
function $$render() {

  
  
  
  
  
  
  
  
  
  
  const parts = location.pathname.split('/').filter(Boolean);
  const search = new URLSearchParams(location.search);
  const page = parts[0] ?? '';
  const portal = page === 'merchant' || page === 'admin' ? page : 'customer';
  const authPage = ['login','register','forgot','reset'].includes(portal==='customer'?page:parts[1]) && !(portal==='admin'&&parts[1]==='register');
  const mode = portal==='customer'?page:parts[1];
  const browse = page === '' || page === 'search' || page === 'shop';
  let account = $state<Row|null>(null), cartCount = $state(guestCart().reduce((sum,row)=>sum+row.quantity,0));
  let searchGroups = $state.raw<SearchGroups>({ products: [], shops: [] });
  let categories = $state<Row[]>([]);
  const pager = new CursorPager<Row & {id:number}>();
  let sort=$state('newest');
  let busy=$state(browse), error=$state(''), authNotice=$state('');
  const category = search.get('category');
  const icons = [{name:'All products',icon:LayoutGrid},{name:'Fashion',icon:Shirt},{name:'Electronics',icon:Headphones},{name:'Home & Living',icon:House},{name:'Beauty',icon:Sparkles},{name:'Sports',icon:Volleyball},{name:'Food & Drink',icon:Coffee}];
  const publicLinks = $derived([...icons,...categories.filter(c=>c.kind==='category'&&!icons.some(i=>i.name===c.name)).map(c=>({name:String(c.name),icon:Package}))]);
  const prefix = portal==='customer'?'':'/'+portal;
  const email: Field = {name:'email',label:'Email address',type:'email'};
  const password: Field = {name:'password',label:'Password',type:'password'};
  let fields: Field[] = $derived(mode==='forgot'?[email]:mode==='reset'?[password]:mode==='login'?[email,password]:[{name:'name',label:'Full name'},email,password,...(portal==='merchant'?[{name:'shop',label:'Shop name'}]:[])]);
  const title = $derived(mode==='forgot'?'Recover your account':mode==='reset'?'Set a new password':mode==='register'?(portal==='merchant'?'Open your shop':'Create an account'):(portal==='customer'?'Welcome back':portal==='merchant'?'Merchant sign in':'Admin sign in'));
  let shopTitle = $derived(page==='shop' ? pager.items[0]?.shop ?? 'Shop catalog' : '');
  let catalogError = $derived(page==='search' ? error : pager.cursor===null ? pager.error : '');
  const shopKey=key();
  async function load() {
    if(page!=='search') {
      const filters={category,sort,shop:page==='shop'?parts[1]:null};
      await pager.reset(cursor=>api('/products'+params({...filters,cursor})));
      return;
    }
    busy=true;error='';
    try {
      const result=await api<SearchResponse>('/search'+params({q:search.get('q')}));
      searchGroups=groupSearchHits(result);
    }catch(e){error=message(e);}finally{busy=false;}
  }
  onMount(()=>{
    const continueCheckout=()=>{continuingCheckout=true;};
    window.addEventListener('tosca-checkout-auth',continueCheckout);
    if(browse) {
      void load();
      api<Row[]>('/categories').then(value=>categories=value).catch(()=>{});
    }
    if(signedIn()) {
      api('/auth/me').then(value=>account=value).catch(()=>{});
      void updateCount();
    }
    return ()=>window.removeEventListener('tosca-checkout-auth',continueCheckout);
  });
  async function updateCount() {
    try { cartCount=(signedIn()&&!guestSession()?await api<Row[]>('/cart'):guestCart()).reduce((sum,row)=>sum+row.quantity,0); } catch {}
  }
  async function submitAuth(data:Row) {
    if(mode==='forgot') {await api('/auth/forgot','POST',data);authNotice='If an account matches this email, a reset link will arrive shortly.';return;}
    if(mode==='reset') {await api('/auth/reset','POST',{...data,token:search.get('token')});location.assign('/login');return;}
    save(await api('/auth/'+mode,'POST',{...data,portal}),false);
    if(portal==='customer') await mergeGuestCart().catch(()=>{});
    const next=search.get('next');
    location.assign(next?.startsWith('/')&&!next.startsWith('//')&&!next.includes('\\')?next:prefix||'/');
  }
  async function signOut() {
    try {await logout();location.assign('/');} catch(e){clear();error=message(e);location.assign('/');}
  }
  let continuingCheckout=false;
  function checkoutLeave(event: BeforeUnloadEvent) {
    if(!continuingCheckout && page==='checkout' && sessionStorage.getItem('checkout-picks')) {event.preventDefault();event.returnValue='';}
  }
;
async () => {

  { svelteHTML.createElement("svelte:window", { "onbeforeunload":checkoutLeave,});}
 { svelteHTML.createElement("header", { "class":`site-header`,});
   { svelteHTML.createElement("div", { "class":`header-inner wrap`,});
     { svelteHTML.createElement("a", {     "href":`/`,"class":`brand`,"aria-label":`Tosca Market home`,}); { svelteHTML.createElement("span", { "class":`brand-mark`,});  { const $$_gaBgnippohS4C = __sveltets_2_ensureComponent(ShoppingBag); new $$_gaBgnippohS4C({ target: __sveltets_2_any(), props: {   "size":21,"strokeWidth":2.3,}});} } { svelteHTML.createElement("span", {});  { svelteHTML.createElement("span", { "class":`brand-dot`,});  } { svelteHTML.createElement("small", {});  } } }
      { const $$_raBhcraeS2C = __sveltets_2_ensureComponent(SearchBar); new $$_raBhcraeS2C({ target: __sveltets_2_any(), props: { "initialQuery":search.get('q') ?? '',}});}
     { svelteHTML.createElement("div", { "class":`nav-actions`,});
       { svelteHTML.createElement("a", {   "href":account?.shops?.length?'/merchant':'/merchant/register',"class":`merchant-link`,});  { const $$_erotS4C = __sveltets_2_ensureComponent(Store); new $$_erotS4C({ target: __sveltets_2_any(), props: { "size":17,}});}  { svelteHTML.createElement("span", {});  } }
       { svelteHTML.createElement("a", {     "href":`/cart`,"class":`icon-button cart-icon`,"aria-label":'Cart, '+cartCount+' items',});  { const $$_gaBgnippohS4C = __sveltets_2_ensureComponent(ShoppingBag); new $$_gaBgnippohS4C({ target: __sveltets_2_any(), props: { "size":21,}});}if(cartCount){ { svelteHTML.createElement("span", {});cartCount; }} }
      if(account){
         { svelteHTML.createElement("details", { "class":`profile-menu`,}); { svelteHTML.createElement("summary", {}); { svelteHTML.createElement("span", { "class":`avatar`,});  { const $$_dnuoRresU6C = __sveltets_2_ensureComponent(UserRound); new $$_dnuoRresU6C({ target: __sveltets_2_any(), props: { "size":16,}});} } { svelteHTML.createElement("span", {});account.user.name; }  { const $$_nwoDnorvehC5C = __sveltets_2_ensureComponent(ChevronDown); new $$_nwoDnorvehC5C({ target: __sveltets_2_any(), props: { "size":14,}});} }
           { svelteHTML.createElement("div", { "class":`dropdown`,}); { svelteHTML.createElement("a", { "href":`/profile`,});  } { svelteHTML.createElement("a", { "href":`/orders`,});  } { svelteHTML.createElement("a", { "href":`/notifications`,});  { const $$_lleB6C = __sveltets_2_ensureComponent(Bell); new $$_lleB6C({ target: __sveltets_2_any(), props: { "size":15,}});}  }if(account.user.admin){ { svelteHTML.createElement("a", { "href":`/admin`,});  }} { svelteHTML.createElement("button", { "onclick":signOut,});  } }
         }
      }else{ { svelteHTML.createElement("a", {   "class":`signin`,"href":`/login`,});  { const $$_dnuoRresU4C = __sveltets_2_ensureComponent(UserRound); new $$_dnuoRresU4C({ target: __sveltets_2_any(), props: { "size":17,}});}  { svelteHTML.createElement("span", {});  } }}
     }
   }
  if(portal==='customer' && !authPage){
     { svelteHTML.createElement("nav", {   "class":`category-bar`,"aria-label":`Shop categories`,}); { svelteHTML.createElement("div", { "class":`wrap categories`,});
         for(let entry of __sveltets_2_ensureArray(publicLinks)){let i = 1;entry.name; { svelteHTML.createElement("a", {   "href":i===0?'/':'/'+params({category:entry.name})+'#catalog',});(i===0&&!category&&page==='')||entry.name===category;  { const $$_noci_yrtne4C = __sveltets_2_ensureComponent(entry.icon); new $$_noci_yrtne4C({ target: __sveltets_2_any(), props: { "size":16,}});}entry.name; }}
     } }
  }
 }

 { svelteHTML.createElement("main", { "class":`wrap main`,});
  if(authPage){
     { svelteHTML.createElement("section", { "class":`auth panel`,});
       { svelteHTML.createElement("a", {   "class":`breadcrumb`,"href":`/`,});   { const $$_thgiRnorvehC3C = __sveltets_2_ensureComponent(ChevronRight); new $$_thgiRnorvehC3C({ target: __sveltets_2_any(), props: { "size":14,}});} labelPortal(portal); }
       { svelteHTML.createElement("h1", {});title; }
        { const $$_sdleiF2C = __sveltets_2_ensureComponent(Fields); new $$_sdleiF2C({ target: __sveltets_2_any(), props: {      fields,"submit":submitAuth,"button":mode==='register'?'Create account':mode==='forgot'?'Send reset link':mode==='reset'?'Update password':'Sign in',"notice":authNotice,}});}
      if(mode==='login'){ { svelteHTML.createElement("p", { "class":`auth-links`,}); { svelteHTML.createElement("a", { "href":`/forgot`,});  }if(portal!=='admin'){ { svelteHTML.createElement("a", { "href":prefix+'/register',});   }} }
      }else{ { svelteHTML.createElement("p", {}); { svelteHTML.createElement("a", { "href":prefix+'/login',});    } }}
      if(portal==='merchant' && account && !account.user.guest && mode==='register'){
         { svelteHTML.createElement("hr", {});} { svelteHTML.createElement("h3", {});      }
          { const $$_sdleiF2C = __sveltets_2_ensureComponent(Fields); new $$_sdleiF2C({ target: __sveltets_2_any(), props: {     "fields":[{name:'name',label:'Shop name'}],"button":`Open shop`,"submit":async(data)=>{await api('/shops','POST',{...data,key:shopKey});location.assign('/merchant');},}});}
      }
     }
  } else if (browse){
    if(page===''&&!category){
       { svelteHTML.createElement("section", { "class":`hero`,});
         { svelteHTML.createElement("div", { "class":`hero-content`,}); { svelteHTML.createElement("div", { "class":`eyebrow`,}); { svelteHTML.createElement("span", {}); }    } { svelteHTML.createElement("h1", {});  { svelteHTML.createElement("br", {});} { svelteHTML.createElement("em", {});  } } { svelteHTML.createElement("p", {});          } { svelteHTML.createElement("a", {   "class":`hero-button`,"href":`#catalog`,});    { const $$_thgiRworrA4C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA4C({ target: __sveltets_2_any(), props: { "size":17,}});} } }
         { svelteHTML.createElement("div", {     "class":`hero-image`,"role":`img`,"aria-label":`Sunlit home with thoughtfully styled furniture`,}); }
       }
       { svelteHTML.createElement("div", { "class":`service-strip`,}); { svelteHTML.createElement("span", {});  { const $$_erotS3C = __sveltets_2_ensureComponent(Store); new $$_erotS3C({ target: __sveltets_2_any(), props: { "size":18,}});}   } { svelteHTML.createElement("span", {});  { const $$_egakcaP3C = __sveltets_2_ensureComponent(Package); new $$_egakcaP3C({ target: __sveltets_2_any(), props: { "size":18,}});}     } { svelteHTML.createElement("span", {});  { const $$_kcurT3C = __sveltets_2_ensureComponent(Truck); new $$_kcurT3C({ target: __sveltets_2_any(), props: { "size":18,}});}    } }
    }
     { svelteHTML.createElement("section", {   "id":`catalog`,"class":`catalog`,});
       { svelteHTML.createElement("div", { "class":`section-head`,}); { svelteHTML.createElement("div", {}); { svelteHTML.createElement("h2", {});page==='search'?'Search results':shopTitle||category||'Explore products'; }if(page==='search'){ { svelteHTML.createElement("p", {});  search.get('q') ?? '';  }} else if (!category && !shopTitle){ { svelteHTML.createElement("p", {});      }} }
        if(page!=='search'){ { svelteHTML.createElement("label", { "class":`sort`,});   { svelteHTML.createElement("select", {     "aria-label":`Sort products`,"bind:value":sort,"onchange":()=>load(),});/*Ωignore_startΩ*/() => sort = __sveltets_2_any(null);/*Ωignore_endΩ*/ { svelteHTML.createElement("option", { "value":`newest`,});  } { svelteHTML.createElement("option", { "value":`oldest`,});  } } }}
       }
      if(catalogError){
         { svelteHTML.createElement("p", {   "class":`error`,"role":`alert`,});catalogError;  { svelteHTML.createElement("button", { "onclick":()=>load(),});  } }
      } else if (page==='search'){
        if(searchGroups.products.length){
           { svelteHTML.createElement("section", {   "class":`search-result-group`,"aria-labelledby":`search-products-title`,});
             { svelteHTML.createElement("h3", { "id":`search-products-title`,});  }
             { const $$_dirGtcudorP3C = __sveltets_2_ensureComponent(ProductGrid); new $$_dirGtcudorP3C({ target: __sveltets_2_any(), props: {  "items":searchGroups.products,"showKinds":true,}});}
           }
        }
        if(searchGroups.shops.length){
           { svelteHTML.createElement("section", {   "class":`search-result-group`,"aria-labelledby":`search-shops-title`,});
             { svelteHTML.createElement("h3", { "id":`search-shops-title`,});  }
             { const $$_dirGtcudorP3C = __sveltets_2_ensureComponent(ProductGrid); new $$_dirGtcudorP3C({ target: __sveltets_2_any(), props: {  "items":searchGroups.shops,"showKinds":true,}});}
           }
        }
        if(!searchGroups.products.length && !searchGroups.shops.length){
           { svelteHTML.createElement("p", {   "class":`inline-empty`,"role":`status`,});busy ? 'Searching…' : 'No matching products or shops.'; }
        }
      } else if (pager.items.length){
          { const $$_dirGtcudorP2C = __sveltets_2_ensureComponent(ProductGrid); new $$_dirGtcudorP2C({ target: __sveltets_2_any(), props: { "items":pager.items,}});}
      }else{
         { svelteHTML.createElement("p", { "class":`inline-empty`,});pager.loading ? 'Loading products…' : 'No matching products.'; }
      }
      if(page!=='search'){  { const $$_eroMrosruC2C = __sveltets_2_ensureComponent(CursorMore); new $$_eroMrosruC2C({ target: __sveltets_2_any(), props: {       "cursor":pager.cursor,"loading":pager.loading,"error":pager.error,"next":()=>pager.next(),}});}}
     }
  } else if (portal!=='customer'){
    if(account&&!account.user.guest){  { const $$_ecapskroW1C = __sveltets_2_ensureComponent(Workspace); new $$_ecapskroW1C({ target: __sveltets_2_any(), props: {  portal,parts,account,}});}}else{ { svelteHTML.createElement("section", { "class":`panel empty`,}); { svelteHTML.createElement("h1", {});labelPortal(portal); } { svelteHTML.createElement("a", {   "class":`primary`,"href":prefix+'/login',});  } }}
  } else if (page==='profile'){
     { svelteHTML.createElement("section", { "class":`panel auth`,}); { svelteHTML.createElement("h1", {});  }if(account){ { svelteHTML.createElement("p", {}); { svelteHTML.createElement("strong", {});account.user.name; } { svelteHTML.createElement("br", {});}account.user.guest ? "Guest checkout on this browser" : account.user.email; } { svelteHTML.createElement("div", { "class":`actions`,}); { svelteHTML.createElement("a", {   "class":`button`,"href":`/orders`,});  } { svelteHTML.createElement("a", {   "class":`button`,"href":`/notifications`,});  } { svelteHTML.createElement("a", {   "class":`primary`,"href":`/merchant`,});  } }}else{ { svelteHTML.createElement("a", { "href":`/login`,});  }} }
  }else{
      { const $$_gnippohS1C = __sveltets_2_ensureComponent(Shopping); new $$_gnippohS1C({ target: __sveltets_2_any(), props: {   page,parts,"changed":updateCount,}});}
  }
 }
 { svelteHTML.createElement("footer", { "class":`site-footer`,}); { svelteHTML.createElement("div", { "class":`wrap`,}); { svelteHTML.createElement("a", {   "href":`/`,"class":`footer-brand`,});  { svelteHTML.createElement("span", {});  }  } { svelteHTML.createElement("nav", { "aria-label":`Footer`,}); { svelteHTML.createElement("a", { "href":`/merchant`,});  } { svelteHTML.createElement("a", { "href":`/orders`,});  } { svelteHTML.createElement("a", { "href":`/admin/login`,});  } } } }




};
return { props: {} as Record<string, never>, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const App__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type App__SvelteComponent_ = ReturnType<typeof App__SvelteComponent_>;
/*Ωignore_endΩ*/export default App__SvelteComponent_;