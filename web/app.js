import init, { BrowserEditor } from './pkg/runebender_browser.js';
const canvas = document.querySelector('#app');
const context = canvas.getContext('2d', { alpha: false });
const textInput = document.querySelector('#text-input');
let composing = false;
const loading = document.querySelector('#loading');
let editor, pending = false, stopped = false, scale = 1, lastFrame = 0, move;
let lastDown = { time: 0, x: 0, y: 0, count: 0 };
const heldKeys = new Map();
const measurements = { frames: 0, renderMs: [], width: 0, height: 0, scale: 1 };
const mods = e => (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
function fail(error) {
  stopped = true;
  loading.hidden = false;
  document.querySelector('#status').textContent = 'Runebender could not start';
  document.querySelector('#error').textContent = String(error);
  console.error(error);
}
function feedback() {
  const state = JSON.parse(editor.feedback());
  canvas.style.cursor = state.cursor;
  if (state.clipboard != null && navigator.clipboard) {
    navigator.clipboard.writeText(state.clipboard).catch(() => {});
  }
  if (state.ime) {
    textInput.style.left = `${state.imePosition[0]}px`;
    textInput.style.top = `${state.imePosition[1]}px`;
  }
  if (state.ime && document.activeElement === canvas) {
    textInput.focus({ preventScroll: true });
    editor.text(3, '');
  } else if (!state.ime && document.activeElement === textInput) {
    canvas.focus({ preventScroll: true });
  }
  return state;
}
function draw(now) {
  pending = false;
  if (stopped || !editor) return;
  try {
    flushMove();
    const state = feedback();
    if (state.dirty || state.animate) {
      const start = performance.now();
      const bytes = editor.frame(lastFrame ? now - lastFrame : 16);
      context.putImageData(new ImageData(new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, bytes.byteLength), canvas.width, canvas.height), 0, 0);
      lastFrame = now;
      measurements.frames++;
      measurements.renderMs.push(performance.now() - start);
      if (measurements.renderMs.length > 120) measurements.renderMs.shift();
    }
    const next = feedback();
    if (next.dirty || next.animate) schedule();
  } catch (error) { fail(error); }
}
function schedule() {
  if (editor && !pending && !stopped && !document.hidden) {
    pending = true;
    requestAnimationFrame(draw);
  }
}
function resize() {
  const rect = canvas.getBoundingClientRect();
  const nextScale = window.devicePixelRatio || 1;
  const width = Math.max(1, Math.round(rect.width * nextScale));
  const height = Math.max(1, Math.round(rect.height * nextScale));
  if (canvas.width === width && canvas.height === height && scale === nextScale) return;
  scale = nextScale;
  canvas.width = width;
  canvas.height = height;
  Object.assign(measurements, { width, height, scale });
  editor?.resize(width, height, scale);
  schedule();
}
function sample(e) {
  const rect = canvas.getBoundingClientRect();
  return { x: (e.clientX - rect.left) * canvas.width / rect.width,
    y: (e.clientY - rect.top) * canvas.height / rect.height,
    button: e.button, buttons: e.buttons, modifiers: mods(e) };
}
function pointer(kind, e, count = 0, dx = 0, dy = 0) {
  editor.pointer(kind, e.x, e.y, e.button, e.buttons, count, e.modifiers, dx, dy);
}
function flushMove() {
  if (move) { pointer(0, move); move = undefined; }
}
canvas.addEventListener('pointerdown', e => {
  if (!editor) return;
  e.preventDefault();
  flushMove();
  canvas.focus({ preventScroll: true });
  canvas.setPointerCapture(e.pointerId);
  const at = sample(e);
  const count = e.timeStamp - lastDown.time < 450 && Math.hypot(at.x - lastDown.x, at.y - lastDown.y) < 6 * scale
    ? lastDown.count % 3 + 1 : 1;
  lastDown = { time: e.timeStamp, x: at.x, y: at.y, count };
  pointer(1, at, count);
  schedule();
});
canvas.addEventListener('pointerup', e => {
  if (!editor) return;
  e.preventDefault();
  flushMove();
  pointer(2, sample(e), lastDown.count);
  if (canvas.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId);
  schedule();
});
canvas.addEventListener('pointermove', e => { if (editor) { move = sample(e); schedule(); } });
canvas.addEventListener('pointercancel', e => {
  if (!editor) return;
  flushMove();
  pointer(2, { ...sample(e), buttons: 0 }, 1);
  schedule();
});
canvas.addEventListener('contextmenu', e => e.preventDefault());
canvas.addEventListener('wheel', e => {
  if (!editor) return;
  e.preventDefault();
  flushMove();
  const unit = e.deltaMode === 1 ? 20 : e.deltaMode === 2 ? canvas.clientHeight : 1;
  // Wheel distances are logical browser pixels; pointer positions are physical.
  pointer(3, sample(e), 0, -e.deltaX * unit, -e.deltaY * unit);
  schedule();
}, { passive: false });
for (const target of [canvas, textInput]) for (const name of ['keydown', 'keyup']) target.addEventListener(name, e => {
  if (!editor || e.key === 'F5' || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r')) return;
  // Let the browser produce its paste event; cancelling Cmd/Ctrl-V suppresses it.
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') return;
  if (e.isComposing || composing || e.key === 'Process' || e.key === 'Dead') return;
  e.preventDefault();
  if (name === 'keydown') heldKeys.set(e.code, e.key); else heldKeys.delete(e.code);
  editor.key(name === 'keydown', e.key, e.code, mods(e), e.repeat);
  schedule();
});
function blur() {
  if (!editor) return;
  flushMove();
  for (const [code, key] of heldKeys) editor.key(false, key, code, 0, false);
  heldKeys.clear();
  editor.pointer(2, -1, -1, 0, 0, 1, 0, 0, 0);
  editor.focus(false);
  schedule();
}
for (const target of [canvas, textInput]) target.addEventListener('blur', e => { if (e.relatedTarget !== canvas && e.relatedTarget !== textInput) blur(); });
for (const target of [canvas, textInput]) target.addEventListener('focus', () => { if (editor) { editor.focus(true); schedule(); } });
textInput.addEventListener('compositionstart', () => { composing = true; });
textInput.addEventListener('compositionupdate', e => { editor.text(1, e.data); schedule(); });
textInput.addEventListener('compositionend', e => {
  editor.text(2, e.data); composing = false; textInput.value = ''; schedule();
});
for (const target of [canvas, textInput]) target.addEventListener('paste', e => {
  if (!editor) return;
  e.preventDefault();
  editor.text(0, e.clipboardData.getData('text/plain'));
  schedule();
});
window.addEventListener('blur', blur);
window.addEventListener('resize', resize);
document.addEventListener('visibilitychange', () => { if (!document.hidden) { lastFrame = 0; resize(); schedule(); } });
new ResizeObserver(resize).observe(canvas);
function watchResolution() {
  const query = matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
  query.addEventListener('change', () => { resize(); watchResolution(); }, { once: true });
}
watchResolution();
// Some hosts change devicePixelRatio without a resize or media-query event.
// This checks one number; it never repaints an unchanged or hidden canvas.
setInterval(() => {
  if (!document.hidden && (window.devicePixelRatio || 1) !== scale) resize();
}, 250);
document.querySelector('#reset').addEventListener('click', () => {
  if (confirm('Discard this tab’s edits and reload the sample font?')) location.reload();
});
try {
  await init();
  resize();
  editor = new BrowserEditor(canvas.width, canvas.height, scale);
  window.runebender = { state: () => JSON.parse(editor.state()),
    metrics: () => ({ ...measurements, renderMs: [...measurements.renderMs] }) };
  draw(performance.now());
  loading.hidden = true;
  canvas.focus({ preventScroll: true });
} catch (error) { fail(error); }
