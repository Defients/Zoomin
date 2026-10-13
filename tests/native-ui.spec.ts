import {test,expect,chromium} from '@playwright/test';
import type {Snapshot} from '../src/types';
test('packaged Windows UI sends real commands and validates preferences',async()=>{
 const browser=await chromium.connectOverCDP('http://127.0.0.1:9222');
 const context=browser.contexts()[0];const page=context.pages().find(p=>p.url().includes('tauri.localhost'));
 if(!page){await browser.close();throw new Error('No Zoomin test WebView found');}
 await page.waitForSelector('.app-shell');
 const state=()=>page.evaluate<Snapshot>(async()=>{
  const native=window as unknown as {__TAURI_INTERNALS__:{invoke:(command:string,args?:unknown)=>Promise<Snapshot>}};
  return native.__TAURI_INTERNALS__.invoke('get_state');
 });
 const initial=(await state()).settings;
 const shot=async(name:string)=>{await page.mouse.move(1040,24);await expect(page.locator('.toast')).toHaveCount(0);await page.screenshot({path:`artifacts/${name}.png`});};
 try{
  await expect(page.locator('.engine-status')).toContainText('Native engine ready');
  if(await page.getByLabel('Dismiss guide').isVisible()){await page.getByLabel('Dismiss guide').click();await page.getByRole('button',{name:'Apply changes'}).click();}
  await page.getByRole('button',{name:/Show lens/}).click();await expect.poll(async()=>(await state()).visible).toBe(true);
  await page.getByRole('button',{name:/Hide lens/}).click();await expect.poll(async()=>(await state()).visible).toBe(false);
  await page.getByRole('button',{name:/8×/}).click();await expect.poll(async()=>(await state()).settings.zoom).toBe(8);
  await page.getByRole('button',{name:/4×/}).click();
  await shot('overview');
  await page.locator('nav').getByRole('button',{name:'Lens',exact:true}).click();
  await page.getByLabel('Width',{exact:true}).fill('400');await page.getByRole('button',{name:'Apply changes'}).click();await expect.poll(async()=>(await state()).settings.width).toBe(400);
  // Delay the response after the real native apply, then edit again in flight.
  await page.evaluate(()=>{const native=window as unknown as {__TAURI_INTERNALS__:{invoke:(command:string,args?:unknown)=>Promise<unknown>}};const original=native.__TAURI_INTERNALS__.invoke;let once=true;native.__TAURI_INTERNALS__.invoke=async(command,args)=>{const result=await original(command,args);if(command==='apply_settings'&&once){once=false;await new Promise(resolve=>setTimeout(resolve,900));native.__TAURI_INTERNALS__.invoke=original;}return result;};});
  await page.getByLabel('Width',{exact:true}).fill('410');await page.getByRole('button',{name:'Apply changes'}).click();await page.getByLabel('Width',{exact:true}).fill('430');
  await expect(page.getByRole('button',{name:'Apply changes'})).toBeEnabled();await expect(page.getByLabel('Width',{exact:true})).toHaveValue('430');expect((await state()).settings.width).toBe(410);
  await page.getByRole('button',{name:'Apply changes'}).click();await expect.poll(async()=>(await state()).settings.width).toBe(430);
  await expect(page.locator('nav [aria-current="page"]')).toHaveText('Lens');
  await shot('lens');
  await page.locator('nav').getByRole('button',{name:'Controls',exact:true}).click();await page.getByLabel('Wheel increment').fill('0.7');await page.getByRole('button',{name:'Apply changes'}).click();await expect.poll(async()=>(await state()).settings.sensitivity).toBe(0.7);
  await expect(page.getByLabel('Shortcut for dock',{exact:true})).toHaveValue((await state()).settings.shortcuts.find(b=>b.action==='dock')!.chord);
  await page.getByLabel('Shortcut for hold',{exact:true}).focus();await page.getByLabel('Shortcut for hold',{exact:true}).press('Control+Shift+Q');await page.getByRole('button',{name:'Apply changes'}).click();await expect.poll(async()=>(await state()).settings.shortcuts.find(b=>b.action==='hold')?.chord).toBe('Ctrl+Shift+Q');
  await page.getByLabel('Preset 1',{exact:true}).fill('99');await page.getByRole('button',{name:'Apply changes'}).click();await expect(page.getByRole('alert')).toContainText('Preset must be between');expect((await state()).settings.presets[0]).toBe(initial.presets[0]);await page.getByRole('button',{name:'Discard',exact:true}).click();await page.getByLabel('Dismiss error').click();
  await shot('controls');
  await page.locator('nav').getByRole('button',{name:'Inspector',exact:true}).click();await shot('inspector');
  await page.locator('nav').getByRole('button',{name:'Streaming',exact:true}).click();await page.getByRole('button',{name:/Minimal overlay/}).click();await page.getByRole('button',{name:'Apply changes'}).click();await expect.poll(async()=>(await state()).settings.streamProfile).toBe('minimal');await shot('streaming');
  await page.locator('nav').getByRole('button',{name:'General',exact:true}).click();await shot('general');
  await page.evaluate(()=>{const native=window as unknown as {__TAURI_INTERNALS__:{invoke:(command:string,args?:unknown)=>Promise<unknown>}};const original=native.__TAURI_INTERNALS__.invoke;native.__TAURI_INTERNALS__.invoke=async(command,args)=>{const result=await original(command,args);if(command==='reset_settings'){await new Promise(resolve=>setTimeout(resolve,900));native.__TAURI_INTERNALS__.invoke=original;}return result;};});
  await page.getByRole('button',{name:'Reset',exact:true}).click();await page.locator('nav').getByRole('button',{name:'Lens',exact:true}).click();await page.getByLabel('Height',{exact:true}).fill('280');
  await expect(page.getByRole('button',{name:'Apply changes'})).toBeEnabled();await expect(page.getByLabel('Height',{exact:true})).toHaveValue('280');await expect(page.getByLabel('Width',{exact:true})).toHaveValue('360');expect((await state()).settings.height).toBe(240);
  await page.getByRole('button',{name:'Apply changes'}).click();await expect.poll(async()=>(await state()).settings.height).toBe(280);
  const overflow=await page.evaluate(()=>({body:document.body.scrollWidth>innerWidth,cards:[...document.querySelectorAll('.card')].some(c=>c.scrollWidth>c.clientWidth)}));expect(overflow).toEqual({body:false,cards:false});
 }finally{
  await page.evaluate(async settings=>{const native=window as unknown as {__TAURI_INTERNALS__:{invoke:(command:string,args?:unknown)=>Promise<unknown>}};await native.__TAURI_INTERNALS__.invoke('perform_action',{action:'hide'});await native.__TAURI_INTERNALS__.invoke('apply_settings',{settings});},initial);
  await browser.close();
 }
});

