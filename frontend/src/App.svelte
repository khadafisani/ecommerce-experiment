<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowRight, Bell, ChevronDown, ChevronRight, Package, ShoppingBag, Store, Truck, UserRound } from '@lucide/svelte';
  import { api, authenticate, exploreRecommendations, groupSearchHits, guestCart, guestSession, key, localRedirect, logout, message, params, signedIn, watchSessionIdentity, type Row, type SearchGroups, type SearchResponse } from './api';
  import { CursorPager } from './cursor-pager.svelte';
  import CursorMore from './CursorMore.svelte';
  import CategoryNav from './CategoryNav.svelte';
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
  const resetToken = mode === 'reset'
    ? new URLSearchParams(location.hash.slice(1)).get('token') ?? search.get('token')
    : null;
  if (resetToken) {
    search.delete('token');
    history.replaceState(history.state, '', location.pathname + (search.size ? '?' + search : ''));
  }
  const browse = page === '' || page === 'search' || page === 'shop';
  const category = search.get('category');
  const storefrontHome = page === '' && !category;
  let account = $state<Row|null>(null), cartCount = $state(guestCart().reduce((sum,row)=>sum+row.quantity,0));
  let searchGroups = $state.raw<SearchGroups>({ products: [], shops: [] });
  const pager = new CursorPager<Row & {id:number}, number | string>();
  let sort = $state<'for-you' | 'newest' | 'oldest'>(storefrontHome ? 'for-you' : 'newest');
  let hasForYou = $state(storefrontHome);
  let busy=$state(browse), error=$state(''), authNotice=$state(''), signOutPending=$state(false), signOutError=$state('');
  const prefix = portal==='customer'?'':'/'+portal;
  const email: Field = {name:'email',label:'Email address',type:'email'};
  const password: Field = {name:'password',label:'Password',type:'password'};
  let fields: Field[] = $derived(mode==='forgot'?[email]:mode==='reset'?[password]:mode==='login'?[email,password]:[{name:'name',label:'Full name'},email,password,...(portal==='merchant'?[{name:'shop',label:'Shop name'}]:[])]);
  const title = $derived(mode==='forgot'?'Recover your account':mode==='reset'?'Set a new password':mode==='register'?(portal==='merchant'?'Open your shop':'Create an account'):(portal==='customer'?'Welcome back':portal==='merchant'?'Merchant sign in':'Admin sign in'));
  let shopTitle = $derived(page==='shop' ? pager.items[0]?.shop ?? 'Shop catalog' : '');
  let catalogError = $derived(page==='search' ? error : pager.cursor===null ? pager.error : '');
  const shopKey=key();
  let catalogRevision = 0;
  const lifetime = new AbortController();

  async function load() {
    const revision = ++catalogRevision;
    if(page!=='search') {
      if (sort === 'for-you') {
        const fetchPage = exploreRecommendations();
        await pager.reset(async (cursor, signal) => {
          const result = await fetchPage(typeof cursor === 'string' ? cursor : null, signal);
          if (!result.enabled && !signal.aborted && revision === catalogRevision) {
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
      await pager.reset((cursor, signal)=>api('/products'+params({...filters,cursor}), 'GET', undefined, true, signal));
      return;
    }
    busy=true;error='';
    try {
      const result=await api<SearchResponse>('/search'+params({q:search.get('q')}), 'GET', undefined, true, lifetime.signal);
      searchGroups=groupSearchHits(result);
    }catch(e){if(!lifetime.signal.aborted)error=message(e);}finally{busy=false;}
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
    const unwatch = watchSessionIdentity(() => {
      continuingCheckout = true;
      location.reload();
    });
    const continueCheckout=()=>{continuingCheckout=true;};
    window.addEventListener('tosca-checkout-auth',continueCheckout);
    if (page === 'checkout') window.addEventListener('beforeunload', checkoutLeave);
    if(browse) {
      void load();
    }
    if(signedIn()) {
      api('/auth/me', 'GET', undefined, true, lifetime.signal).then(value=>account=value).catch(()=>{});
      void updateCount();
    }
    return () => {
      unwatch();
      lifetime.abort();
      catalogRevision++;
      pager.cancel();
      window.removeEventListener('tosca-checkout-auth', continueCheckout);
      if (page === 'checkout') window.removeEventListener('beforeunload', checkoutLeave);
    };
  });
  async function updateCount() {
    try { cartCount=(signedIn()&&!guestSession()?await api<Row[]>('/cart', 'GET', undefined, true, lifetime.signal):guestCart()).reduce((sum,row)=>sum+row.quantity,0); } catch {}
  }
  async function submitAuth(data:Row) {
    if(mode==='forgot') {await api('/auth/forgot','POST',data);authNotice='If an account matches this email, a reset link will arrive shortly.';return;}
    if(mode==='reset') {await api('/auth/reset','POST',{...data,token:resetToken});location.assign('/login');return;}
    if (mode !== 'login' && mode !== 'register') return;
    await authenticate(mode, {...data,portal}, { importGuestCart: portal === 'customer' });
    location.assign(localRedirect(search.get('next'), prefix||'/'));
  }
  async function signOut() {
    if (signOutPending) return;
    signOutPending = true;
    signOutError = '';
    try { await logout(); location.assign('/'); }
    catch (e) { signOutError=message(e); }
    finally { signOutPending=false; }
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
          <div class="dropdown"><a href="/profile">Profile</a><a href="/orders">Orders</a><a href="/notifications"><Bell size={15}/> Notifications</a>{#if account.user.admin}<a href="/admin">Admin dashboard</a>{/if}<button disabled={signOutPending} onclick={signOut}>Log out</button>{#if signOutError}<p class="error" role="alert">{signOutError}</p>{/if}</div>
        </details>
      {:else}<a class="signin" href="/login"><UserRound size={17}/> <span>Sign in</span></a>{/if}
    </div>
  </div>
  {#if portal==='customer' && !authPage}
    <CategoryNav {category} {storefrontHome} />
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
