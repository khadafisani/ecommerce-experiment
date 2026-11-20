<script lang="ts">
  import { upload, message, type Row } from './api';
  export type Field = {name:string; label:string; type?:string; value?:string|number|boolean; required?:boolean; options?: {value:string|number; text:string}[]};
  let {fields,button='Save',submit,notice='',privateMedia=false}: {fields:Field[];button?:string;submit:(data:Row)=>Promise<void>;notice?:string;privateMedia?:boolean} = $props();
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
</script>
<form class="fields" onsubmit={send}>
  {#each fields as field (field.name)}
    <label class:check={field.type==='checkbox'}>
      {#if field.type==='checkbox'}<input name={field.name} type="checkbox" checked={Boolean(field.value)}> {field.label}
      {:else}
        <span>{field.label}</span>
        {#if field.options}<select name={field.name} value={String(field.value ?? field.options[0]?.value ?? '')} required={field.required ?? true}>{#each field.options as option (option.value)}<option value={option.value}>{option.text}</option>{/each}</select>
        {:else if field.type==='textarea'}<textarea name={field.name} value={String(field.value ?? '')} rows="4" required={field.required ?? true}></textarea>
        {:else if field.type==='image'||field.type==='media'}<input name={field.name} type="file" accept={field.type==='image'?'image/*':'image/*,video/mp4,video/webm'} multiple={field.type==='media'} required={field.required ?? false}>
        {:else}<input name={field.name} type={field.type ?? 'text'} value={String(field.value ?? '')} required={field.required ?? true} min={field.type==='number'?0:undefined} minlength={field.type==='password'?10:undefined}>
        {/if}
      {/if}
    </label>
  {/each}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if notice}<p class="feedback" role="status">{notice}</p>{/if}
  <button class="primary" disabled={pending}>{pending ? 'Saving…' : button}</button>
</form>
