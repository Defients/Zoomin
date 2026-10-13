import {chromium} from 'playwright';
const browser=await chromium.connectOverCDP('http://127.0.0.1:9222');
const page=browser.contexts()[0].pages().find(p=>p.url().includes('tauri.localhost'));
if(!page)throw new Error('No Zoomin test WebView found');
try{await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('exit_app'));}catch(error){if(!String(error).match(/closed|disconnected/))throw error;}
try{await browser.close();}catch{/* app closure also disconnects the test transport */}
