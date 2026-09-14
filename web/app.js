import init, { BrowserEditor } from './pkg/runebender_browser.js';
const canvas = document.querySelector('#app');
const context = canvas.getContext('2d', {alpha:false});
const error = document.querySelector('#error');
let editor, pending=false, lastDown={time:0,x:0,y:0,count:0};
const mods = e => (e.shiftKey?1:0)|(e.ctrlKey?2:0)|(e.altKey?4:0)|(e.metaKey?8:0);
function fail(e) { document.querySelector('#loading').hidden=false; document.querySelector('#status').textContent='Runebender could not start'; error.textContent=String(e); console.error(e); }
function draw() {
  pending=false;
  try { const bytes=editor.frame(); context.putImageData(new ImageData(new Uint8ClampedArray(bytes),canvas.width,canvas.height),0,0); }
  catch(e){fail(e);}
}
function schedule(){if(!pending){pending=true;requestAnimationFrame(draw);}}
function resize(){
  const rect=canvas.getBoundingClientRect();
  canvas.width=Math.max(1,Math.round(rect.width));canvas.height=Math.max(1,Math.round(rect.height));
  editor?.resize(canvas.width,canvas.height);if(editor)schedule();
}
function point(e){const r=canvas.getBoundingClientRect();return [(e.clientX-r.left)*canvas.width/r.width,(e.clientY-r.top)*canvas.height/r.height];}
canvas.addEventListener('pointerdown',e=>{
  e.preventDefault();canvas.focus();canvas.setPointerCapture(e.pointerId);
  const [x,y]=point(e);const count=e.timeStamp-lastDown.time<450&&Math.hypot(x-lastDown.x,y-lastDown.y)<6 ? lastDown.count%3+1:1;
  lastDown={time:e.timeStamp,x,y,count};
  editor.pointer(1,x,y,e.button,e.buttons,count,mods(e),0,0);schedule();
});
canvas.addEventListener('pointerup',e=>{e.preventDefault();const [x,y]=point(e);editor.pointer(2,x,y,e.button,e.buttons,lastDown.count,mods(e),0,0);schedule();});
canvas.addEventListener('pointermove',e=>{const [x,y]=point(e);editor.pointer(0,x,y,e.button,e.buttons,0,mods(e),0,0);schedule();});
canvas.addEventListener('pointercancel',e=>{const [x,y]=point(e);editor.pointer(2,x,y,0,0,1,mods(e),0,0);schedule();});
canvas.addEventListener('contextmenu',e=>e.preventDefault());
canvas.addEventListener('wheel',e=>{e.preventDefault();const[x,y]=point(e);const unit=e.deltaMode===1?20:e.deltaMode===2?canvas.height:1;editor.pointer(3,x,y,0,e.buttons,0,mods(e),-e.deltaX*unit,-e.deltaY*unit);schedule();},{passive:false});
for(const name of ['keydown','keyup'])canvas.addEventListener(name,e=>{
  if(e.key==='F5'||((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='r'))return;
  e.preventDefault();editor.key(name==='keydown',e.key,e.code,mods(e),e.repeat);schedule();
});
window.addEventListener('blur',()=>{if(editor){editor.pointer(2,-1,-1,0,0,1,0,0,0);schedule();}});
new ResizeObserver(resize).observe(canvas);
document.querySelector('#reset').addEventListener('click',()=>{
  if(confirm('Discard this tab’s edits and reload the sample font?'))location.reload();
});
try{
  await init();resize();editor=new BrowserEditor(canvas.width,canvas.height);
  // Read-only inspection for integration checks, never a second editing implementation.
  window.runebender={state:()=>JSON.parse(editor.state())};
  draw();document.querySelector('#loading').hidden=true;canvas.focus();
}catch(e){fail(e);}
