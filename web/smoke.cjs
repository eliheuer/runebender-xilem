const assert=require('node:assert/strict');
const{chromium}=require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');
(async()=>{
const browser=await chromium.launch({headless:true,executablePath:process.env.RUNEBENDER_CHROME});
const page=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4324/');
const frame = process.env.RUNEBENDER_IFRAME ? page.frames().find(f=>f.url().includes('/app/index.html')) : page;
assert.ok(frame, 'editor frame exists');
await frame.waitForFunction(()=>window.runebender);
const state=()=>frame.evaluate(()=>window.runebender.state());
await page.mouse.dblclick(410,150,{delay:100});assert.equal((await state()).mode,'editor');
const before=await state();
await page.mouse.move(624,168);await page.mouse.down();await page.mouse.move(646,147,{steps:6});await page.mouse.up();
const edited=await state();console.log('DRAG',edited.modified,edited.selected_points,edited.points.slice(12,18));assert.equal(edited.modified,true);assert.notDeepEqual(edited.points,before.points);
await page.keyboard.press('Control+z');assert.deepEqual((await state()).points,before.points);console.log('UNDO restored outline');
await page.keyboard.press('Control+Shift+z');assert.deepEqual((await state()).points,edited.points);console.log('REDO restored edit');
await page.mouse.move(720,320);await page.mouse.wheel(0,-100);await page.waitForTimeout(200);console.log('WHEEL zoom',(await state()).zoom);
await page.keyboard.press('Control+s');assert.deepEqual((await state()).points,edited.points);
await page.keyboard.press('Control+o');assert.match((await state()).note,/desktop/i);
if(process.env.RUNEBENDER_SCREENSHOT)await page.screenshot({path:process.env.RUNEBENDER_SCREENSHOT});
await page.mouse.move(245,350);await page.mouse.down();await page.mouse.move(295,350,{steps:5});await page.mouse.up();await page.waitForTimeout(100);

await page.mouse.click(1150,43);await page.waitForTimeout(100);console.log('NODES',await state());
assert.equal((await state()).mode,'nodes');
assert.deepEqual(errors,[]);await browser.close();
})().catch(error=>{console.error(error);process.exit(1);});
