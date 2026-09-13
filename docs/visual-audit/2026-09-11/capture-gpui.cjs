const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const {chromium} = require('playwright');
// Usage: NODE_PATH=<playwright modules> node capture-gpui.cjs <dist> <font server URL> <output dir> <chromium executable>
const [dist, fontServer, output, executablePath] = process.argv.slice(2);
if (!dist || !fontServer || !output || !executablePath) throw new Error('Expected dist, font server URL, output dir, and Chromium executable');
const root=path.resolve(dist);
fs.mkdirSync(output,{recursive:true});
(async()=>{
 const server=http.createServer((req,res)=>{
  const name=decodeURIComponent(new URL(req.url,'http://localhost').pathname).replace(/^\/gpui\//,'/');
  const file=path.join(root,name==='/'?'index.html':name);
  if(!file.startsWith(root+'/')||!fs.existsSync(file)){res.writeHead(404).end();return;}
  res.writeHead(200,{'Content-Type':file.endsWith('.wasm')?'application/wasm':file.endsWith('.js')?'text/javascript':'text/html','Cross-Origin-Opener-Policy':'same-origin','Cross-Origin-Embedder-Policy':'require-corp'});
  fs.createReadStream(file).pipe(res);
 });
 await new Promise(r=>server.listen(18321,'127.0.0.1',r));
 const browser=await chromium.launch({headless:true,executablePath,args:['--enable-unsafe-webgpu','--use-angle=swiftshader']});
 const page=await browser.newPage({viewport:{width:1280,height:720},deviceScaleFactor:1});
 page.on('console',m=>console.log('console:',m.type(),m.text().slice(0,400)));
 page.on('pageerror',e=>console.log('pageerror:',e.message));
 let gets=0;page.on('response',r=>{if(r.url().includes('/api/file/'))gets++;});
 await page.goto('http://127.0.0.1:18321/gpui/?server='+encodeURIComponent(fontServer)+'&glyph=R');
 await page.waitForTimeout(20000);
 console.log('font files fetched:',gets);
 await page.mouse.move(1279,719);
 await page.screenshot({path:path.join(output,'gpui-overview-gray.png')});


 await page.mouse.click(75,59);
 await page.keyboard.type('0052',{delay:100});
 await page.waitForTimeout(1000);
 await page.screenshot({path:path.join(output,'gpui-search-gray.png')});
 await page.mouse.dblclick(640,365,{delay:150});
 await page.mouse.click(65,103);
 for(let i=0;i<8;i++) await page.keyboard.press('Backspace');
 for(let i=0;i<8;i++) await page.keyboard.press('Delete');
 await page.mouse.click(730,20);
 await page.mouse.move(1279,719);
 await page.waitForTimeout(1000);
 await page.screenshot({path:path.join(output,'gpui-editor-gray-1280.png')});
 await page.waitForTimeout(500);
 await page.screenshot({path:path.join(output,'gpui-editor-gray-1280-repeat.png')});
 await page.mouse.click(1100,218);
 await page.mouse.move(1279,719);
 await page.waitForTimeout(500);
 await page.screenshot({path:path.join(output,'gpui-editor-gray-folded.png')});
 console.log(await page.locator('body').innerText());
 await browser.close();await new Promise(r=>server.close(r));
})();
