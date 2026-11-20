///<reference types="svelte" />
;
import { upload, message, type Row } from './api';
;
  export type Field = {name:string; label:string; type?:string; value?:string|number|boolean; required?:boolean; options?: {value:string|number; text:string}[]};;
;type $$ComponentProps =  {fields:Field[];button?:string;submit:(data:Row)=>Promise<void>;notice?:string;privateMedia?:boolean};function $$render() {

  

  let {fields,button='Save',submit,notice='',privateMedia=false}:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
  let pending=$state(false),error=$state('');
  async function send(event: SubmitEvent) {
    event.preventDefault(); if(pending)return;
    pending=true;error='';
    try {
      const form=new FormData(event.currentTarget as HTMLFormElement);
      const data:Row={};
      for(const f of fields) {
        const value=form.get(f.name);
        if(f.type==='image'||f.type==='media'){
          const files=form.getAll(f.name).filter((file):file is File=>file instanceof File && file.size>0);
          const urls=await Promise.all(files.map(file=>upload(file,privateMedia)));
          data[f.name]=f.type==='image' ? urls[0] ?? f.value ?? '' : urls;
        } else if(f.type==='checkbox') data[f.name]=value==='on';
        else if(f.type==='number') data[f.name]=value ? Number(value) : null;
        else data[f.name]=value ?? '';
      }
      await submit(data);
    } catch(e){error=message(e);} finally{pending=false;}
  }
;
async () => {
 { svelteHTML.createElement("form", {   "class":`fields`,"onsubmit":send,});
     for(let field of __sveltets_2_ensureArray(fields)){field.name;
     { svelteHTML.createElement("label", { });field.type==='checkbox';
      if(field.type==='checkbox'){ { svelteHTML.createElement("input", {     "name":field.name,"type":`checkbox`,"checked":Boolean(field.value),});} field.label;
      }else{
         { svelteHTML.createElement("span", {});field.label; }
        if(field.options){ { svelteHTML.createElement("select", {     "name":field.name,"value":String(field.value ?? field.options[0]?.value ?? ''),"required":field.required ?? true,});   for(let option of __sveltets_2_ensureArray(field.options)){option.value; { svelteHTML.createElement("option", { "value":option.value,});option.text; }} }
        } else if (field.type==='textarea'){ { svelteHTML.createElement("textarea", {       "name":field.name,"value":String(field.value ?? ''),"rows":4,"required":field.required ?? true,}); }
        } else if (field.type==='image'||field.type==='media'){ { svelteHTML.createElement("input", {         "name":field.name,"type":`file`,"accept":field.type==='image'?'image/*':'image/*,video/mp4,video/webm',"multiple":field.type==='media',"required":field.required ?? false,});}
        }else{ { svelteHTML.createElement("input", {           "name":field.name,"type":field.type ?? 'text',"value":String(field.value ?? ''),"required":field.required ?? true,"min":field.type==='number'?0:undefined,"minlength":field.type==='password'?10:undefined,});}
        }
      }
     }
  }
  if(error){ { svelteHTML.createElement("p", {   "class":`error`,"role":`alert`,});error; }}
  if(notice){ { svelteHTML.createElement("p", {   "class":`feedback`,"role":`status`,});notice; }}
   { svelteHTML.createElement("button", {   "class":`primary`,"disabled":pending,});pending ? 'Saving…' : button; }
 }
};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Fields__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Fields__SvelteComponent_ = ReturnType<typeof Fields__SvelteComponent_>;
/*Ωignore_endΩ*/export default Fields__SvelteComponent_;