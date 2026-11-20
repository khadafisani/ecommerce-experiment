///<reference types="svelte" />
;
import { onMount } from 'svelte';
import { ArrowLeft, ArrowRight, Boxes, ClipboardList, LayoutDashboard, Plus, Store, Users, Wallet } from '@lucide/svelte';
import { api, date, downloadReturnMedia, key, label, message, money, params, type Row } from './api';
import { CursorPager } from './cursor-pager.svelte';
import CursorMore from './CursorMore.svelte';
import Chart from './Chart.svelte';
import ProductInitials from './ProductInitials.svelte';
import Fields, {type Field} from './Fields.svelte';

;type $$ComponentProps =  {portal:string;parts:string[];account:Row};function $$render() {
 const productRows/*Ωignore_positionΩ*/ = (list:Row[])/*Ωignore_startΩ*/: ReturnType<import('svelte').Snippet>/*Ωignore_endΩ*/ => { async ()/*Ωignore_positionΩ*/ => {
   { svelteHTML.createElement("div", { "class":`table-scroll`,}); { svelteHTML.createElement("table", {}); { svelteHTML.createElement("thead", {}); { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});admin?'Shop':'Lowest price'; } { svelteHTML.createElement("th", {});admin?'Taxonomy':'Sold / Stock'; } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});admin?'Created':'Last updated by'; } { svelteHTML.createElement("th", {});  } } } { svelteHTML.createElement("tbody", {});
     for(let row of __sveltets_2_ensureArray(list)){row.id; { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", {}); { svelteHTML.createElement("div", { "class":`table-item`,});if(row.image){ { svelteHTML.createElement("img", {  "src":row.image,"alt":"",});}}else{  { const $$_slaitinItcudorP6C = __sveltets_2_ensureComponent(ProductInitials); new $$_slaitinItcudorP6C({ target: __sveltets_2_any(), props: { "name":row.name,}});}} { svelteHTML.createElement("div", {}); { svelteHTML.createElement("strong", {});row.name; }if(admin){ { svelteHTML.createElement("small", {});row.description; }}if(row.note){ { svelteHTML.createElement("small", {});row.note; }} } } } { svelteHTML.createElement("td", {});admin?row.shop:money(row.lowest_price); } { svelteHTML.createElement("td", {});admin?[row.brand,row.category].filter(Boolean).join(' / ')||'—':row.sold+' / '+row.stock+(Number(row.stock)===0?' · Out of stock':''); } { svelteHTML.createElement("td", {}); { svelteHTML.createElement("span", { "class":`badge`,});label(row.status); } } { svelteHTML.createElement("td", {});admin?date(row.created_at):row.actor; } { svelteHTML.createElement("td", { "class":`row-actions`,});
    if(admin){
      if(row.status==='pending'){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>moderate(row,'active'),});  } { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>moderate(row,'rejected'),});  }}
      if(row.status==='active'){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>moderate(row,'inactive'),});  }}
    }else{ { svelteHTML.createElement("a", { "href":base+'/products/'+row.id,});   { const $$_thgiRworrA6C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA6C({ target: __sveltets_2_any(), props: { "size":15,}});} }}
   } }} { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", { "colspan":6,});   } }
   } } }
};return __sveltets_2_any(0)}; const orderRows/*Ωignore_positionΩ*/ = (list:Row[])/*Ωignore_startΩ*/: ReturnType<import('svelte').Snippet>/*Ωignore_endΩ*/ => { async ()/*Ωignore_positionΩ*/ => {
   { svelteHTML.createElement("div", { "class":`table-scroll`,}); { svelteHTML.createElement("table", {}); { svelteHTML.createElement("thead", {}); { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});   } { svelteHTML.createElement("th", {}); } } } { svelteHTML.createElement("tbody", {});
       for(let row of __sveltets_2_ensureArray(list)){row.id; { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", {}); row.id; } { svelteHTML.createElement("td", {});date(row.created_at); } { svelteHTML.createElement("td", {});money(row.subtotal+row.shipping+row.tax); } { svelteHTML.createElement("td", {}); { svelteHTML.createElement("span", { "class":`badge`,});label(row.status); } } { svelteHTML.createElement("td", {});row.actor; } { svelteHTML.createElement("td", {}); { svelteHTML.createElement("a", { "href":base+'/orders/'+row.id,});   { const $$_thgiRworrA6C = __sveltets_2_ensureComponent(ArrowRight); new $$_thgiRworrA6C({ target: __sveltets_2_any(), props: { "size":15,}});} } } }
    } { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", { "colspan":6,});   } }
   } } }
};return __sveltets_2_any(0)}; const taxonomyRows/*Ωignore_positionΩ*/ = (list:Row[])/*Ωignore_startΩ*/: ReturnType<import('svelte').Snippet>/*Ωignore_endΩ*/ => { async ()/*Ωignore_positionΩ*/ => {
   { svelteHTML.createElement("div", { "class":`table-scroll`,}); { svelteHTML.createElement("table", {}); { svelteHTML.createElement("thead", {}); { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } } } { svelteHTML.createElement("tbody", {});
       for(let row of __sveltets_2_ensureArray(list)){row.id; { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", {});row.name; } { svelteHTML.createElement("td", {});label(row.kind); } { svelteHTML.createElement("td", {}); { svelteHTML.createElement("span", { "class":`badge`,});label(row.status); } } { svelteHTML.createElement("td", {});date(row.created_at); } { svelteHTML.createElement("td", { "class":`row-actions`,});
      if(row.status!=='approved'){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>perform('/admin/taxonomy/'+row.id,'PUT',{status:'approved'}),});  }}
      if(row.status==='pending'){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>perform('/admin/taxonomy/'+row.id,'PUT',{status:'rejected'}),});  } { svelteHTML.createElement("button", { "onclick":()=>editTaxonomy(row),});  }}
     } }} { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", { "colspan":5,});   } }
   } } }
};return __sveltets_2_any(0)}; const filter/*Ωignore_positionΩ*/ = (searchable:boolean,sortOptions=false)/*Ωignore_startΩ*/: ReturnType<import('svelte').Snippet>/*Ωignore_endΩ*/ => { async ()/*Ωignore_positionΩ*/ => {
   { svelteHTML.createElement("form", {   "class":`filter`,"onsubmit":(e)=>{e.preventDefault();load();},});
    if(searchable){ { svelteHTML.createElement("input", {     "aria-label":`Search records`,"placeholder":`Search records`,"bind:value":q,});/*Ωignore_startΩ*/() => q = __sveltets_2_any(null);/*Ωignore_endΩ*/}}
    if(statusesForSection.length){ { svelteHTML.createElement("select", {     "aria-label":`Filter by status`,"bind:value":status,"onchange":()=>load(),});/*Ωignore_startΩ*/() => status = __sveltets_2_any(null);/*Ωignore_endΩ*/ { svelteHTML.createElement("option", {"value":"",});  }   for(let value of __sveltets_2_ensureArray(statusesForSection)){value; { svelteHTML.createElement("option", { "value":value,});label(value); }} }}
    if(sortOptions){ { svelteHTML.createElement("select", {     "aria-label":`Sort by date`,"bind:value":sort,"onchange":()=>load(),});/*Ωignore_startΩ*/() => sort = __sveltets_2_any(null);/*Ωignore_endΩ*/ { svelteHTML.createElement("option", {"value":"",});section==='dashboard'&&admin?'Oldest first':'Newest first'; } { svelteHTML.createElement("option", { "value":section==='dashboard'&&admin?'newest':'oldest',});section==='dashboard'&&admin?'Newest first':'Oldest first'; } }}
    if(section==='reviews'){ { svelteHTML.createElement("input", {       "aria-label":`Shop ID`,"placeholder":`Shop ID`,"inputmode":`numeric`,"bind:value":shopFilter,});/*Ωignore_startΩ*/() => shopFilter = __sveltets_2_any(null);/*Ωignore_endΩ*/} { svelteHTML.createElement("input", {       "type":`email`,"aria-label":`Reviewer email`,"placeholder":`Reviewer email`,"bind:value":userFilter,});/*Ωignore_startΩ*/() => userFilter = __sveltets_2_any(null);/*Ωignore_endΩ*/}}
    if(searchable){ { svelteHTML.createElement("button", {});  }}
   }
};return __sveltets_2_any(0)}; const periods/*Ωignore_positionΩ*/ = (values:string[])/*Ωignore_startΩ*/: ReturnType<import('svelte').Snippet>/*Ωignore_endΩ*/ => { async ()/*Ωignore_positionΩ*/ => {
   { svelteHTML.createElement("label", { "class":`period`,});  { svelteHTML.createElement("select", {   "bind:value":duration,"onchange":()=>load(),});/*Ωignore_startΩ*/() => duration = __sveltets_2_any(null);/*Ωignore_endΩ*/   for(let value of __sveltets_2_ensureArray(values)){value; { svelteHTML.createElement("option", { "value":value,});({day:'Daily',week:'Weekly',month:'Monthly',year:'Yearly'} as Record<string,string>)[value]; }} } }
};return __sveltets_2_any(0)};
  
  
  
  
  
  
  
  
  let {portal,parts,account}:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/=$props();
  let admin=$derived(portal==='admin');
  let activeShop=$derived(account.shops.find((s:Row)=>s.id===Number(parts[1])) ?? (!parts[1]?account.shops[0]:null));
  let section=$derived((admin?parts[1]:parts[2])??'dashboard');
  let record=$derived(admin?parts[2]:parts[3]);
  let base=$derived(admin?'/admin':'/merchant/'+activeShop?.id);
  let error=$state(''), feedback=$state(''), pending=$state(false), loading=$state(true);
  let data=$state<Row>({}), rows=$state<Row[]>([]), couriers=$state<Row[]>([]), terms=$state<Row[]>([]);
  let editor=$state<{ title:string;fields:Field[];action:(data:Row)=>Promise<void> }|null>(null);
  let status=$state(''), sort=$state(''), duration=$state('day'), q=$state(''), shopFilter=$state(''), userFilter=$state('');
  let lineQuery=$state('');
  const pager = new CursorPager<Row & {id:number}>();
  let paged = $derived(!admin && !record && (section==='products'||section==='orders'));
  const adminSections=['dashboard','products','taxonomy','reviews','users','shops','couriers','payments'];
  const shopSections=['dashboard','products','orders','team'];
  const navigation:Record<string,string>={dashboard:'Overview',products:'Products',taxonomy:'Brands & categories',reviews:'Reviews',users:'Users',shops:'Shops',couriers:'Couriers',payments:'Manual payments',orders:'Orders',team:'Team'};
  const navIcons:Record<string,typeof Store>={dashboard:LayoutDashboard,products:Boxes,orders:ClipboardList,users:Users,team:Users,payments:Wallet};
  const statuses=['waiting_payment','waiting_shop','packing','delivery','completed','rejected','return_review','return_accepted','return_rejected','return_completed','cancelled'];
  const productStatuses=['pending','active','inactive','rejected'];
  let statusesForSection=$derived(section==='products'&&!record?productStatuses:section==='taxonomy'?['pending','approved','rejected']:section==='payments'?['waiting','paid','cancelled']:!admin&&(section==='dashboard'||section==='orders')?statuses:[]);
  const shopKey=key();
  onMount(()=>{if(!admin&&section==='products'&&record&&record!=='new')duration='week';void load();});
  async function load() {
    if(!admin&&!activeShop||admin&&!account.user.admin){loading=false;return;}
    if(paged) {
      const route=base+'/'+section;
      const filters={q,status};
      loading=false;error='';
      await pager.reset(cursor=>api(route+params({...filters,cursor})));
      return;
    }
    loading=true;error='';
    try{
      if(section==='dashboard') data=await api(base+'/dashboard'+params(admin?{status:duration,q,sort}:{status,sort:duration}));
      else if(!admin&&record&&record!=='new') {
        data=await api(base+'/'+section+'/'+record+params({status:section==='products'?duration:null,sort}));
        if(section==='orders') couriers=await api<Row[]>('/couriers');
      } else if(!admin&&record==='new') terms=await api<Row[]>('/categories');
      else {
        rows=await api<Row[]>(base+'/'+section+params({q,status,sort,shop:shopFilter,category:userFilter}));
      }
    }catch(e){error=message(e);}finally{loading=false;}
  }
  async function perform(path:string,method='POST',body?:Row) {
    if(pending)return;pending=true;error='';feedback='';
    try{await api(path,method,body);feedback='Saved.';await load();}catch(e){error=message(e);}finally{pending=false;}
  }
  function form(title:string,fields:Field[],submit:(data:Row)=>Promise<unknown>,success?:string) {
    editor={title,fields,action:async(data)=>{await submit(data);editor=null;if(success)location.assign(success);else {feedback='Saved.';await load();}}};
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
;
async () => {







if(admin&&!account.user.admin){
   { svelteHTML.createElement("section", { "class":`panel empty`,}); { svelteHTML.createElement("h1", {});   } { svelteHTML.createElement("a", { "href":`/admin/login`,});    } }
} else if (!admin&&!activeShop){
   { svelteHTML.createElement("section", { "class":`panel auth`,}); { svelteHTML.createElement("h1", {});parts[1]?'Shop unavailable':'Open your shop'; }
  if(!parts[1]){  { const $$_sdleiF1C = __sveltets_2_ensureComponent(Fields); new $$_sdleiF1C({ target: __sveltets_2_any(), props: {     "fields":[{name:'name',label:'Shop name'}],"button":`Open shop`,"submit":async(data)=>{const result=await api('/shops','POST',{...data,key:shopKey});location.assign('/merchant/'+result.id);},}});}}else{ { svelteHTML.createElement("a", { "href":`/merchant`,});    }} }
}else{
   { svelteHTML.createElement("div", { "class":`work-layout`,});
   { svelteHTML.createElement("aside", { "class":`work-nav`,});
     { svelteHTML.createElement("h2", {});admin?'Admin':activeShop?.name; }
    if(!admin && account.shops.length>1){ { svelteHTML.createElement("select", {     "aria-label":`Switch shop`,"value":activeShop?.id,"onchange":(e)=>location.assign('/merchant/'+e.currentTarget.value),});   for(let shop of __sveltets_2_ensureArray(account.shops)){shop.id; { svelteHTML.createElement("option", { "value":shop.id,});shop.name; }} }}
     { svelteHTML.createElement("nav", { "aria-label":`Workspace navigation`,});   for(let entry of __sveltets_2_ensureArray(admin?adminSections:shopSections)){entry;
      if(entry!=='team'||activeShop?.owner){
        const Icon=navIcons[entry]??Store;
         { svelteHTML.createElement("a", {   "href":entry==='dashboard'?base:base+'/'+entry,});section===entry;  { const $$_nocI4C = __sveltets_2_ensureComponent(Icon); new $$_nocI4C({ target: __sveltets_2_any(), props: { "size":17,}});}navigation[entry]; }
      }
    } }
     { svelteHTML.createElement("a", {   "class":`breadcrumb`,"href":`/`,});  { const $$_tfeLworrA3C = __sveltets_2_ensureComponent(ArrowLeft); new $$_tfeLworrA3C({ target: __sveltets_2_any(), props: { "size":15,}});}  }
   }
   { svelteHTML.createElement("div", { "class":`workspace`,});
     { svelteHTML.createElement("div", { "class":`section-head`,}); { svelteHTML.createElement("h1", {});record==='new'?'New product':record?(data.name??(section==='orders'?'Order #'+record:navigation[section])):navigation[section]; }
    if(!admin&&section==='products'&&!record){ { svelteHTML.createElement("a", {   "class":`primary`,"href":base+'/products/new',});  { const $$_sulP4C = __sveltets_2_ensureComponent(Plus); new $$_sulP4C({ target: __sveltets_2_any(), props: { "size":17,}});}   }}
    if(admin&&section==='users'){ { svelteHTML.createElement("button", {   "class":`primary`,"onclick":()=>form('Create administrator',accountFields,data=>api('/admin/accounts','POST',data)),});  { const $$_sulP4C = __sveltets_2_ensureComponent(Plus); new $$_sulP4C({ target: __sveltets_2_any(), props: { "size":17,}});}   }}
    if(!admin&&section==='team'&&activeShop?.owner){ { svelteHTML.createElement("button", {   "class":`primary`,"onclick":()=>form('Create employee account',accountFields,data=>api(base+'/team','POST',data)),});  { const $$_sulP4C = __sveltets_2_ensureComponent(Plus); new $$_sulP4C({ target: __sveltets_2_any(), props: { "size":17,}});}   }}
    if(admin&&section==='couriers'){ { svelteHTML.createElement("button", {   "class":`primary`,"onclick":()=>courierEditor(),});  { const $$_sulP4C = __sveltets_2_ensureComponent(Plus); new $$_sulP4C({ target: __sveltets_2_any(), props: { "size":17,}});}   }}
    if(admin&&section==='taxonomy'){ { svelteHTML.createElement("button", {   "class":`primary`,"onclick":()=>form('Add approved term',[name(),{name:'kind',label:'Type',options:[{value:'brand',text:'Brand'},{value:'category',text:'Category'}]}],data=>api('/admin/taxonomy','POST',data)),});  { const $$_sulP4C = __sveltets_2_ensureComponent(Plus); new $$_sulP4C({ target: __sveltets_2_any(), props: { "size":17,}});}   }}
     }
    if(error){ { svelteHTML.createElement("p", {   "class":`error`,"role":`alert`,});error; }}
    if(feedback){ { svelteHTML.createElement("p", {   "class":`feedback`,"role":`status`,});feedback; }}
    if(paged&&pager.cursor===null&&pager.error){ { svelteHTML.createElement("p", {   "class":`error`,"role":`alert`,});pager.error;  { svelteHTML.createElement("button", { "onclick":()=>load(),});  } }}
    if((loading&&!Object.keys(data).length&&!rows.length) || (paged&&pager.loading&&!pager.items.length)){ { svelteHTML.createElement("p", { "class":`inline-empty`,});  }}

    if(section==='dashboard'){
      ;__sveltets_2_ensureSnippet(periods(admin?['day','week','month','year']:['day','month','year']));
       { svelteHTML.createElement("div", { "class":`detail-grid chart-grid`,}); { const $$_trahC3C = __sveltets_2_ensureComponent(Chart); new $$_trahC3C({ target: __sveltets_2_any(), props: {    "title":`Sales`,"points":data.sales??[],"currency":true,}});}if(admin){  { const $$_trahC3C = __sveltets_2_ensureComponent(Chart); new $$_trahC3C({ target: __sveltets_2_any(), props: {   "title":`Unique signed-in users`,"points":data.users??[],}});}}else{ { const $$_trahC3C = __sveltets_2_ensureComponent(Chart); new $$_trahC3C({ target: __sveltets_2_any(), props: {    "title":`Product stock health`,"points":data.stock??[],"pie":true,}});}} }
       { svelteHTML.createElement("section", { "class":`panel section`,}); { svelteHTML.createElement("h2", {});admin?'Moderation queue':'Recent orders'; };__sveltets_2_ensureSnippet(filter(admin,admin));if(admin){ { svelteHTML.createElement("h3", {});   };__sveltets_2_ensureSnippet(productRows(data.products??[])); { svelteHTML.createElement("h3", { "class":`section`,});     };__sveltets_2_ensureSnippet(taxonomyRows(data.taxonomy??[]));}else{;__sveltets_2_ensureSnippet(orderRows(data.orders??[]));} }
    } else if (section==='products'&&record==='new'&&!admin){
       { svelteHTML.createElement("section", { "class":`panel`,}); { svelteHTML.createElement("h2", {});  }
        if(terms.length){ { svelteHTML.createElement("details", {}); { svelteHTML.createElement("summary", {});    } { svelteHTML.createElement("p", {});terms.map(term=>term.name).join(' · '); } }}
          { const $$_sdleiF3C = __sveltets_2_ensureComponent(Fields); new $$_sdleiF3C({ target: __sveltets_2_any(), props: {     "fields":[...productFields(),...variantFields().map(field=>({...field,name:'v_'+field.name,label:field.name==='name'?'First variant name':field.label}))],"button":`Submit for review`,"submit":createProduct,}});}
       }
    } else if (section==='products'&&record&&!admin&&data.id){
       { svelteHTML.createElement("section", { "class":`panel`,}); { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("span", { "class":`badge`,});label(data.status); } { svelteHTML.createElement("button", { "onclick":()=>form('Edit and resubmit product',productFields(data),input=>api(base+'/products/'+record,'PUT',input)),});  } }
        if(data.image){ { svelteHTML.createElement("img", {     "class":`record-image`,"src":data.image,"alt":data.name,});}}else{ { svelteHTML.createElement("div", { "class":`record-image record-placeholder`,});  { const $$_slaitinItcudorP4C = __sveltets_2_ensureComponent(ProductInitials); new $$_slaitinItcudorP4C({ target: __sveltets_2_any(), props: { "name":data.name,}});} }} { svelteHTML.createElement("p", {});data.description; }if(data.note){ { svelteHTML.createElement("p", { "class":`feedback`,});data.note; }}
         { svelteHTML.createElement("p", { "class":`muted`,});data.brand??'—';  data.category??'—';     data.actor; } { svelteHTML.createElement("p", {});  { svelteHTML.createElement("strong", {});data.counts.reduce((total:number,count:Row)=>total+Number(count.value),0); }    { svelteHTML.createElement("strong", {});data.sold; }    { svelteHTML.createElement("strong", {});data.stock; } } { svelteHTML.createElement("div", { "class":`tags`,});   for(let count of __sveltets_2_ensureArray(data.counts)){count.status; { svelteHTML.createElement("span", {});label(count.status);  count.value; }} }
       }
       { svelteHTML.createElement("section", { "class":`panel section`,}); { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("h2", {});  } { svelteHTML.createElement("button", { "onclick":newVariant,});  { const $$_sulP5C = __sveltets_2_ensureComponent(Plus); new $$_sulP5C({ target: __sveltets_2_any(), props: { "size":16,}});}   } }
           for(let v of __sveltets_2_ensureArray(data.variants)){v.id; { svelteHTML.createElement("div", { "class":`split review`,}); { svelteHTML.createElement("span", {});v.name;v.subvariant?' / '+v.subvariant:'';  money(v.price);  v.stock;   v.enabled?'':' · Disabled'; } { svelteHTML.createElement("button", { "onclick":()=>form('Edit variant',variantFields(v),input=>api(base+'/products/'+record+'/variants/'+v.id,'PUT',input)),});  } }}
       }
       { svelteHTML.createElement("div", { "class":`section`,});;__sveltets_2_ensureSnippet(periods(['week','month','year']));  { const $$_trahC3C = __sveltets_2_ensureComponent(Chart); new $$_trahC3C({ target: __sveltets_2_any(), props: {   "title":`Units sold`,"points":data.series,}});} }
       { svelteHTML.createElement("section", { "class":`panel section`,}); { svelteHTML.createElement("h2", {});  };__sveltets_2_ensureSnippet(filter(false,true));;__sveltets_2_ensureSnippet(orderRows(data.orders)); }
       { svelteHTML.createElement("section", { "class":`panel section`,}); { svelteHTML.createElement("h2", {});  }   for(let review of __sveltets_2_ensureArray(data.reviews)){review.id; { svelteHTML.createElement("article", { "class":`review`,}); { svelteHTML.createElement("strong", {});review.user;  review.rating;  } { svelteHTML.createElement("p", {});review.body; }if(review.reply){ { svelteHTML.createElement("blockquote", {});review.reply; }} { svelteHTML.createElement("button", { "onclick":()=>form('Reply to review',[{name:'body',label:'Reply',type:'textarea',value:review.reply}],input=>api(base+'/reviews/'+review.id,'PUT',input)),});  } }} { svelteHTML.createElement("p", {});   } }
    } else if (section==='products'){
       { svelteHTML.createElement("section", { "class":`panel`,});;__sveltets_2_ensureSnippet(filter(true,admin));;__sveltets_2_ensureSnippet(productRows(paged?pager.items:rows)); }
    } else if (section==='orders'&&record&&data.id){
       { svelteHTML.createElement("section", { "class":`panel`,});
         { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("span", { "class":`badge`,});label(data.status); } { svelteHTML.createElement("span", {});   data.actor; } } { svelteHTML.createElement("p", {});data.address; } { svelteHTML.createElement("p", {});  { svelteHTML.createElement("strong", {});money(data.subtotal+data.shipping+data.tax); }   money(data.shipping);   money(data.tax); }
         { svelteHTML.createElement("div", { "class":`actions`,});
          if(data.status==='waiting_shop'){ { svelteHTML.createElement("button", {     "class":`primary`,"disabled":pending,"onclick":()=>orderStatus('packing'),});  } { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>orderStatus('rejected'),});  }}
          if(data.status==='packing'){ { svelteHTML.createElement("button", {   "class":`primary`,"onclick":dispatch,});  }}
          if(data.status==='delivery'&&data.courier_id===1){ { svelteHTML.createElement("button", { "onclick":tracking,});   }}
          if(data.status==='return_review'){ { svelteHTML.createElement("button", {     "class":`primary`,"disabled":pending,"onclick":()=>orderStatus('return_accepted'),});  } { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>orderStatus('return_rejected'),});  }}
          if(data.status==='return_accepted'){ { svelteHTML.createElement("button", {     "class":`primary`,"disabled":pending,"onclick":()=>orderStatus('return_completed'),});  }}
         }
        if(data.courier){ { svelteHTML.createElement("p", {});data.courier;  data.receipt; }}
        if(data.return){ { svelteHTML.createElement("h3", {});  } { svelteHTML.createElement("p", {});data.return.description; }   for(let url of __sveltets_2_ensureArray(data.return.media)){url; { svelteHTML.createElement("button", { "onclick":()=>downloadReturnMedia(url).catch(e=>error=message(e)),});  }}}
           for(let event of __sveltets_2_ensureArray(data.tracking)){let i = 1;i; { svelteHTML.createElement("div", { "class":`split review`,}); { svelteHTML.createElement("span", {});event.note; } { svelteHTML.createElement("span", {});date(event.created_at); } }}
       }
       { svelteHTML.createElement("section", { "class":`panel section`,}); { svelteHTML.createElement("h2", {});  } { svelteHTML.createElement("input", {     "aria-label":`Search items`,"placeholder":`Search items`,"bind:value":lineQuery,});/*Ωignore_startΩ*/() => lineQuery = __sveltets_2_any(null);/*Ωignore_endΩ*/}
       { svelteHTML.createElement("div", { "class":`table-scroll`,}); { svelteHTML.createElement("table", {}); { svelteHTML.createElement("thead", {}); { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } } } { svelteHTML.createElement("tbody", {});
         for(let item of __sveltets_2_ensureArray(data.items.filter((item:Row)=>(item.product_name+' '+item.variant_name).toLowerCase().includes(lineQuery.toLowerCase())))){let i = 1;i; { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", {}); { svelteHTML.createElement("a", { "href":base+'/products/'+item.product_id,});item.product_name; } } { svelteHTML.createElement("td", {});item.variant_name; } { svelteHTML.createElement("td", {});item.quantity; } { svelteHTML.createElement("td", {});money(item.price); } }} { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", { "colspan":4,});   } }
       } } } }
    } else if (section==='orders'){
       { svelteHTML.createElement("section", { "class":`panel`,});;__sveltets_2_ensureSnippet(filter(false));;__sveltets_2_ensureSnippet(orderRows(pager.items)); }
    } else if (section==='taxonomy'&&admin){
       { svelteHTML.createElement("section", { "class":`panel`,});;__sveltets_2_ensureSnippet(filter(true,true));;__sveltets_2_ensureSnippet(taxonomyRows(rows)); }
    }else{
       { svelteHTML.createElement("section", { "class":`panel`,});
        if(admin){;__sveltets_2_ensureSnippet(filter(true,section==='reviews'));}
         { svelteHTML.createElement("div", { "class":`table-scroll`,}); { svelteHTML.createElement("table", {}); { svelteHTML.createElement("thead", {}); { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } { svelteHTML.createElement("th", {});  } } } { svelteHTML.createElement("tbody", {});
             for(let row of __sveltets_2_ensureArray(rows)){row.id; { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", {}); { svelteHTML.createElement("strong", {});row.name; }if(section==='couriers'&&row.logo){ { svelteHTML.createElement("img", {    "class":`table-logo`,"src":row.logo,"alt":"",});}} }
           { svelteHTML.createElement("td", {});if(section==='payments'){ { svelteHTML.createElement("span", { "class":`id-cell`,});row.checkout_no; } { svelteHTML.createElement("br", {});}money(row.total);  date(row.created_at);} else if (section==='reviews'){row.user;  row.shop;  date(row.created_at); { svelteHTML.createElement("br", {});}row.body;}else{row.email??row.owner??row.url??'—';} }
           { svelteHTML.createElement("td", {});section==='reviews'?row.rating+'/5':section==='payments'?label(row.payment):(section==='team' ? row.role+' ? '+(row.suspended?'Suspended':row.active?'Active':'Inactive') : row.role)??(row.suspended?'Suspended':row.admin?'Administrator':section==='couriers'?'Available':'Active'); }
           { svelteHTML.createElement("td", { "class":`row-actions`,});
            if(section==='team'&&row.role==='Employee'){
               { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>setEmployeeAccess(row),});row.active?'Deactivate access':'Reactivate access'; }
              if(row.can_reset_password){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>resetEmployeePassword(row),});  }}
            }
            if(section==='users'||section==='shops'){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>perform(base+'/'+section+'/'+row.id,'PUT',{suspended:!row.suspended}),});row.suspended?'Restore':'Suspend'; }}
            if(section==='users'){ { svelteHTML.createElement("button", { "onclick":()=>form('Change password',[{name:'password',label:'New password',type:'password'}],input=>api(base+'/users/'+row.id,'PUT',input)),});  }}
            if(section==='reviews'){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>{if(confirm('Hide this review from the marketplace?'))perform(base+'/reviews/'+row.id,'PUT');},});  }}
            if(section==='couriers'&&row.id!==1){ { svelteHTML.createElement("button", { "onclick":()=>courierEditor(row),});  }}
            if(section==='payments'){ { svelteHTML.createElement("a", { "href":'/orders/'+row.id,});  }if(row.payment==='waiting'){ { svelteHTML.createElement("button", {   "disabled":pending,"onclick":()=>{if(confirm('Confirm that manual payment has been received?'))perform('/checkout/'+row.id+'/pay');},});  }}}
           } }} { svelteHTML.createElement("tr", {}); { svelteHTML.createElement("td", { "colspan":4,});   } }
         } } }
       }
    }
    if(paged){  { const $$_eroMrosruC2C = __sveltets_2_ensureComponent(CursorMore); new $$_eroMrosruC2C({ target: __sveltets_2_any(), props: {       "cursor":pager.cursor,"loading":pager.loading,"error":pager.error,"next":()=>pager.next(),}});}}
   } }
}

if(editor){
   { svelteHTML.createElement("dialog", {   "open":true,"class":`dialog`,"aria-label":editor.title,}); { svelteHTML.createElement("div", { "class":`split`,}); { svelteHTML.createElement("h2", {});editor.title; } { svelteHTML.createElement("button", { "onclick":()=>editor=null,});  } }  { const $$_sdleiF1C = __sveltets_2_ensureComponent(Fields); new $$_sdleiF1C({ target: __sveltets_2_any(), props: {   "fields":editor.fields,"submit":editor.action,}});} } { svelteHTML.createElement("div", { "class":`backdrop`,}); }
}
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Workspace__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Workspace__SvelteComponent_ = ReturnType<typeof Workspace__SvelteComponent_>;
/*Ωignore_endΩ*/export default Workspace__SvelteComponent_;