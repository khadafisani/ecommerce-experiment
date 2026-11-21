<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowLeft, ArrowRight, Boxes, ClipboardList, LayoutDashboard, Plus, Store, Users, Wallet } from '@lucide/svelte';
  import { api, date, downloadReturnMedia, key, label, message, money, params, type Row } from './api';
  import { CursorPager } from './cursor-pager.svelte';
  import CursorMore from './CursorMore.svelte';
  import Chart from './Chart.svelte';
  import ProductInitials from './ProductInitials.svelte';
  import RecordHistory from './RecordHistory.svelte';
  import ResponsiveImage from './ResponsiveImage.svelte';
  import Fields, {type Field} from './Fields.svelte';
  let {portal,parts,account}: {portal:string;parts:string[];account:Row}=$props();
  let admin=$derived(portal==='admin');
  let activeShop=$derived(account.shops.find((s:Row)=>s.id===Number(parts[1])) ?? (!parts[1]?account.shops[0]:null));
  let section=$derived((admin?parts[1]:parts[2])??'dashboard');
  let record=$derived(admin?parts[2]:parts[3]);
  let base=$derived(admin?'/admin':'/merchant/'+activeShop?.id);
  let error=$state(''), feedback=$state(''), pending=$state(false), loading=$state(true);
  let data=$state<Row>({}), rows=$state<Row[]>([]), couriers=$state<Row[]>([]), terms=$state<Row[]>([]);
  let historyRevision=$state(0);
  let editor=$state<{ title:string;fields:Field[];action:(data:Row)=>Promise<void> }|null>(null);
  let status=$state(''), sort=$state(''), duration=$state('day'), q=$state(''), shopFilter=$state(''), userFilter=$state('');
  let lineQuery=$state('');
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let loadController: AbortController | undefined;
  const searchDelay = 250;
  const pager = new CursorPager<Row & {id:number}>();
  let paged = $derived(!admin && !record && (section==='products'||section==='orders'));
  const adminSections=['dashboard','products','taxonomy','reviews','users','shops','couriers','payments'];
  const shopSections=['dashboard','products','orders','team'];
  const navigation:Record<string,string>={dashboard:'Overview',products:'Products',taxonomy:'Brands & categories',reviews:'Reviews',users:'Users',shops:'Shops',couriers:'Couriers',payments:'Manual payments',orders:'Orders',team:'Team'};
  const recordNames:Record<string,string>={products:'Product',taxonomy:'Taxonomy term',reviews:'Review',users:'User',shops:'Shop',couriers:'Courier',payments:'Checkout',orders:'Order',team:'Team member'};
  let adminRecord=$derived(admin && record && section!=='dashboard' && adminSections.includes(section));
  let historyPath=$derived(base+'/'+section+'/'+record+'/history');
  const navIcons:Record<string,typeof Store>={dashboard:LayoutDashboard,products:Boxes,orders:ClipboardList,users:Users,team:Users,payments:Wallet};
  const statuses=['waiting_payment','waiting_shop','packing','delivery','completed','rejected','return_review','return_accepted','return_rejected','return_completed','cancelled'];
  const productStatuses=['pending','active','inactive','rejected'];
  let statusesForSection=$derived(section==='products'&&!record?productStatuses:section==='taxonomy'?['pending','approved','rejected']:section==='payments'?['waiting','paid','cancelled']:section==='reviews'?['hidden']:!admin&&(section==='dashboard'||section==='orders')?statuses:[]);
  const shopKey=key();
  onMount(()=>{
    if(!admin&&section==='products'&&record&&record!=='new')duration='week';
    void load();
    return ()=>{
      clearTimeout(searchTimer);
      loadController?.abort();
      pager.cancel();
    };
  });
  function queueSearch(event: Event & { currentTarget: HTMLInputElement }) {
    clearTimeout(searchTimer);
    if (event instanceof InputEvent && event.isComposing) return;
    if (event.currentTarget.form?.matches(':invalid')) return;
    searchTimer = setTimeout(() => void load(), searchDelay);
  }
  async function load() {
    clearTimeout(searchTimer);
    loadController?.abort();
    if(!admin&&!activeShop||admin&&!account.user.admin){loading=false;return;}
    const controller = new AbortController();
    loadController = controller;
    const get = <T = Row>(path: string) => api<T>(path, 'GET', undefined, true, controller.signal);
    if(paged) {
      const route=base+'/'+section;
      const filters={q,status};
      loading=false;error='';
      await pager.reset((cursor, signal)=>api(route+params({...filters,cursor}), 'GET', undefined, true, signal));
      return;
    }
    loading=true;error='';
    try{
      if(adminRecord) return;
      if(section==='dashboard') data=await get(base+'/dashboard'+params(admin?{status:duration,q,sort}:{status,sort:duration}));
      else if(!admin&&record&&record!=='new') {
        data=await get(base+'/'+section+'/'+record+params({status:section==='products'?duration:null,sort}));
        if(section==='orders') couriers=await get<Row[]>('/couriers');
      } else if(!admin&&record==='new') terms=await get<Row[]>('/categories');
      else {
        rows=await get<Row[]>(base+'/'+section+params({q,status,sort,shop:shopFilter,category:userFilter}));
      }
    }catch(e){if(!controller.signal.aborted)error=message(e);}
    finally{if(!controller.signal.aborted)loading=false;}
  }
  async function perform(path:string,method='POST',body?:Row) {
    if(pending)return;pending=true;error='';feedback='';
    try{await api(path,method,body);historyRevision++;feedback='Saved.';await load();}catch(e){error=message(e);}finally{pending=false;}
  }
  function form(title:string,fields:Field[],submit:(data:Row)=>Promise<unknown>,success?:string) {
    editor={title,fields,action:async(data)=>{await submit(data);editor=null;if(success)location.assign(success);else {historyRevision++;feedback='Saved.';await load();}}};
  }
  const name=(value?:string):Field=>({name:'name',label:'Name',value});
  const note=(label='Note'):Field=>({name:'note',label,type:'textarea'});
  const accountFields=[name(),{name:'email',label:'Email address',type:'email'},{name:'password',label:'Initial password',type:'password'}];
  function productFields(p?:Row):Field[] {
    return [name(p?.name),{name:'description',label:'Description',type:'textarea',value:p?.description},{name:'image',label:'Product image',type:'image',value:p?.image},
      {name:'brand',label:'Brand',value:p?.brand,required:false},{name:'category',label:'Category',value:p?.category,required:false}];
  }
  function variantFields(v?:Row):Field[] {
    return [name(v?.name??'Standard'),{name:'subvariant',label:'Subvariant',value:v?.subvariant,required:false},
      {name:'price',label:'Price (Rp)',type:'number',value:v?.price},{name:'stock',label:'Stock',type:'number',value:v?.stock??0},{name:'grams',label:'Weight (grams)',type:'number',value:v?.grams},
      ...(v?[{name:'enabled',label:'Available',type:'checkbox',value:Boolean(v.enabled)}]:[])];
  }
  const productKey=key();
  async function createProduct(input:Row) {
    const {v_name,v_subvariant,v_price,v_stock,v_grams,...product}=input;
    const result=await api(base+'/products','POST',{...product,key:productKey,variant:{name:v_name,subvariant:v_subvariant,price:v_price,stock:v_stock,grams:v_grams}});
    location.assign(base+'/products/'+result.id);
  }
  function newVariant() {
    const requestKey=key();
    form('Add a variant',variantFields(),data=>api(base+'/products/'+record+'/variants','POST',{...data,key:requestKey}));
  }
  function moderate(p:Row,status:string) {
    if(status==='active')void perform('/admin/products/'+p.id,'PUT',{status});
    else form(label(status)+' product',[note('Reason for the merchant')],data=>api('/admin/products/'+p.id,'PUT',{...data,status}));
  }
  function courierEditor(row?:Row) {
    form(row?'Edit courier':'Add courier',[name(row?.name),{name:'logo',label:'Courier logo',type:'image',value:row?.logo},{name:'url',label:'Tracking URL',type:'url',value:row?.url,required:false}],data=>api('/admin/couriers','POST',{...data,id:row?.id}));
  }
  async function editTaxonomy(row:Row) {
    try{
      const terms=await api<Row[]>('/admin/taxonomy?status=approved');
      form('Replace proposed '+row.kind,[{name:'replacement',label:'Approved '+row.kind,options:terms.filter(v=>v.kind===row.kind).map(v=>({value:v.id,text:v.name}))}],data=>api('/admin/taxonomy/'+row.id,'PUT',{status:'rejected',replacement:Number(data.replacement)}));
    }catch(e){error=message(e);}
  }
  function dispatch() {
    form('Dispatch order',[{name:'courier',label:'Expedition',options:couriers.map(v=>({value:v.id,text:v.name}))},{name:'receipt',label:'Receipt number'}],data=>api(base+'/orders/'+record,'POST',{...data,status:'delivery',courier:Number(data.courier)}));
  }
  function tracking() {
    const trackingKey=key();
    form('Add tracking update',[note('Shipping progress')],data=>api(base+'/orders/'+record,'POST',{...data,status:'tracking',key:trackingKey}));
  }
  function stockHealth(row:Row) {
    if(row.status==='pending')return 'In review';
    if(row.status!=='active')return 'Inactive';
    if(Number(row.stock)===0)return 'Out of stock';
    return row.stock<=Math.max(1,row.recent_sold/4)?'Low stock':'Healthy';
  }
  function setEmployeeAccess(employee:Row) {
    const active=!employee.active;
    if(!active&&!confirm('Deactivate shop access for '+employee.name+'?'))return;
    void perform(base+'/team/'+employee.id,'PUT',{active});
  }
  function resetEmployeePassword(employee:Row) {
    form('Reset password for '+employee.name,[{name:'password',label:'New password',type:'password'}],
      data=>api(base+'/team/'+employee.id,'PUT',data));
  }
  function orderStatus(status:string) {void perform(base+'/orders/'+record,'POST',{status});}
</script>

{#snippet productActions(row:Row)}
  {#if row.status==='pending'}<button disabled={pending} onclick={()=>moderate(row,'active')}>Approve</button><button disabled={pending} onclick={()=>moderate(row,'rejected')}>Reject</button>{/if}
  {#if row.status==='active'}<button disabled={pending} onclick={()=>moderate(row,'inactive')}>Make inactive</button>{/if}
{/snippet}
{#snippet taxonomyActions(row:Row)}
  {#if row.status!=='approved'}<button disabled={pending} onclick={()=>perform('/admin/taxonomy/'+row.id,'PUT',{status:'approved'})}>Approve</button>{/if}
  {#if row.status==='pending'}<button disabled={pending} onclick={()=>perform('/admin/taxonomy/'+row.id,'PUT',{status:'rejected'})}>Reject</button><button onclick={()=>editTaxonomy(row)}>Replace</button>{/if}
{/snippet}
{#snippet otherActions(row:Row)}
  {#if section==='team'&&row.role==='Employee'}
    <button disabled={pending} onclick={()=>setEmployeeAccess(row)}>{row.active?'Deactivate access':'Reactivate access'}</button>
    {#if row.can_reset_password}<button disabled={pending} onclick={()=>resetEmployeePassword(row)}>Reset password</button>{/if}
  {/if}
  {#if section==='users'||section==='shops'}<button disabled={pending} onclick={()=>perform(base+'/'+section+'/'+row.id,'PUT',{suspended:!row.suspended})}>{row.suspended?'Restore':'Suspend'}</button>{/if}
  {#if section==='users'}<button onclick={()=>form('Change password',[{name:'password',label:'New password',type:'password'}],input=>api(base+'/users/'+row.id,'PUT',input))}>Change password</button>{/if}
  {#if section==='reviews'&&!row.deleted}<button disabled={pending} onclick={()=>{if(confirm('Hide this review from the marketplace?'))perform(base+'/reviews/'+row.id,'PUT');}}>Hide review</button>{/if}
  {#if section==='couriers'&&row.id!==1}<button onclick={()=>courierEditor(row)}>Edit</button>{/if}
  {#if section==='payments'}<a href={'/orders/'+row.id}>View checkout</a>{#if row.payment==='waiting'}<button disabled={pending} onclick={()=>{if(confirm('Confirm that manual payment has been received?'))perform('/checkout/'+row.id+'/pay');}}>Mark paid</button>{/if}{/if}
{/snippet}
{#snippet adminActions(row:Row)}
  {#if section==='products'}{@render productActions(row)}
  {:else if section==='taxonomy'}{@render taxonomyActions(row)}
  {:else}{@render otherActions(row)}{/if}
{/snippet}
{#snippet productRows(list:Row[])}
  <div class="table-scroll"><table><thead><tr><th>Product</th><th>{admin?'Shop':'Lowest price'}</th><th>{admin?'Taxonomy':'Sold / Stock'}</th><th>Status</th><th>{admin?'Created':'Last updated by'}</th><th>Actions</th></tr></thead><tbody>
  {#each list as row (row.id)}<tr><td><div class="table-item">{#if row.image}<ResponsiveImage src={row.image} alt="" sizes="40px" uploadSize="thumbnail" />{:else}<ProductInitials name={row.name}/>{/if}<div><strong>{row.name}</strong>{#if admin}<small>{row.description}</small>{/if}{#if row.note}<small>{row.note}</small>{/if}</div></div></td><td>{admin?row.shop:money(row.lowest_price)}</td><td>{admin?[row.brand,row.category].filter(Boolean).join(' / ')||'—':row.sold+' / '+row.stock+(Number(row.stock)===0?' · Out of stock':'')}</td><td><span class="badge">{label(row.status)}</span></td><td>{admin?date(row.created_at):row.actor}</td><td class="row-actions">
    <a href={base+'/products/'+row.id}>Details <ArrowRight size={15}/></a>
    {#if admin}{@render productActions(row)}{/if}
  </td></tr>{:else}<tr><td colspan="6">No matching products.</td></tr>{/each}
  </tbody></table></div>
{/snippet}
{#snippet orderRows(list:Row[])}
  <div class="table-scroll"><table><thead><tr><th>Order</th><th>Date</th><th>Total</th><th>Status</th><th>Last updated by</th><th></th></tr></thead><tbody>
    {#each list as row (row.id)}<tr><td>#{row.id}</td><td>{date(row.created_at)}</td><td>{money(row.subtotal+row.shipping+row.tax)}</td><td><span class="badge">{label(row.status)}</span></td><td>{row.actor}</td><td><a href={base+'/orders/'+row.id}>Details <ArrowRight size={15}/></a></td></tr>
    {:else}<tr><td colspan="6">No matching orders.</td></tr>{/each}
  </tbody></table></div>
{/snippet}
{#snippet taxonomyRows(list:Row[])}
  <div class="table-scroll"><table><thead><tr><th>Name</th><th>Type</th><th>Status</th><th>Date</th><th>Actions</th></tr></thead><tbody>
    {#each list as row (row.id)}<tr><td>{row.name}</td><td>{label(row.kind)}</td><td><span class="badge">{label(row.status)}</span></td><td>{date(row.created_at)}</td><td class="row-actions">
      <a href={'/admin/taxonomy/'+row.id}>Details <ArrowRight size={15}/></a>
      {@render taxonomyActions(row)}
    </td></tr>{:else}<tr><td colspan="5">No matching terms.</td></tr>{/each}
  </tbody></table></div>
{/snippet}
{#snippet filter(searchable:boolean,sortOptions=false)}
  <form class="filter" onsubmit={(e)=>{e.preventDefault();load();}}>
    {#if searchable}<input aria-label="Search records" placeholder="Search records" bind:value={q} oninput={queueSearch} oncompositionend={queueSearch}>{/if}
    {#if statusesForSection.length}<select aria-label="Filter by status" bind:value={status} onchange={()=>load()}><option value="">{section==='reviews'?'Visible':'All statuses'}</option>{#each statusesForSection as value (value)}<option value={value}>{label(value)}</option>{/each}</select>{/if}
    {#if sortOptions}<select aria-label="Sort by date" bind:value={sort} onchange={()=>load()}><option value="">{section==='dashboard'&&admin?'Oldest first':'Newest first'}</option><option value={section==='dashboard'&&admin?'newest':'oldest'}>{section==='dashboard'&&admin?'Newest first':'Oldest first'}</option></select>{/if}
    {#if section==='reviews'}<input aria-label="Shop ID" placeholder="Shop ID" inputmode="numeric" bind:value={shopFilter} oninput={queueSearch} oncompositionend={queueSearch}><input type="email" aria-label="Reviewer email" placeholder="Reviewer email" bind:value={userFilter} oninput={queueSearch} oncompositionend={queueSearch}>{/if}
    {#if searchable}<button>Search</button>{/if}
  </form>
{/snippet}
{#snippet periods(values:string[])}
  <label class="period">Period <select bind:value={duration} onchange={()=>load()}>{#each values as value (value)}<option value={value}>{({day:'Daily',week:'Weekly',month:'Monthly',year:'Yearly'} as Record<string,string>)[value]}</option>{/each}</select></label>
{/snippet}

{#if admin&&!account.user.admin}
  <section class="panel empty"><h1>Admin access required</h1><a href="/admin/login">Sign in as admin</a></section>
{:else if !admin&&!activeShop}
  <section class="panel auth"><h1>{parts[1]?'Shop unavailable':'Open your shop'}</h1>
  {#if !parts[1]}<Fields fields={[{name:'name',label:'Shop name'}]} button="Open shop" submit={async(data)=>{const result=await api('/shops','POST',{...data,key:shopKey});location.assign('/merchant/'+result.id);}}/>{:else}<a href="/merchant">Go to your shops</a>{/if}</section>
{:else}
  <div class="work-layout">
  <aside class="work-nav">
    <h2>{admin?'Admin':activeShop?.name}</h2>
    {#if !admin && account.shops.length>1}<select aria-label="Switch shop" value={activeShop?.id} onchange={(e)=>location.assign('/merchant/'+e.currentTarget.value)}>{#each account.shops as shop (shop.id)}<option value={shop.id}>{shop.name}</option>{/each}</select>{/if}
    <nav aria-label="Workspace navigation">{#each (admin?adminSections:shopSections) as entry (entry)}
      {#if entry!=='team'||activeShop?.owner}
        {@const Icon=navIcons[entry]??Store}
        <a class:active={section===entry} href={entry==='dashboard'?base:base+'/'+entry}><Icon size={17}/>{navigation[entry]}</a>
      {/if}
    {/each}</nav>
    <a class="breadcrumb" href="/"><ArrowLeft size={15}/> Marketplace</a>
  </aside>
  <div class="workspace">
    {#if record}<a class="breadcrumb" href={base+'/'+section}><ArrowLeft size={15}/> {navigation[section]}</a>{/if}
    <div class="section-head"><h1>{record==='new'&&!admin?'New product':record?(data.name??recordNames[section]+' #'+record):navigation[section]}</h1>
    {#if adminRecord || (record && data.id && ['products','orders','team'].includes(section))}<a class="button" href="#record-history">View history</a>{/if}
    {#if !admin&&section==='products'&&!record}<a class="primary" href={base+'/products/new'}><Plus size={17}/> New product</a>{/if}
    {#if !record&&admin&&section==='users'}<button class="primary" onclick={()=>form('Create administrator',accountFields,data=>api('/admin/accounts','POST',data))}><Plus size={17}/> Add admin</button>{/if}
    {#if !record&&!admin&&section==='team'&&activeShop?.owner}<button class="primary" onclick={()=>form('Create employee account',accountFields,data=>api(base+'/team','POST',data))}><Plus size={17}/> Add employee</button>{/if}
    {#if !record&&admin&&section==='couriers'}<button class="primary" onclick={()=>courierEditor()}><Plus size={17}/> Add courier</button>{/if}
    {#if !record&&admin&&section==='taxonomy'}<button class="primary" onclick={()=>form('Add approved term',[name(),{name:'kind',label:'Type',options:[{value:'brand',text:'Brand'},{value:'category',text:'Category'}]}],data=>api('/admin/taxonomy','POST',data))}><Plus size={17}/> Add term</button>{/if}
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if feedback}<p class="feedback" role="status">{feedback}</p>{/if}
    {#if paged&&pager.cursor===null&&pager.error}<p class="error" role="alert">{pager.error} <button onclick={()=>load()}>Retry</button></p>{/if}
    {#if (loading&&!Object.keys(data).length&&!rows.length) || (paged&&pager.loading&&!pager.items.length)}<p class="inline-empty">Loading…</p>{/if}

    {#if adminRecord}
      <RecordHistory path={historyPath} title={recordNames[section]+' history'} revision={historyRevision} actions={adminActions} anchor="record-history"/>
      {#if section==='products'}
        <RecordHistory path={historyPath} title="Variant history" kind="variants" revision={historyRevision}/>
        <RecordHistory path={historyPath} title="Review history" kind="reviews" revision={historyRevision}/>
      {:else if section==='payments'}
        <RecordHistory path={historyPath} title="Order history" kind="orders" revision={historyRevision}/>
        <RecordHistory path={historyPath} title="Return history" kind="returns" revision={historyRevision}/>
      {/if}
    {:else if section==='dashboard'}
      {@render periods(admin?['day','week','month','year']:['day','month','year'])}
      <div class="detail-grid chart-grid"><Chart title="Sales" points={data.sales??[]} currency/>{#if admin}<Chart title="Unique signed-in users" points={data.users??[]}/>{:else}<Chart title="Product stock health" points={data.stock??[]} pie/>{/if}</div>
      <section class="panel section"><h2>{admin?'Moderation queue':'Recent orders'}</h2>{@render filter(admin,admin)}{#if admin}<h3>Products awaiting review</h3>{@render productRows(data.products??[])}<h3 class="section">Brands & categories awaiting review</h3>{@render taxonomyRows(data.taxonomy??[])}{:else}{@render orderRows(data.orders??[])}{/if}</section>
    {:else if section==='products'&&record==='new'&&!admin}
      <section class="panel"><h2>Product information</h2>
        {#if terms.length}<details><summary>Approved brands and categories</summary><p>{terms.map(term=>term.name).join(' · ')}</p></details>{/if}
        <Fields fields={[...productFields(),...variantFields().map(field=>({...field,name:'v_'+field.name,label:field.name==='name'?'First variant name':field.label}))]} button="Submit for review" submit={createProduct}/>
      </section>
    {:else if section==='products'&&record&&!admin&&data.id}
      <section class="panel"><div class="split"><span class="badge">{label(data.status)}</span><button onclick={()=>form('Edit and resubmit product',productFields(data),input=>api(base+'/products/'+record,'PUT',input))}>Edit product</button></div>
        {#if data.image}<ResponsiveImage class="record-image" src={data.image} alt={data.name} sizes="230px" />{:else}<div class="record-image record-placeholder"><ProductInitials name={data.name}/></div>{/if}<p>{data.description}</p>{#if data.note}<p class="feedback">{data.note}</p>{/if}
        <p class="muted">{data.brand??'—'} / {data.category??'—'} · Last updated by {data.actor}</p><p>Orders <strong>{data.counts.reduce((total:number,count:Row)=>total+Number(count.value),0)}</strong> · Sold <strong>{data.sold}</strong> · Stock <strong>{data.stock}</strong></p><div class="tags">{#each data.counts as count (count.status)}<span>{label(count.status)} · {count.value}</span>{/each}</div>
      </section>
      <section class="panel section"><div class="split"><h2>Variants</h2><button onclick={newVariant}><Plus size={16}/> Add variant</button></div>
        {#each data.variants as v (v.id)}<div class="split review"><span>{v.name}{v.subvariant?' / '+v.subvariant:''} · {money(v.price)} · {v.stock} in stock {v.enabled?'':' · Disabled'}</span><button onclick={()=>form('Edit variant',variantFields(v),input=>api(base+'/products/'+record+'/variants/'+v.id,'PUT',input))}>Edit</button></div>{/each}
      </section>
      <div class="section">{@render periods(['week','month','year'])}<Chart title="Units sold" points={data.series}/></div>
      <section class="panel section"><h2>Related orders</h2>{@render filter(false,true)}{@render orderRows(data.orders)}</section>
      <section class="panel section"><h2>Reviews</h2>{#each data.reviews as review (review.id)}<article class="review"><strong>{review.user} · {review.rating}/5</strong><p>{review.body}</p>{#if review.reply}<blockquote>{review.reply}</blockquote>{/if}<button onclick={()=>form('Reply to review',[{name:'body',label:'Reply',type:'textarea',value:review.reply}],input=>api(base+'/reviews/'+review.id,'PUT',input))}>Reply</button></article>{:else}<p>No reviews yet.</p>{/each}</section>
      <RecordHistory path={historyPath} title="Product history" revision={historyRevision} anchor="record-history"/>
      <RecordHistory path={historyPath} title="Variant history" kind="variants" revision={historyRevision}/>
      <RecordHistory path={historyPath} title="Review history" kind="reviews" revision={historyRevision}/>
    {:else if section==='products'}
      <section class="panel">{@render filter(true,admin)}{@render productRows(paged?pager.items:rows)}</section>
    {:else if section==='orders'&&record&&data.id}
      <section class="panel">
        <div class="split"><span class="badge">{label(data.status)}</span><span>Last updated by {data.actor}</span></div><p>{data.address}</p><p>Total <strong>{money(data.subtotal+data.shipping+data.tax)}</strong> · Shipping {money(data.shipping)} · Tax {money(data.tax)}</p>
        <div class="actions">
          {#if data.status==='waiting_shop'}<button class="primary" disabled={pending} onclick={()=>orderStatus('packing')}>Accept order</button><button disabled={pending} onclick={()=>orderStatus('rejected')}>Reject order</button>{/if}
          {#if data.status==='packing'}<button class="primary" onclick={dispatch}>Dispatch order</button>{/if}
          {#if data.status==='delivery'&&data.courier_id===1}<button onclick={tracking}>Add tracking update</button>{/if}
          {#if data.status==='return_review'}<button class="primary" disabled={pending} onclick={()=>orderStatus('return_accepted')}>Accept return</button><button disabled={pending} onclick={()=>orderStatus('return_rejected')}>Reject return</button>{/if}
          {#if data.status==='return_accepted'}<button class="primary" disabled={pending} onclick={()=>orderStatus('return_completed')}>Complete return</button>{/if}
        </div>
        {#if data.courier}<p>{data.courier} · {data.receipt}</p>{/if}
        {#if data.return}<h3>Return evidence</h3><p>{data.return.description}</p>{#each data.return.media as url (url)}<button onclick={()=>downloadReturnMedia(url).catch(e=>error=message(e))}>Download attachment</button>{/each}{/if}
        {#each data.tracking as event,i (i)}<div class="split review"><span>{event.note}</span><span>{date(event.created_at)}</span></div>{/each}
      </section>
      <section class="panel section"><h2>Order items</h2><input aria-label="Search items" placeholder="Search items" bind:value={lineQuery}>
      <div class="table-scroll"><table><thead><tr><th>Item</th><th>Variant</th><th>Quantity</th><th>Unit price</th></tr></thead><tbody>
      {#each data.items.filter((item:Row)=>(item.product_name+' '+item.variant_name).toLowerCase().includes(lineQuery.toLowerCase())) as item,i (i)}<tr><td><a href={base+'/products/'+item.product_id}>{item.product_name}</a></td><td>{item.variant_name}</td><td>{item.quantity}</td><td>{money(item.price)}</td></tr>{:else}<tr><td colspan="4">No items match.</td></tr>{/each}
      </tbody></table></div></section>
      <RecordHistory path={historyPath} title="Order history" revision={historyRevision} anchor="record-history"/>
      <RecordHistory path={historyPath} title="Return history" kind="returns" revision={historyRevision}/>
    {:else if section==='orders'}
      <section class="panel">{@render filter(false)}{@render orderRows(pager.items)}</section>
    {:else if section==='taxonomy'&&admin}
      <section class="panel">{@render filter(true,true)}{@render taxonomyRows(rows)}</section>
    {:else if section==='team'&&record}
      {#if data.id}
        <section class="panel">
          <div class="split"><p>{data.email} · {data.role} · {data.suspended?'Suspended':data.active?'Active':'Inactive'}</p><div class="actions">{@render otherActions(data)}</div></div>
        </section>
        <RecordHistory path={historyPath} title="Team access history" revision={historyRevision} anchor="record-history"/>
      {/if}
    {:else}
      <section class="panel">
        {#if admin}{@render filter(true,section==='reviews')}{/if}
        <div class="table-scroll"><table><thead><tr><th>Name</th><th>Details</th><th>Status</th><th>Actions</th></tr></thead><tbody>
          {#each rows as row (row.id)}<tr><td><strong>{row.name}</strong>{#if section==='couriers'&&row.logo}<ResponsiveImage class="table-logo" src={row.logo} alt="" sizes="36px" uploadSize="thumbnail" />{/if}</td>
          <td>{#if section==='payments'}<span class="id-cell">{row.checkout_no}</span><br>{money(row.total)} · {date(row.created_at)}{:else if section==='reviews'}{row.user} · {row.shop} · {date(row.created_at)}<br>{row.body}{:else}{row.email??row.owner??row.url??'—'}{/if}</td>
          <td>{section==='reviews'?row.rating+'/5'+(row.deleted?' · Hidden':''):section==='payments'?label(row.payment):(section==='team' ? row.role+' · '+(row.suspended?'Suspended':row.active?'Active':'Inactive') : row.role)??(row.suspended?'Suspended':row.admin?'Administrator':section==='couriers'?'Available':'Active')}</td>
          <td class="row-actions">
            <a href={base+'/'+section+'/'+row.id}>Details <ArrowRight size={15}/></a>
            {@render otherActions(row)}
          </td></tr>{:else}<tr><td colspan="4">No matching records.</td></tr>{/each}
        </tbody></table></div>
      </section>
    {/if}
    {#if paged}<CursorMore cursor={pager.cursor} loading={pager.loading} error={pager.error} next={()=>pager.next()}/>{/if}
  </div></div>
{/if}

{#if editor}
  <dialog open class="dialog" aria-label={editor.title}><div class="split"><h2>{editor.title}</h2><button onclick={()=>editor=null}>Close</button></div><Fields fields={editor.fields} submit={editor.action}/></dialog><div class="backdrop"></div>
{/if}
