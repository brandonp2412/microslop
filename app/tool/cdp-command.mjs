#!/usr/bin/env node

const [port = '9222', method, paramsJson = '{}'] = process.argv.slice(2);
if (!method) throw new Error('Usage: cdp-command.mjs PORT METHOD [PARAMS_JSON]');
const params = JSON.parse(paramsJson);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((target) => target.type === 'page');
if (!page) throw new Error('No page target found');

const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener('open', resolve, {once: true});
  socket.addEventListener('error', reject, {once: true});
});
const id = 1;
const response = await new Promise((resolve) => {
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(event.data);
    if (message.id === id) resolve(message);
  });
  socket.send(JSON.stringify({id, method, params}));
});
console.log(JSON.stringify(response, null, 2));
socket.close();
