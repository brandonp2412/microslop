#!/usr/bin/env node

const [port = '9222', file] = process.argv.slice(2);
if (!file) throw new Error('Usage: cdp-file-upload.mjs PORT FILE');
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((target) => target.type === 'page');
if (!page) throw new Error('No page target found');

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
const events = [];
socket.addEventListener('message', (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  } else if (message.method) {
    events.push(message);
  }
});
await new Promise((resolve, reject) => {
  socket.addEventListener('open', resolve, {once: true});
  socket.addEventListener('error', reject, {once: true});
});

async function send(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve) => {
    pending.set(id, resolve);
    socket.send(JSON.stringify({id, method, params}));
  });
}

await send('Page.enable');
await send('Page.setInterceptFileChooserDialog', {enabled: true});
const button = await send('Runtime.evaluate', {
  expression: `(()=>{const e=[...document.querySelectorAll('flt-semantics[role=button]')].find(e=>e.innerText==='Upload image'); if(!e) throw new Error('Upload image button missing'); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2};})()`,
  returnByValue: true,
});
const point = button.result?.result?.value;
if (!point) throw new Error(`Could not resolve Upload image button coordinates: ${JSON.stringify(button)}`);
await send('Input.dispatchMouseEvent', {type: 'mousePressed', x: point.x, y: point.y, button: 'left', clickCount: 1});
await send('Input.dispatchMouseEvent', {type: 'mouseReleased', x: point.x, y: point.y, button: 'left', clickCount: 1});

const chooser = await new Promise((resolve, reject) => {
  const deadline = Date.now() + 5000;
  const timer = setInterval(() => {
    const index = events.findIndex((event) => event.method === 'Page.fileChooserOpened');
    if (index >= 0) {
      clearInterval(timer);
      resolve(events.splice(index, 1)[0]);
    } else if (Date.now() > deadline) {
      clearInterval(timer);
      reject(new Error('No file chooser opened'));
    }
  }, 20);
});
await send('DOM.setFileInputFiles', {
  files: [file],
  backendNodeId: chooser.params.backendNodeId,
});
console.log(JSON.stringify(chooser.params));
socket.close();
