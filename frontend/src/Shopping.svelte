<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowLeft, ArrowRight, CreditCard, MapPin, Package, ShoppingBag, Store, Truck } from '@lucide/svelte';
  import { api, authenticate, checkedGuestCart, date, downloadReturnMedia, guestCart, guestSession, isPublicMedia, key, label, message, money, params, recommendedProducts, recordProductView, setGuest, signedIn, trackingUrl, type ItemPick, type Row } from './api';
  import { CursorPager } from './cursor-pager.svelte';
  import CursorMore from './CursorMore.svelte';
  import ProductGrid from './ProductGrid.svelte';
  import ProductInitials from './ProductInitials.svelte';
  import ResponsiveImage from './ResponsiveImage.svelte';
  import Fields from './Fields.svelte';
  let {page,parts,changed}: {page:string;parts:string[];changed:()=>Promise<void>}=$props();
  let id=$derived(Number(parts[1]));
  let authenticated=$state(signedIn());
  let data=$state<Row|null>(null), rows=$state<Row[]>([]), addresses=$state<Row[]>([]);
  let recommendations = $state.raw<Row[]>([]);
  let relatedProducts = $derived(recommendations.length ? recommendations : data?.similar ?? []);
  let error=$state(''), feedback=$state(''), loading=$state(true), pending=$state(false);
  let variantId=$state<number|undefined>(), quantity=$state(1), selected=$state<number[]>([]);
  let variant=$derived(data?.variants?.find((v:Row)=>v.id===variantId));
  let validQuantity = $derived(variant && Number.isInteger(quantity) && quantity >= 1 && quantity <= Math.min(999, variant.stock));
  const pager = new CursorPager<Row & {id:number}>();
  let q=$state(''), orderTracking=$state<Row|null>(null);
  let addressId=$state<number|undefined>(), summary=$state<Row|null>(null);
  const lifetime = new AbortController();
  let quoteController: AbortController | undefined;
  let addressKey=key();
  type Checkout = {items:ItemPick[];from_cart:boolean;guest_cart?:boolean;key:string};
  function readCheckout(): Checkout | null {
    try {
      const value = JSON.parse(sessionStorage.getItem('checkout-picks') ?? 'null');
      if (!value || typeof value.key !== 'string' || !Array.isArray(value.items) || !value.items.length
        || value.items.length > 100 || !value.items.every((item: ItemPick) => item != null && Number.isSafeInteger(item.id)
          && item.id > 0 && Number.isInteger(item.quantity) && item.quantity >= 1 && item.quantity <= 999)) return null;
      return { items: value.items, from_cart: value.from_cart === true, guest_cart: value.guest_cart === true, key: value.key };
    } catch { return null; }
  }
  const checkout=$derived(page === 'checkout' ? readCheckout() : null);
  const get = <T = Row>(path: string, signal = lifetime.signal) => api<T>(path, 'GET', undefined, true, signal);
  onMount(()=>{
    void load();
    return () => {
      lifetime.abort();
      quoteController?.abort();
      pager.cancel();
    };
  });
  async function load() {
    if(page==='orders'&&!id&&signedIn()) {
      const query=q;
      loading=false;
      await pager.reset((cursor, signal)=>get('/checkout'+params({q:query,cursor}), signal));
      return;
    }
    loading=true;error='';
    try {
      if(page==='product') {
        const product=await get('/products/'+id);
        data=product;
        variantId=product.variants.find((v:Row)=>v.stock>0)?.id??product.variants[0]?.id;
        recommendations = [];
        void recordProductView(product.id).then(() => recommendedProducts(product.id, lifetime.signal)).then(items => {
          if (data?.id === product.id) recommendations = items;
        }).catch(() => {});
      } else if(page==='cart') {
        rows=signedIn()&&!guestSession()?await get<Row[]>('/cart'):await checkedGuestCart(lifetime.signal);
        selected=rows.filter(v=>!v.reason).map(v=>v.id);
      } else if(page==='checkout' && signedIn()) {
        addresses=await get<Row[]>('/addresses');
        addressId=addresses[0]?.id;
        await getQuote();
      } else if(page==='orders'&&signedIn()) {
        if(id) data=await get('/checkout/'+id);
      } else if(page==='notifications'&&signedIn()) rows=await get<Row[]>('/notifications');
    }catch(e){if(!lifetime.signal.aborted)error=message(e);}finally{authenticated=signedIn();loading=false;}
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
      rows=signedIn()&&!guestSession()?await get<Row[]>('/cart'):await checkedGuestCart(lifetime.signal);
      selected=rows.filter(v=>!v.reason&&active.includes(v.id)).map(v=>v.id);
      await changed();
    });
  }
  function changeQuantity(event: Event & { currentTarget: HTMLInputElement }, row: Row) {
    const input = event.currentTarget;
    if (!input.validity.valid || !Number.isInteger(input.valueAsNumber)) {
      input.value = String(row.quantity);
      error = 'Enter a quantity from 1 to 999.';
      return;
    }
    const next = input.valueAsNumber;
    void updateCart(row, next).then(() => {
      input.value = String(rows.find(item => item.id === row.id)?.quantity ?? row.quantity);
    });
  }
  function start(items:ItemPick[],from_cart:boolean) {
    if (!items.length || items.length > 100) {
      error = 'Select 1 to 100 items to check out.';
      return;
    }
    const guest_cart=from_cart&&(!signedIn()||guestSession());
    sessionStorage.setItem('checkout-picks',JSON.stringify({items,from_cart:from_cart&&!guest_cart,guest_cart,key:key()}));
    location.assign('/checkout');
  }
  async function addProduct() {
    if(!data||!variant||!validQuantity)return;
    const line = variant;
    const amount = quantity;
    const item={product_id:data.id,product_name:data.name,image:data.image,shop:data.shop,variant:variant.name,subvariant:variant.subvariant,price:variant.price};
    await act(async()=>{
      if(signedIn()&&!guestSession()) {
        await api('/cart/'+line.id,'POST',{quantity:amount});
      }else {
        const old=guestCart().find(r=>r.id===line.id)?.quantity??0;
        if(old+amount>line.stock) throw new Error('This quantity exceeds available stock');
        setGuest(line.id,old+amount,item);
      }
      feedback='Added to your cart.';
      await changed();
    });
  }
  async function getQuote() {
    quoteController?.abort();
    summary=null;error='';
    if (!addressId||!checkout)return;
    const controller = new AbortController();
    quoteController = controller;
    try {
      const quote=await api('/checkout/quote','POST',{...checkout,from_cart:checkout.from_cart || Boolean(checkout.guest_cart && !guestSession()),address:addressId},true,controller.signal);
      if(!controller.signal.aborted)summary=quote;
    } catch(e){if(!controller.signal.aborted)error=message(e);}
    finally { if (quoteController === controller) quoteController = undefined; }
  }
  async function addAddress(input:Row) {
    const result=await api('/addresses','POST',{...input,key:addressKey});
    addresses=await get<Row[]>('/addresses');
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
      await authenticate('guest');
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
    location.assign(checkoutId && /^[1-9][0-9]*$/.test(checkoutId)?'/orders/'+checkoutId+'?submitted='+page:'/orders');
  }
  const submitted=new URLSearchParams(location.search).get('submitted');
</script>

{#snippet media(files:string[])}
  {@const attachments = Array.isArray(files) ? [...new Set(files.filter(isPublicMedia))] : []}
  {#if attachments.length}<div class="media-row">{#each attachments as file (file)}
    {#if /\.(mp4|webm)$/.test(file)}<video src={file} controls aria-label="Customer attachment"><track kind="captions" /></video>
    {:else}<a href={file} target="_blank" rel="noreferrer"><ResponsiveImage src={file} alt="Customer attachment" sizes="106px" uploadSize="thumbnail" /></a>{/if}
  {/each}</div>{/if}
{/snippet}
{#snippet lines(items:Row[])}
  <div class="table-scroll"><table><thead><tr><th>Item</th><th>Variant</th><th>Quantity</th><th>Price</th></tr></thead>
    <tbody>{#each items as item,i (i)}<tr><td><a href={'/product/'+item.product_id}>{item.product_name}</a></td><td>{item.variant_name??item.variant}</td><td>{item.quantity}</td><td>{money(item.price)}</td></tr>{/each}</tbody></table></div>
{/snippet}

{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if loading}<p class="inline-empty">Loading…</p>
{:else if !authenticated&&(page==='orders'||page==='checkout'||page==='notifications'||formPage)}
  <section class="panel empty"><h1>{page==='checkout' && checkout?'How would you like to check out?':'Sign in to continue'}</h1><div class="actions"><a class="primary" href={'/login'+params({next:location.pathname+location.search})} onclick={()=>{if(page==='checkout')window.dispatchEvent(new Event('tosca-checkout-auth'));}}>Sign in <ArrowRight size={16}/></a>{#if page==='checkout'&&checkout}<button disabled={pending} onclick={continueAsGuest}>Continue as guest</button>{/if}</div></section>
{:else if page==='product' && data}
  <a class="breadcrumb" href="/"><ArrowLeft size={16}/> Marketplace</a>
  <section class="detail-grid">
    <div class="detail-image">{#if data.image}<ResponsiveImage src={data.image} alt={data.name}
      sizes="(max-width: 620px) calc(100vw - 24px), (max-width: 900px) calc(50vw - 25px), (max-width: 1320px) calc(50vw - 38px), 622px"
      uploadSize="detail" loading="eager" />{:else}<ProductInitials name={data.name}/>{/if}</div>
    <div class="panel product-detail">
      <a class="shop-name" href={'/shop/'+data.shop_id}><Store size={18}/> {data.shop}</a>
      <h1>{data.name}</h1><strong class="detail-price">{money(variant?.price)}</strong>
      <p>{data.description}</p>
      {#if data.brand || data.category}<div class="tags">{#if data.brand}<span>{data.brand}</span>{/if}{#if data.category}<span>{data.category}</span>{/if}</div>{/if}
      <div class="field-row"><label>Variant<select bind:value={variantId} onchange={()=>quantity=1}>{#each data.variants as v (v.id)}<option value={v.id} disabled={!v.stock}>{v.name}{v.subvariant?' / '+v.subvariant:''}{v.stock?'':' · Out of stock'}</option>{/each}</select></label>
      <label>Quantity<input type="number" min="1" max={Math.min(999,variant?.stock??1)} bind:value={quantity}></label></div>
      {#if variant}<p class="muted">{variant.stock} in stock · {variant.grams} g</p>{/if}
      <div class="actions"><button disabled={pending||!validQuantity} onclick={addProduct}><ShoppingBag size={17}/> Add to cart</button>
        <button class="primary" disabled={pending||!validQuantity} onclick={()=>start([{id:variantId!,quantity}],false)}>Buy now <ArrowRight size={16}/></button></div>
      {#if feedback}<p class="feedback" role="status">{feedback}</p>{/if}
    </div>
  </section>
  <section class="panel section"><h2>Customer reviews</h2>{#each data.reviews as review (review.id)}<article class="review"><div class="split"><strong>{review.user}</strong><span class="rating">{review.rating}/5</span></div><p>{review.body}</p>{@render media(review.media)}{#if review.reply}<blockquote><strong>Shop reply</strong><br>{review.reply}</blockquote>{/if}</article>{:else}<p class="muted">No reviews yet.</p>{/each}</section>
  {#if relatedProducts.length}<section class="section"><h2>More to explore</h2><ProductGrid items={relatedProducts}/></section>{/if}
{:else if page==='cart'}
  <h1>Shopping cart</h1>
  {#if rows.length}
    <div class="detail-grid cart-layout"><div class="panel cart-list">
      {#each rows as row (row.id)}<article class:unavailable={row.reason} class="cart-row">
        <input type="checkbox" aria-label={'Select '+row.product_name} value={row.id} bind:group={selected} disabled={Boolean(row.reason)}>
        <a class="thumb" href={'/product/'+row.product_id} aria-label={row.product_name}>{#if row.image}<ResponsiveImage src={row.image} alt="" sizes="58px" uploadSize="thumbnail" />{:else}<ProductInitials name={row.product_name}/>{/if}</a>
        <div class="cart-product"><a href={'/product/'+row.product_id}><strong>{row.product_name}</strong></a><small>{row.variant}{row.subvariant?' / '+row.subvariant:''} · {row.shop}</small><small>Added {ago(row.created_at).toLowerCase()}</small>{#if row.reason}<span class="badge">{row.reason}</span>{/if}</div>
        <label class="quantity"><span class="sr-only">Quantity for {row.product_name}</span><input type="number" min="1" max="999" value={row.quantity} disabled={pending||Boolean(row.reason)} onchange={(e)=>changeQuantity(e,row)}></label>
        <strong>{money(row.price)}</strong><button class="text-button" disabled={pending} onclick={()=>updateCart(row,0)}>Remove</button>
      </article>{/each}
    </div><aside class="panel totals"><h2>Order summary</h2><div class="split"><span>Selected items</span><strong>{chosen.length}</strong></div><div class="split"><span>Subtotal</span><strong>{money(total)}</strong></div><button class="primary" disabled={pending||!chosen.length} onclick={()=>start(chosen.map(({id,quantity})=>({id,quantity})),true)}>Checkout <ArrowRight size={17}/></button></aside></div>
  {:else}<section class="panel empty"><ShoppingBag size={28}/><h2>Your cart is empty</h2><a class="primary" href="/">Explore products</a></section>{/if}
{:else if page==='checkout'}
  <a class="breadcrumb" href="/cart"><ArrowLeft size={16}/> Cart</a><h1>Checkout</h1>
  {#if checkout}
    <div class="detail-grid checkout-layout">
      <div class="stack">
        <section class="panel"><h2><MapPin size={20}/> Shipping address</h2>{#if addresses.length}<label class="field">Saved addresses<select bind:value={addressId} onchange={getQuote}>{#each addresses as address (address.id)}<option value={address.id}>{address.label} · {address.detail}</option>{/each}</select></label>{/if}
        <details open={!addresses.length}><summary>Add an address</summary><Fields fields={[{name:'label',label:'Address label'},{name:'detail',label:'Recipient, phone and full address',type:'textarea'}]} button="Save address" submit={addAddress}/></details></section>
        <section class="panel"><h2><CreditCard size={20}/> Payment method</h2><label class="check"><input type="radio" name="payment" checked> Manual payment</label><div class="gateway"><span>Online payments coming later</span><div>{#each ['QRIS','BCA','BRI','Mandiri'] as brand (brand)}<button disabled><ResponsiveImage src={'/payment/'+brand.toLowerCase()+'.svg'} alt={brand} sizes="51px" /></button>{/each}</div></div></section>
        {#if summary}{#each Object.entries(summary.groups) as [shop,g] (shop)}
          <section class="panel"><h3><Store size={17}/> {(g as Row).shop}</h3>{@render lines((g as Row).items)}<p>Shipping · {(g as Row).grams/1000} kg · {money((g as Row).shipping)}</p></section>
        {/each}{/if}
      </div>
      <aside class="panel totals"><h2>Order total</h2>{#if summary}<div class="split"><span>Subtotal</span><strong>{money(summary.subtotal)}</strong></div><div class="split"><span>Shipping</span><strong>{money(summary.shipping)}</strong></div><div class="split"><span>Tax</span><strong>{money(summary.tax)}</strong></div><div class="split grand"><span>Total</span><strong>{money(summary.total)}</strong></div>{/if}<button class="primary" disabled={!summary||pending} onclick={placeOrder}>Place order <ArrowRight size={17}/></button></aside>
    </div>
  {:else}<section class="panel empty"><p>Select items to check out.</p><a href="/cart">View your cart</a></section>{/if}
{:else if page==='orders'&&!id}
  <h1>My orders</h1>
  <section class="panel"><form class="filter" onsubmit={(e)=>{e.preventDefault();load();}}><input placeholder="Search orders" aria-label="Search orders" bind:value={q}><button>Search</button></form>
  <div class="table-scroll"><table><thead><tr><th>Checkout number</th><th>Date</th><th>Grand total</th><th>Status</th><th></th></tr></thead><tbody>
  {#each pager.items as row (row.id)}<tr><td class="id-cell">{row.checkout_no}</td><td>{date(row.created_at)}</td><td>{money(row.total)}</td><td><span class="badge">{row.status}</span></td><td><a href={'/orders/'+row.id}>View order <ArrowRight size={14}/></a></td></tr>{:else}<tr><td colspan="5">{#if pager.loading}Loading orders?{:else if pager.error}<span role="alert">{pager.error}</span> <button onclick={()=>load()}>Retry</button>{:else}No matching orders.{/if}</td></tr>{/each}</tbody></table></div>
  <CursorMore cursor={pager.cursor} loading={pager.loading} error={pager.error} next={()=>pager.next()}/></section>
{:else if page==='orders'&&data}
  <a class="breadcrumb" href="/orders"><ArrowLeft size={16}/> My orders</a>
  <section class="panel"><div class="split"><h1>Checkout #{data.id}</h1><span class="badge">{data.status}</span></div><p class="muted id-cell">{data.checkout_no} · {date(data.created_at)}</p><p>{data.address}</p>
    <p>Total <strong>{money(data.total)}</strong> · Shipping {money(data.shipping)} · Tax {money(data.tax)}</p>
    {#if submitted}<p class="feedback">{submitted==='return'?'Your return is in review.':'Your review was submitted.'}</p>{/if}
    {#if data.payment==='waiting'}<p>{data.payment_instructions}</p><button disabled={pending} onclick={()=>{if(confirm('Cancel this unpaid checkout?'))commit('/checkout/'+id+'/cancel');}}>Cancel checkout</button>{/if}
  </section>
  {#each data.orders as order (order.id)}
    <section class="panel section"><div class="split"><h2><Store size={18}/> {order.shop}</h2><span class="badge">{label(order.status)}</span></div>
      {@render lines(order.items)}
      <p>Subtotal {money(order.subtotal)} · Shipping {money(order.shipping)} · Tax {money(order.tax)}</p>
      <div class="actions">
        {#if order.courier}<button onclick={()=>orderTracking=order}><Truck size={17}/> Track package</button>{/if}
        {#if order.status==='delivery'}<button class="primary" disabled={pending} onclick={()=>commit('/orders/'+order.id+'/receive')}>Confirm delivery</button>{/if}
        {#if order.returnable}<a class="button" href={'/return/'+order.id+params({checkout:id})}>Request return</a>{/if}
      </div>
      {#if order.completed_at}<div class="actions section">{#each order.items.filter((item:Row,i:number)=>!item.reviewed&&order.items.findIndex((entry:Row)=>entry.product_id===item.product_id)===i) as item (item.product_id)}<a class="button" href={'/review/'+order.id+'/'+item.product_id+params({checkout:id})}>Review {item.product_name}</a>{/each}</div>{/if}
      {#if order.return}<p>{order.return.description}</p><div class="actions">{#each order.return.media as file,i (file)}<button onclick={()=>act(()=>downloadReturnMedia(file))}>Download attachment {i+1}</button>{/each}</div>{/if}
    </section>
  {/each}
{:else if formPage}
  <section class="panel auth"><h1>{page==='return'?'Request a return':'Review your purchase'}</h1>
    <Fields fields={page==='return'?[{name:'description',label:'Reason for the return',type:'textarea'},{name:'media',label:'Photo or video',type:'media',required:true}]:[{name:'rating',label:'Rating',options:[5,4,3,2,1].map(n=>({value:n,text:n+' / 5'}))},{name:'body',label:'Your review',type:'textarea'},{name:'media',label:'Photos or videos',type:'media'}]} button={page==='return'?'Submit return':'Post review'} submit={submitReview} privateMedia={page==='return'}/>
  </section>
{:else if page==='notifications'}
  <h1>Notifications</h1><section class="panel">
    <button disabled={pending} onclick={()=>commit('/notifications/read')}>Mark all read</button>
    {#each rows as row (row.id)}<div class="split review"><span>{row.message}{#if !row.read_at} <span class="badge">New</span>{/if}</span><small>{date(row.created_at)}</small></div>{:else}<p>No notifications yet.</p>{/each}
  </section>
{:else if !error}
  <section class="panel empty"><Package size={28}/><h1>Page unavailable</h1><a class="primary" href="/">Back to the marketplace</a></section>
{/if}

{#if orderTracking}
  {@const courierLink = trackingUrl(orderTracking.courier_url)}
  <dialog open class="dialog" aria-label="Shipping progress">
    <div class="split"><h2>Shipping progress</h2><button onclick={()=>orderTracking=null}>Close</button></div>
    <p>{orderTracking.courier} · {orderTracking.receipt}</p>
    {#if courierLink}<a href={courierLink} target="_blank" rel="noreferrer">Open courier tracking</a>{/if}
    {#each orderTracking.tracking as event,i (i)}<div class="split review"><span>{event.note}</span><small>{date(event.created_at)}</small></div>{:else}<p>No tracking updates yet.</p>{/each}
  </dialog><div class="backdrop"></div>
{/if}
