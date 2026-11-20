///<reference types="svelte" />
;
import { onMount } from 'svelte';
import { ArrowLeft, ArrowRight, CreditCard, MapPin, Package, ShoppingBag, Store, Truck } from '@lucide/svelte';
import { api, checkedGuestCart, date, downloadReturnMedia, guestCart, guestSession, key, label, message, money, params, save, setGuest, signedIn, type ItemPick, type Row } from './api';
import { CursorPager } from './cursor-pager.svelte';
import CursorMore from './CursorMore.svelte';
import ProductGrid from './ProductGrid.svelte';
import ProductInitials from './ProductInitials.svelte';
import Fields from './Fields.svelte';

;type $$ComponentProps =  {page:string;parts:string[];changed:()=>Promise<void>};function $$render() {
 const media/*Ωignore_positionΩ*/ = (files:string[])/*Ωignore_startΩ*/: ReturnType<import('svelte').Snippet>/*Ωignore_endΩ*/ => { async ()/*Ωignore_positionΩ*/ => {
  if(files?.length){ { svelteHTML.createElement("div", { "class":`media-row`,});   for(let file of __sveltets_2_ensureArray(files)){file;
    if(/\.(mp4|webm)$/.test(file)){ { svelteHTML.createElement("video", {   "src":file,"controls":true,"aria-label":`Customer attachment`,}); { svelteHTML.createElement("track", {  "kind":`captions`,});} }
    }else{ { svelteHTML.createElement("a", {     "href":file,"target":`_blank`,"rel":`noreferrer`,}); { svelteHTML.createElement("img", {   "src":file,"alt":`Customer attachment`,});} }}
  } }}
};return __sveltets_2_any(0)}; const lines/*Ωignore_positionΩ*/ = (items:Row[])/*Ωignore_startΩ*/: ReturnType<import('svelte').Snippet>/*Ωignore_endΩ*/ => { async ()/*Ωignore_positionΩ*/ => {
   { svelteHTML.createElement("div", { "class":`table-scroll`,}); { svelteHTML.createElement("table", {}); { svelteHTML.createElement("thead", {}); { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } } }
     { svelteHTML.createElement("tbody", {});   for(let item of __sveltets_2_ensureArray(items)){let i = 1;i; { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", {}); { svelteHTML.createElement("a", { "href":'/product/'+item.product_id,});item.product_name; } } { svelteHTML.createElement("td", {});item.variant_name??item.variant; } { svelteHTML.createElement("td", {});item.quantity; } { svelteHTML.createElement("td", {});money(item.price); } }} } } }
};return __sveltets_2_any(0)};
  
  
  
  
  
  
  
  
  let {page,parts,changed}:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/=$props();
  let id=$derived(Number(parts[1]));
  let authenticated=$state(signedIn());
  let data=$state<Row|null>(null), rows=$state<Row[]>([]), addresses=$state<Row[]>([]);
  let error=$state(''), feedback=$state(''), loading=$state(true), pending=$state(false);
  let variantId=$state<number|undefined>(), quantity=$state(1), selected=$state<number[]>([]);
  let variant=$derived(data?.variants?.find((v:Row)=>v.id===variantId));
  const pager = new CursorPager<Row & {id:number}>();
  let q=$state(''), orderTracking=$state<Row|null>(null);
  let addressId=$state<number|undefined>(), summary=$state<Row|null>(null);
  let quoteVersion=0;
  let addressKey=key();
  function readCheckout(): {items:ItemPick[];from_cart:boolean;guest_cart?:boolean;key:string} | null {
    try{return JSON.parse(sessionStorage.getItem('checkout-picks') ?? 'null');}catch{return null;}
  }
  const checkout=readCheckout();
  onMount(()=>{ void load(); });
  async function load() {
    if(page==='orders'&&!id&&signedIn()) {
      const query=q;
      loading=false;
      await pager.reset(cursor=>api('/checkout'+params({q:query,cursor})));
      return;
    }
    loading=true;error='';
    try {
      if(page==='product') {
        const product=await api('/products/'+id);
        data=product;
        variantId=product.variants.find((v:Row)=>v.stock>0)?.id??product.variants[0]?.id;
      } else if(page==='cart') {
        rows=signedIn()&&!guestSession()?await api<Row[]>('/cart'):await checkedGuestCart();
        selected=rows.filter(v=>!v.reason).map(v=>v.id);
      } else if(page==='checkout' && signedIn()) {
        addresses=await api<Row[]>('/addresses');
        addressId=addresses[0]?.id;
        await getQuote();
      } else if(page==='orders'&&signedIn()) {
        if(id) data=await api('/checkout/'+id);
      } else if(page==='notifications'&&signedIn()) rows=await api<Row[]>('/notifications');
    }catch(e){error=message(e);}finally{loading=false;}
  }
  async function act(operation:()=>Promise<void>) {
    if(pending)return;
    pending=true;error='';feedback='';
    try{await operation();}catch(e){error=message(e);}finally{pending=false;}
  }
  function ago(value:string) {
    const stamp=value.replace(' ','T')+(value.endsWith('Z')?'':'Z');
    const days=Math.max(0,Math.floor((Date.now()-new Date(stamp).valueOf())/86400000));
    return days===0?'Today':days<7?days+'d ago':days<30?Math.floor(days/7)+'w ago':days<365?Math.floor(days/30)+'m ago':Math.floor(days/365)+'y ago';
  }
  async function updateCart(row:Row,next:number) {
    await act(async()=>{
      if(signedIn()&&!guestSession()) await api('/cart/'+row.id,'PUT',{quantity:next});
      else setGuest(row.id,next);
      const active=selected;
      rows=signedIn()&&!guestSession()?await api<Row[]>('/cart'):await checkedGuestCart();
      selected=rows.filter(v=>!v.reason&&active.includes(v.id)).map(v=>v.id);
      await changed();
    });
  }
  function start(items:ItemPick[],from_cart:boolean) {
    const guest_cart=from_cart&&(!signedIn()||guestSession());
    sessionStorage.setItem('checkout-picks',JSON.stringify({items,from_cart:from_cart&&!guest_cart,guest_cart,key:key()}));
    location.assign('/checkout');
  }
  async function addProduct() {
    if(!data||!variant)return;
    const item={product_id:data.id,product_name:data.name,image:data.image,shop:data.shop,variant:variant.name,subvariant:variant.subvariant,price:variant.price};
    await act(async()=>{
      const line=variant!;
      if(signedIn()&&!guestSession()) {
        const old=(await api<Row[]>('/cart')).find(r=>r.id===line.id)?.quantity??0;
        await api('/cart/'+line.id,'PUT',{quantity:old+quantity});
      }else {
        const old=guestCart().find(r=>r.id===line.id)?.quantity??0;
        if(old+quantity>line.stock) throw new Error('This quantity exceeds available stock');
        setGuest(line.id,old+quantity,item);
      }
      feedback='Added to your cart.';
      await changed();
    });
  }
  async function getQuote() {
    const version=++quoteVersion;
    summary=null;error='';
    if (!addressId||!checkout)return;
    try {const quote=await api('/checkout/quote','POST',{...checkout,from_cart:checkout.from_cart || Boolean(checkout.guest_cart && !guestSession()),address:addressId});if(version===quoteVersion)summary=quote;}
    catch(e){if(version===quoteVersion)error=message(e);}
  }
  async function addAddress(input:Row) {
    const result=await api('/addresses','POST',{...input,key:addressKey});
    addresses=await api<Row[]>('/addresses');
    addressId=result.id;addressKey=key();await getQuote();
  }
  async function placeOrder() {
    if(!checkout||!addressId||!summary)return;
    await act(async()=>{let order: Row;
      try{order=await api('/checkout','POST',{...checkout,from_cart:checkout.from_cart || Boolean(checkout.guest_cart && !guestSession()),address:addressId,expected_total:summary?.total});}
      catch(e){if(message(e).startsWith('The checkout total changed')) await getQuote();throw e;}
      if(checkout.guest_cart) for(const item of checkout.items) setGuest(item.id,0);
      sessionStorage.removeItem('checkout-picks');location.assign('/orders/'+order.id);});
  }
  async function continueAsGuest() {
    await act(async()=>{
      save(await api('/auth/guest','POST'),true);
      authenticated=true;
      await load();
    });
  }
  async function commit(path:string) { await act(async()=>{await api(path,'POST');await load();}); }
  let chosen=$derived(rows.filter(r=>selected.includes(r.id)&&!r.reason));
  let total=$derived(chosen.reduce((sum,r)=>sum+r.price*r.quantity,0));
  const formPage=$derived(page==='return'||page==='review');
  async function submitReview(input:Row) {
    if(page==='return')await api('/orders/'+id+'/return','POST',input);
    else await api('/orders/'+id+'/reviews','POST',{...input,rating:Number(input.rating),product_id:Number(parts[2])});
    const checkoutId=new URLSearchParams(location.search).get('checkout');
    location.assign(checkoutId?'/orders/'+checkoutId+'?submitted='+page:'/orders');
  }
  const submitted=new URLSearchParams(location.search).get('submitted');
;
async () => {




if(error){ { svelteHTML.createElement("p", {   "class":`error`,"role":`alert`,});error; }}
if(loading){ { svelteHTML.createElement("p", { "class":`inline-empty`,});  }
} else if (!authenticated&&(page==='orders'||page==='checkout'||page==='notifications'||formPage)){
   { svelteHTML.createElement("section", { "class":`panel empty`,}); { svelteHTML.createElement("h1", {});page==='checkout' && checkout?'How would you like to check out?':'Sign in to continue'; } { svelteHTML.createElement("div", { "class":`actions`,}); { svelteHTML.createElement("a", {     "class":`primary`,"href":'/login'+params({next:location.pathname+location.search}),"onclick":()=>{if(page==='checkout')window.dispatchEvent(new Event('tosca-checkout-auth'));},});    { const $$_thgiRworrA3C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA3C({ target: __sveltets_2_any(), props: { "size":16,}});} }if(page==='checkout'&&checkout){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":continueAsGuest,});   }} } }
} else if (page==='product' && data){
   { svelteHTML.createElement("a", {   "class":`breadcrumb`,"href":`/`,});  { const $$_tfeLworrA1C = __sveltets_2_ensureComponent(ArrowLeft); new $$_tfeLworrA1C({ target: __sveltets_2_any(), props: { "size":16,}});}  }
   { svelteHTML.createElement("section", { "class":`detail-grid`,});
     { svelteHTML.createElement("div", { "class":`detail-image`,});if(data.image){ { svelteHTML.createElement("img", {   "src":data.image,"alt":data.name,});}}else{  { const $$_slaitinItcudorP2C = __sveltets_2_ensureComponent(ProductInitials); new $$_slaitinItcudorP2C({ target: __sveltets_2_any(), props: { "name":data.name,}});}} }
     { svelteHTML.createElement("div", { "class":`panel product-detail`,});
       { svelteHTML.createElement("a", {   "class":`shop-name`,"href":'/shop/'+data.shop_id,});  { const $$_erotS3C = __sveltets_2_ensureComponent(Store); new $$_erotS3C({ target: __sveltets_2_any(), props: { "size":18,}});} data.shop; }
       { svelteHTML.createElement("h1", {});data.name; } { svelteHTML.createElement("strong", { "class":`detail-price`,});money(variant?.price); }
       { svelteHTML.createElement("p", {});data.description; }
      if(data.brand || data.category){ { svelteHTML.createElement("div", { "class":`tags`,});if(data.brand){ { svelteHTML.createElement("span", {});data.brand; }}if(data.category){ { svelteHTML.createElement("span", {});data.category; }} }}
       { svelteHTML.createElement("div", { "class":`field-row`,}); { svelteHTML.createElement("label", {});  { svelteHTML.createElement("select", {   "bind:value":variantId,"onchange":()=>quantity=1,});/*Ωignore_startΩ*/() => variantId = __sveltets_2_any(null);/*Ωignore_endΩ*/   for(let v of __sveltets_2_ensureArray(data.variants)){v.id; { svelteHTML.createElement("option", {   "value":v.id,"disabled":!v.stock,});v.name;v.subvariant?' / '+v.subvariant:'';v.stock?'':' · Out of stock'; }} } }
       { svelteHTML.createElement("label", {});  { svelteHTML.createElement("input", {       "type":`number`,"min":`1`,"max":Math.min(999,variant?.stock??1),"bind:value":quantity,});/*Ωignore_startΩ*/() => quantity = __sveltets_2_any(null);/*Ωignore_endΩ*/} } }
      if(variant){ { svelteHTML.createElement("p", { "class":`muted`,});variant.stock;    variant.grams;  }}
       { svelteHTML.createElement("div", { "class":`actions`,}); { svelteHTML.createElement("button", {   "disabled":pending||!variant||quantity<1||quantity>variant.stock||!quantity,"onclick":addProduct,});  { const $$_gaBgnippohS4C = __sveltets_2_ensureComponent(ShoppingBag); new $$_gaBgnippohS4C({ target: __sveltets_2_any(), props: { "size":17,}});}    }
         { svelteHTML.createElement("button", {     "class":`primary`,"disabled":!variant||quantity<1||quantity>variant.stock||!quantity,"onclick":()=>start([{id:variantId!,quantity}],false),});    { const $$_thgiRworrA4C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA4C({ target: __sveltets_2_any(), props: { "size":16,}});} } }
      if(feedback){ { svelteHTML.createElement("p", {   "class":`feedback`,"role":`status`,});feedback; }}
     }
   }
   { svelteHTML.createElement("section", { "class":`panel section`,}); { svelteHTML.createElement("h2", {});  }   for(let review of __sveltets_2_ensureArray(data.reviews)){review.id; { svelteHTML.createElement("article", { "class":`review`,}); { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("strong", {});review.user; } { svelteHTML.createElement("span", { "class":`rating`,});review.rating;  } } { svelteHTML.createElement("p", {});review.body; };__sveltets_2_ensureSnippet(media(review.media));if(review.reply){ { svelteHTML.createElement("blockquote", {}); { svelteHTML.createElement("strong", {});  } { svelteHTML.createElement("br", {});}review.reply; }} }} { svelteHTML.createElement("p", { "class":`muted`,});   } }
  if(data.similar.length){ { svelteHTML.createElement("section", { "class":`section`,}); { svelteHTML.createElement("h2", {});   }  { const $$_dirGtcudorP1C = __sveltets_2_ensureComponent(ProductGrid); new $$_dirGtcudorP1C({ target: __sveltets_2_any(), props: { "items":data.similar,}});} }}
} else if (page==='cart'){
   { svelteHTML.createElement("h1", {});  }
  if(rows.length){
     { svelteHTML.createElement("div", { "class":`detail-grid cart-layout`,}); { svelteHTML.createElement("div", { "class":`panel cart-list`,});
         for(let row of __sveltets_2_ensureArray(rows)){row.id; { svelteHTML.createElement("article", {   "class":`cart-row`,});row.reason;
         { svelteHTML.createElement("input", {        "type":`checkbox`,"aria-label":'Select '+row.product_name,"value":row.id,"disabled":Boolean(row.reason),});selected = __sveltets_2_any(null);}
         { svelteHTML.createElement("a", {     "class":`thumb`,"href":'/product/'+row.product_id,"aria-label":row.product_name,});if(row.image){ { svelteHTML.createElement("img", {  "src":row.image,"alt":"",});}}else{  { const $$_slaitinItcudorP4C = __sveltets_2_ensureComponent(ProductInitials); new $$_slaitinItcudorP4C({ target: __sveltets_2_any(), props: { "name":row.product_name,}});}} }
         { svelteHTML.createElement("div", { "class":`cart-product`,}); { svelteHTML.createElement("a", { "href":'/product/'+row.product_id,}); { svelteHTML.createElement("strong", {});row.product_name; } } { svelteHTML.createElement("small", {});row.variant;row.subvariant?' / '+row.subvariant:'';  row.shop; } { svelteHTML.createElement("small", {}); ago(row.created_at).toLowerCase(); }if(row.reason){ { svelteHTML.createElement("span", { "class":`badge`,});row.reason; }} }
         { svelteHTML.createElement("label", { "class":`quantity`,}); { svelteHTML.createElement("span", { "class":`sr-only`,});  row.product_name; } { svelteHTML.createElement("input", {           "type":`number`,"min":`1`,"max":`999`,"value":row.quantity,"disabled":pending||Boolean(row.reason),"onchange":(e)=>updateCart(row,Number(e.currentTarget.value)),});} }
         { svelteHTML.createElement("strong", {});money(row.price); } { svelteHTML.createElement("button", {     "class":`text-button`,"disabled":pending,"onclick":()=>updateCart(row,0),});  }
       }}
     } { svelteHTML.createElement("aside", { "class":`panel totals`,}); { svelteHTML.createElement("h2", {});  } { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("span", {});  } { svelteHTML.createElement("strong", {});chosen.length; } } { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("span", {});  } { svelteHTML.createElement("strong", {});money(total); } } { svelteHTML.createElement("button", {     "class":`primary`,"disabled":!chosen.length,"onclick":()=>start(chosen.map(({id,quantity})=>({id,quantity})),true),});   { const $$_thgiRworrA3C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA3C({ target: __sveltets_2_any(), props: { "size":17,}});} } } }
  }else{ { svelteHTML.createElement("section", { "class":`panel empty`,});  { const $$_gaBgnippohS1C = __sveltets_2_ensureComponent(ShoppingBag); new $$_gaBgnippohS1C({ target: __sveltets_2_any(), props: { "size":28,}});} { svelteHTML.createElement("h2", {});    } { svelteHTML.createElement("a", {   "class":`primary`,"href":`/`,});  } }}
} else if (page==='checkout'){
   { svelteHTML.createElement("a", {   "class":`breadcrumb`,"href":`/cart`,});  { const $$_tfeLworrA1C = __sveltets_2_ensureComponent(ArrowLeft); new $$_tfeLworrA1C({ target: __sveltets_2_any(), props: { "size":16,}});}  } { svelteHTML.createElement("h1", {});  }
  if(checkout){
     { svelteHTML.createElement("div", { "class":`detail-grid checkout-layout`,});
       { svelteHTML.createElement("div", { "class":`stack`,});
         { svelteHTML.createElement("section", { "class":`panel`,}); { svelteHTML.createElement("h2", {});  { const $$_niPpaM4C = __sveltets_2_ensureComponent(MapPin); new $$_niPpaM4C({ target: __sveltets_2_any(), props: { "size":20,}});}   }if(addresses.length){ { svelteHTML.createElement("label", { "class":`field`,});  { svelteHTML.createElement("select", {   "bind:value":addressId,"onchange":getQuote,});/*Ωignore_startΩ*/() => addressId = __sveltets_2_any(null);/*Ωignore_endΩ*/   for(let address of __sveltets_2_ensureArray(addresses)){address.id; { svelteHTML.createElement("option", { "value":address.id,});address.label;  address.detail; }} } }}
         { svelteHTML.createElement("details", { "open":!addresses.length,}); { svelteHTML.createElement("summary", {});   }  { const $$_sdleiF4C = __sveltets_2_ensureComponent(Fields); new $$_sdleiF4C({ target: __sveltets_2_any(), props: {     "fields":[{name:'label',label:'Address label'},{name:'detail',label:'Recipient, phone and full address',type:'textarea'}],"button":`Save address`,"submit":addAddress,}});} } }
         { svelteHTML.createElement("section", { "class":`panel`,}); { svelteHTML.createElement("h2", {});  { const $$_draCtiderC4C = __sveltets_2_ensureComponent(CreditCard); new $$_draCtiderC4C({ target: __sveltets_2_any(), props: { "size":20,}});}   } { svelteHTML.createElement("label", { "class":`check`,}); { svelteHTML.createElement("input", {    "type":`radio`,"name":`payment`,"checked":true,});}   } { svelteHTML.createElement("div", { "class":`gateway`,}); { svelteHTML.createElement("span", {});    } { svelteHTML.createElement("div", {});   for(let brand of __sveltets_2_ensureArray((['QRIS','BCA','BRI','Mandiri']))){brand; { svelteHTML.createElement("button", {"disabled":true,}); { svelteHTML.createElement("img", {   "src":'/payment/'+brand.toLowerCase()+'.svg',"alt":brand,});} }} } } }
        if(summary){   for(let [shop,g] of __sveltets_2_ensureArray(Object.entries(summary.groups))){shop;
           { svelteHTML.createElement("section", { "class":`panel`,}); { svelteHTML.createElement("h3", {});  { const $$_erotS4C = __sveltets_2_ensureComponent(Store); new $$_erotS4C({ target: __sveltets_2_any(), props: { "size":17,}});} (g as Row).shop; };__sveltets_2_ensureSnippet(lines((g as Row).items)); { svelteHTML.createElement("p", {});  (g as Row).grams/1000;   money((g as Row).shipping); } }
        }}
       }
       { svelteHTML.createElement("aside", { "class":`panel totals`,}); { svelteHTML.createElement("h2", {});  }if(summary){ { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("span", {});  } { svelteHTML.createElement("strong", {});money(summary.subtotal); } } { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("span", {});  } { svelteHTML.createElement("strong", {});money(summary.shipping); } } { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("span", {});  } { svelteHTML.createElement("strong", {});money(summary.tax); } } { svelteHTML.createElement("div", { "class":`split grand`,}); { svelteHTML.createElement("span", {});  } { svelteHTML.createElement("strong", {});money(summary.total); } }} { svelteHTML.createElement("button", {     "class":`primary`,"disabled":!summary||pending,"onclick":placeOrder,});    { const $$_thgiRworrA3C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA3C({ target: __sveltets_2_any(), props: { "size":17,}});} } }
     }
  }else{ { svelteHTML.createElement("section", { "class":`panel empty`,}); { svelteHTML.createElement("p", {});     } { svelteHTML.createElement("a", { "href":`/cart`,});   } }}
} else if (page==='orders'&&!id){
   { svelteHTML.createElement("h1", {});  }
   { svelteHTML.createElement("section", { "class":`panel`,}); { svelteHTML.createElement("form", {   "class":`filter`,"onsubmit":(e)=>{e.preventDefault();load();},}); { svelteHTML.createElement("input", {     "placeholder":`Search orders`,"aria-label":`Search orders`,"bind:value":q,});/*Ωignore_startΩ*/() => q = __sveltets_2_any(null);/*Ωignore_endΩ*/} { svelteHTML.createElement("button", {});  } }
   { svelteHTML.createElement("div", { "class":`table-scroll`,}); { svelteHTML.createElement("table", {}); { svelteHTML.createElement("thead", {}); { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {}); } } } { svelteHTML.createElement("tbody", {});
     for(let row of __sveltets_2_ensureArray(pager.items)){row.id; { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", { "class":`id-cell`,});row.checkout_no; } { svelteHTML.createElement("td", {});date(row.created_at); } { svelteHTML.createElement("td", {});money(row.total); } { svelteHTML.createElement("td", {}); { svelteHTML.createElement("span", { "class":`badge`,});row.status; } } { svelteHTML.createElement("td", {}); { svelteHTML.createElement("a", { "href":'/orders/'+row.id,});    { const $$_thgiRworrA7C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA7C({ target: __sveltets_2_any(), props: { "size":14,}});} } } }} { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", { "colspan":5,});if(pager.loading){ } else if (pager.error){ { svelteHTML.createElement("span", { "role":`alert`,});pager.error; }  { svelteHTML.createElement("button", { "onclick":()=>load(),});  }}else{  } } } } } }
    { const $$_eroMrosruC1C = __sveltets_2_ensureComponent(CursorMore); new $$_eroMrosruC1C({ target: __sveltets_2_any(), props: {       "cursor":pager.cursor,"loading":pager.loading,"error":pager.error,"next":()=>pager.next(),}});} }
} else if (page==='orders'&&data){
   { svelteHTML.createElement("a", {   "class":`breadcrumb`,"href":`/orders`,});  { const $$_tfeLworrA1C = __sveltets_2_ensureComponent(ArrowLeft); new $$_tfeLworrA1C({ target: __sveltets_2_any(), props: { "size":16,}});}   }
   { svelteHTML.createElement("section", { "class":`panel`,}); { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("h1", {}); data.id; } { svelteHTML.createElement("span", { "class":`badge`,});data.status; } } { svelteHTML.createElement("p", { "class":`muted id-cell`,});data.checkout_no;  date(data.created_at); } { svelteHTML.createElement("p", {});data.address; }
     { svelteHTML.createElement("p", {});  { svelteHTML.createElement("strong", {});money(data.total); }   money(data.shipping);   money(data.tax); }
    if(submitted){ { svelteHTML.createElement("p", { "class":`feedback`,});submitted==='return'?'Your return is in review.':'Your review was submitted.'; }}
    if(data.payment==='waiting'){ { svelteHTML.createElement("p", {});data.payment_instructions; } { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>{if(confirm('Cancel this unpaid checkout?'))commit('/checkout/'+id+'/cancel');},});  }}
   }
     for(let order of __sveltets_2_ensureArray(data.orders)){order.id;
     { svelteHTML.createElement("section", { "class":`panel section`,}); { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("h2", {});  { const $$_erotS3C = __sveltets_2_ensureComponent(Store); new $$_erotS3C({ target: __sveltets_2_any(), props: { "size":18,}});} order.shop; } { svelteHTML.createElement("span", { "class":`badge`,});label(order.status); } }
      ;__sveltets_2_ensureSnippet(lines(order.items));
       { svelteHTML.createElement("p", {}); money(order.subtotal);   money(order.shipping);   money(order.tax); }
       { svelteHTML.createElement("div", { "class":`actions`,});
        if(order.courier){ { svelteHTML.createElement("button", { "onclick":()=>orderTracking=order,});  { const $$_kcurT3C = __sveltets_2_ensureComponent(Truck); new $$_kcurT3C({ target: __sveltets_2_any(), props: { "size":17,}});}   }}
        if(order.status==='delivery'){ { svelteHTML.createElement("button", {     "class":`primary`,"disabled":pending,"onclick":()=>commit('/orders/'+order.id+'/receive'),});  }}
        if(order.returnable){ { svelteHTML.createElement("a", {   "class":`button`,"href":'/return/'+order.id+params({checkout:id}),});  }}
       }
      if(order.completed_at){ { svelteHTML.createElement("div", { "class":`actions section`,});   for(let item of __sveltets_2_ensureArray((order.items.filter((item:Row,i:number)=>!item.reviewed&&order.items.findIndex((entry:Row)=>entry.product_id===item.product_id)===i)))){item.product_id; { svelteHTML.createElement("a", {   "class":`button`,"href":'/review/'+order.id+'/'+item.product_id+params({checkout:id}),}); item.product_name; }} }}
      if(order.return){ { svelteHTML.createElement("p", {});order.return.description; } { svelteHTML.createElement("div", { "class":`actions`,});   for(let file of __sveltets_2_ensureArray(order.return.media)){let i = 1;file; { svelteHTML.createElement("button", { "onclick":()=>act(()=>downloadReturnMedia(file)),});  i+1; }} }}
     }
  }
} else if (formPage){
   { svelteHTML.createElement("section", { "class":`panel auth`,}); { svelteHTML.createElement("h1", {});page==='return'?'Request a return':'Review your purchase'; }
      { const $$_sdleiF1C = __sveltets_2_ensureComponent(Fields); new $$_sdleiF1C({ target: __sveltets_2_any(), props: {       "fields":page==='return'?[{name:'description',label:'Reason for the return',type:'textarea'},{name:'media',label:'Photo or video',type:'media',required:true}]:[{name:'rating',label:'Rating',options:[5,4,3,2,1].map(n=>({value:n,text:n+' / 5'}))},{name:'body',label:'Your review',type:'textarea'},{name:'media',label:'Photos or videos',type:'media'}],"button":page==='return'?'Submit return':'Post review',"submit":submitReview,"privateMedia":page==='return',}});}
   }
} else if (page==='notifications'){
   { svelteHTML.createElement("h1", {});  } { svelteHTML.createElement("section", { "class":`panel`,});
     { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>commit('/notifications/read'),});   }
       for(let row of __sveltets_2_ensureArray(rows)){row.id; { svelteHTML.createElement("div", { "class":`split review`,}); { svelteHTML.createElement("span", {});row.message;if(!row.read_at){  { svelteHTML.createElement("span", { "class":`badge`,});  }} } { svelteHTML.createElement("small", {});date(row.created_at); } }} { svelteHTML.createElement("p", {});   }
   }
} else if (!error){
   { svelteHTML.createElement("section", { "class":`panel empty`,});  { const $$_egakcaP1C = __sveltets_2_ensureComponent(Package); new $$_egakcaP1C({ target: __sveltets_2_any(), props: { "size":28,}});} { svelteHTML.createElement("h1", {});  } { svelteHTML.createElement("a", {   "class":`primary`,"href":`/`,});    } }
}

if(orderTracking){
   { svelteHTML.createElement("dialog", {   "open":true,"class":`dialog`,"aria-label":`Shipping progress`,});
     { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("h2", {});  } { svelteHTML.createElement("button", { "onclick":()=>orderTracking=null,});  } }
     { svelteHTML.createElement("p", {});orderTracking.courier;  orderTracking.receipt; }
    if(orderTracking.courier_url){ { svelteHTML.createElement("a", {     "href":orderTracking.courier_url,"target":`_blank`,"rel":`noreferrer`,});   }}
       for(let event of __sveltets_2_ensureArray(orderTracking.tracking)){let i = 1;i; { svelteHTML.createElement("div", { "class":`split review`,}); { svelteHTML.createElement("span", {});event.note; } { svelteHTML.createElement("small", {});date(event.created_at); } }} { svelteHTML.createElement("p", {});    }
   } { svelteHTML.createElement("div", { "class":`backdrop`,}); }
}
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Shopping__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Shopping__SvelteComponent_ = ReturnType<typeof Shopping__SvelteComponent_>;
/*Ωignore_endΩ*/export default Shopping__SvelteComponent_;