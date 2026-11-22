// SPDX-License-Identifier: GPL-3.0-only
// Send synthetic events to a recording-only local Audience Live session.
// Requires Node.js and the `ws` package (also discoverable through NODE_PATH).
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline/promises');
const { setTimeout: delay } = require('node:timers/promises');
const WebSocket = require('ws');

async function main() {
  const input = readline.createInterface({ input: process.stdin, output: process.stdout });
  let join;
  try {
    join = new URL(await input.question('Recording-only local join URL: '));
  } finally {
    input.close();
  }
  if (join.protocol !== 'http:' || !['127.0.0.1', 'localhost'].includes(join.hostname)) {
    throw new Error('Use a dedicated loopback HTTP session, never a hosted room.');
  }
  const code = join.pathname.split('/').filter(Boolean).at(-1);
  const token = join.hash.slice(1);
  if (!code || !/^[A-Za-z0-9_-]{43}$/.test(token)) {
    throw new Error('Expected the complete local join URL including its token fragment.');
  }
  const comments = JSON.parse(fs.readFileSync(
    path.join(__dirname, '../docs/assets/audience-live/comments.json'), 'utf8'));
  if (comments.length !== 10 || comments.filter(c => c.language === 'en').length !== 6
      || comments.filter(c => c.language === 'ja').length !== 4
      || comments.some(c => c.handle !== 'Anonymous')) {
    throw new Error('Expected ten anonymous comments with a 60:40 language split.');
  }
  const socket = new WebSocket(`ws://${join.host}/ws/audience/${code}`, { origin: join.origin });
  const pending = new Map();
  let counter = 0;
  let welcomeResolve;
  let welcomeReject;
  const welcome = new Promise((resolve, reject) => {
    welcomeResolve = resolve;
    welcomeReject = reject;
  });
  const welcomeTimeout = setTimeout(() => welcomeReject(new Error('Welcome timeout')), 5000);
  socket.on('open', () => socket.send(JSON.stringify({
    protocol: 'qp.audience', version: '1', type: 'hello', token,
  })));
  socket.on('message', raw => {
    const event = JSON.parse(raw.toString());
    if (event.type === 'welcome') welcomeResolve();
    if (event.type === 'result') {
      const request = pending.get(event.request_id);
      if (request) {
        pending.delete(event.request_id);
        clearTimeout(request.timeout);
        if (event.status === 'accepted') request.resolve();
        else request.reject(new Error(`Event rejected: ${event.status}`));
      }
    }
  });
  const fail = error => {
    welcomeReject(error);
    for (const request of pending.values()) {
      clearTimeout(request.timeout);
      request.reject(error);
    }
    pending.clear();
  };
  socket.on('error', fail);
  socket.on('close', () => fail(new Error('Recording connection closed')));
  const send = fields => new Promise((resolve, reject) => {
    const request_id = String(++counter);
    const timeout = setTimeout(() => {
      pending.delete(request_id);
      reject(new Error('Acknowledgement timeout; event was not retried'));
    }, 4000);
    pending.set(request_id, { resolve, reject, timeout });
    socket.send(JSON.stringify({ protocol: 'qp.audience', version: '1', request_id, ...fields }));
  });
  try {
    await welcome;
    clearTimeout(welcomeTimeout);
    console.log('Connected. Recording countdown: 3 seconds.');
    await delay(3000);
    console.log('START: record 27 seconds, then end the session.');
    const start = performance.now();
    const kinds = ['thumbs_up', 'heart', 'applause', 'laugh', 'question'];
    for (let index = 0; index < comments.length; index++) {
      const comment = comments[index];
      await delay(Math.max(0, comment.at * 1000 - (performance.now() - start)));
      await send({ type: 'comment', body: comment.body, handle: comment.handle });
      await send({ type: 'reaction', kind: kinds[index % kinds.length] });
    }
    console.log('Accepted: 10 comments (6 English, 4 Japanese) and 10 reactions.');
  } finally {
    clearTimeout(welcomeTimeout);
    socket.close();
  }
}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
