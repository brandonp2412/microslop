#!/usr/bin/env node

const [port = '9222', ...parts] = process.argv.slice(2);
const expression = parts.join(' ') || 'document.body.innerText';
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((target) => target.type === 'page');
if (!page) throw new Error('No page target found');

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener('message', (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve, reject) => {
  socket.addEventListener('open', resolve, {once: true});
  socket.addEventListener('error', reject, {once: true});
});

const id = nextId++;
const response = await new Promise((resolve) => {
  pending.set(id, resolve);
  socket.send(JSON.stringify({
    id,
    method: 'Runtime.evaluate',
    params: {expression, awaitPromise: true, returnByValue: true},
  }));
});
console.log(JSON.stringify(response.result, null, 2));
socket.close();
