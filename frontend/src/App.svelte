<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowRight, Bell, ChevronDown, ChevronRight, Coffee, Headphones, House, LayoutGrid, Package, Shirt, ShoppingBag, Sparkles, Store, Truck, UserRound, Volleyball } from '@lucide/svelte';
  import { api, clear, exploreRecommendations, groupSearchHits, guestCart, guestSession, key, logout, mergeGuestCart, message, params, save, signedIn, type Row, type SearchGroups, type SearchResponse } from './api';
  import { CursorPager } from './cursor-pager.svelte';
  import CursorMore from './CursorMore.svelte';
  import ProductGrid from './ProductGrid.svelte';
  import SearchBar from './SearchBar.svelte';
  import Fields, {type Field} from './Fields.svelte';
  import ResponsiveImage from './ResponsiveImage.svelte';
  const parts = location.pathname.split('/').filter(Boolean);
  const search = new URLSearchParams(location.search);
  const page = parts[0] ?? '';
  const portal = page === 'merchant' || page === 'admin' ? page : 'customer';
  const authPage = ['login','register','forgot','reset'].includes(portal==='customer'?page:parts[1]) && !(portal==='admin'&&parts[1]==='register');
  const mode = portal==='customer'?page:parts[1];
  const browse = page === '' || page === 'search' || page === 'shop';
  const category = search.get('category');
  const storefrontHome = page === '' && !category;
  let account = $state<Row|null>(null), cartCount = $state(guestCart().reduce((sum,row)=>sum+row.quantity,0));
  let searchGroups = $state.raw<SearchGroups>({ products: [], shops: [] });
  let categories = $state<Row[]>([]);
  const pager = new CursorPager<Row & {id:number}, number | string>();
  let sort = $state<'for-you' | 'newest' | 'oldest'>(storefrontHome ? 'for-you' : 'newest');
  let hasForYou = $state(storefrontHome);
  let busy=$state(browse), error=$state(''), authNotice=$state('');
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
  let catalogRevision = 0;

  async function load() {
    const revision = ++catalogRevision;
    if(page!=='search') {
      if (sort === 'for-you') {
        const fetchPage = exploreRecommendations();
        await pager.reset(async cursor => {
          const result = await fetchPage(typeof cursor === 'string' ? cursor : null);
          if (!result.enabled && revision === catalogRevision) {
            hasForYou = false;
            sort = 'newest';
            void load();
          }
          return result;
        });
        if (revision === catalogRevision && pager.error) {
          sort = 'newest';
          await load();
        }
        return;
      }
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

  function selectSort(event: Event & { currentTarget: HTMLSelectElement }) {
    const selected = event.currentTarget.value;
    if (selected === 'for-you' || selected === 'newest' || selected === 'oldest') {
      sort = selected;
      void load();
    }
  }

  function restoreStorefront(event: PageTransitionEvent) {
    if (!storefrontHome || !event.persisted) return;
    void load();
  }

  onMount(()=>{
    const continueCheckout=()=>{continuingCheckout=true;};
    window.addEventListener('tosca-checkout-auth',continueCheckout);
    if (page === 'checkout') window.addEventListener('beforeunload', checkoutLeave);
    if(browse) {
      void load();
      api<Row[]>('/categories').then(value=>categories=value).catch(()=>{});
    }
    if(signedIn()) {
      api('/auth/me').then(value=>account=value).catch(()=>{});
      void updateCount();
    }
    return () => {
      window.removeEventListener('tosca-checkout-auth', continueCheckout);
      if (page === 'checkout') window.removeEventListener('beforeunload', checkoutLeave);
    };
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
</script>

{#snippet routeError()}
  <p class="error" role="alert">This page could not load. <button onclick={() => location.reload()}>Reload page</button></p>
{/snippet}

<svelte:window onpageshow={restoreStorefront}/>
<header class="site-header">
  <div class="header-inner wrap">
    <a href="/" class="brand" aria-label="Tosca Market home"><span class="brand-mark"><ShoppingBag size={21} strokeWidth={2.3}/></span><span>tosca<span class="brand-dot">.</span><small>MARKET</small></span></a>
    <SearchBar initialQuery={search.get('q') ?? ''}/>
    <div class="nav-actions">
      <a href={account?.shops?.length?'/merchant':'/merchant/register'} class="merchant-link"><Store size={17}/> <span>Merchant</span></a>
      <a href="/cart" class="icon-button cart-icon" aria-label={'Cart, '+cartCount+' items'}><ShoppingBag size={21}/>{#if cartCount}<span>{cartCount}</span>{/if}</a>
      {#if account}
        <details class="profile-menu"><summary><span class="avatar"><UserRound size={16}/></span><span>{account.user.name}</span><ChevronDown size={14}/></summary>
          <div class="dropdown"><a href="/profile">Profile</a><a href="/orders">Orders</a><a href="/notifications"><Bell size={15}/> Notifications</a>{#if account.user.admin}<a href="/admin">Admin dashboard</a>{/if}<button onclick={signOut}>Log out</button></div>
        </details>
      {:else}<a class="signin" href="/login"><UserRound size={17}/> <span>Sign in</span></a>{/if}
    </div>
  </div>
  {#if portal==='customer' && !authPage}
    <nav class="category-bar" aria-label="Shop categories"><div class="wrap categories">
      {#each publicLinks as entry,i (entry.name)}<a class:active={(i===0&&!category&&page==='')||entry.name===category} href={i===0?'/':'/'+params({category:entry.name})+'#catalog'}><entry.icon size={16}/>{entry.name}</a>{/each}
    </div></nav>
  {/if}
</header>

<main class="wrap main">
  {#if authPage}
    <section class="auth panel">
      <a class="breadcrumb" href="/">Marketplace <ChevronRight size={14}/> {labelPortal(portal)}</a>
      <h1>{title}</h1>
      <Fields {fields} submit={submitAuth} button={mode==='register'?'Create account':mode==='forgot'?'Send reset link':mode==='reset'?'Update password':'Sign in'} notice={authNotice}/>
      {#if mode==='login'}<p class="auth-links"><a href="/forgot">Forgot password?</a>{#if portal!=='admin'}<a href={prefix+'/register'}>Create an account</a>{/if}</p>
      {:else}<p><a href={prefix+'/login'}>Back to sign in</a></p>{/if}
      {#if portal==='merchant' && account && !account.user.guest && mode==='register'}
        <hr><h3>Open a shop with your account</h3>
        <Fields fields={[{name:'name',label:'Shop name'}]} button="Open shop" submit={async(data)=>{await api('/shops','POST',{...data,key:shopKey});location.assign('/merchant');}}/>
      {/if}
    </section>
  {:else if browse}
    {#if storefrontHome}
      <section class="hero">
        <div class="hero-content"><div class="eyebrow"><span></span> THE EVERYDAY EDIT</div><h1>Good finds.<br><em>Better days.</em></h1><p>Discover something for every day, from independent shops and brands.</p><a class="hero-button" href="#catalog">Explore products <ArrowRight size={17}/></a></div>
        <div class="hero-image">
          <ResponsiveImage src="/catalog/hero.jpg" alt="Sunlit home with thoughtfully styled furniture"
            sizes="(max-width: 620px) calc(100vw - 24px), (max-width: 900px) calc(47vw - 16px), (max-width: 1320px) calc(47vw - 28px), 592px"
            loading="eager" fetchpriority="high" />
        </div>
      </section>
      <div class="service-strip"><span><Store size={18}/> Shop independent</span><span><Package size={18}/> Delivery from every shop</span><span><Truck size={18}/> Track your orders</span></div>
    {/if}
    <section id="catalog" class="catalog">
      <div class="section-head"><div><h2>{page==='search'?'Search results':shopTitle||category||'Explore products'}</h2>{#if page==='search'}<p>Results for “{search.get('q') ?? ''}”</p>{:else if storefrontHome && sort === 'newest'}<p>Latest finds from across the marketplace.</p>{/if}</div>
        {#if page !== 'search' && (!storefrontHome || hasForYou)}
          <label class="sort">Sort by
            <select aria-label="Sort products" value={sort} onchange={selectSort}>
              {#if hasForYou}<option value="for-you">For you</option>{/if}
              <option value="newest">Newest</option>
              {#if !storefrontHome}<option value="oldest">Oldest</option>{/if}
            </select>
          </label>
        {/if}
      </div>
      {#if catalogError}
        <p class="error" role="alert">{catalogError} <button onclick={()=>load()}>Retry</button></p>
      {:else if page==='search'}
        {#if searchGroups.products.length}
          <section class="search-result-group" aria-labelledby="search-products-title">
            <h3 id="search-products-title">Products</h3>
            <ProductGrid items={searchGroups.products} showKinds/>
          </section>
        {/if}
        {#if searchGroups.shops.length}
          <section class="search-result-group" aria-labelledby="search-shops-title">
            <h3 id="search-shops-title">Shops</h3>
            <ProductGrid items={searchGroups.shops} showKinds/>
          </section>
        {/if}
        {#if !searchGroups.products.length && !searchGroups.shops.length}
          <p class="inline-empty" role="status">{busy ? 'Searching…' : 'No matching products or shops.'}</p>
        {/if}
      {:else if pager.items.length}
        <ProductGrid items={pager.items}/>
      {:else}
        <p class="inline-empty">{pager.loading ? 'Loading products…' : 'No matching products.'}</p>
      {/if}
      {#if page!=='search'}<CursorMore cursor={pager.cursor} loading={pager.loading} error={pager.error} next={()=>pager.next()}/>{/if}
    </section>
  {:else if portal!=='customer'}
    {#if account&&!account.user.guest}
      {#await import('./Workspace.svelte')}
        <p class="inline-empty" role="status">Loading workspace…</p>
      {:then { default: Workspace }}
        <Workspace {portal} {parts} {account}/>
      {:catch}
        {@render routeError()}
      {/await}
    {:else}<section class="panel empty"><h1>{labelPortal(portal)}</h1><a class="primary" href={prefix+'/login'}>Sign in</a></section>{/if}
  {:else if page==='profile'}
    <section class="panel auth"><h1>My profile</h1>{#if account}<p><strong>{account.user.name}</strong><br>{account.user.guest ? "Guest checkout on this browser" : account.user.email}</p><div class="actions"><a class="button" href="/orders">My orders</a><a class="button" href="/notifications">Notifications</a><a class="primary" href="/merchant">Merchant workspace</a></div>{:else}<a href="/login">Sign in</a>{/if}</section>
  {:else}
    {#await import('./Shopping.svelte')}
      <p class="inline-empty" role="status">Loading…</p>
    {:then { default: Shopping }}
      <Shopping {page} {parts} changed={updateCount}/>
    {:catch}
      {@render routeError()}
    {/await}
  {/if}
</main>
<footer class="site-footer"><div class="wrap"><a href="/" class="footer-brand">tosca<span>.</span> market</a><nav aria-label="Footer"><a href="/merchant">Merchant workspace</a><a href="/orders">My orders</a><a href="/admin/login">Admin</a></nav></div></footer>

<script lang="ts" module>
  const labelPortal=(portal:string)=>({customer:'Customer',merchant:'Merchant',admin:'Admin'})[portal]??portal;
</script>

<style>
  .search-result-group + .search-result-group { margin-top: 24px; }
  .search-result-group h3 { margin: 0 0 10px; font-size: 16px; }
</style>
