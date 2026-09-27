import assert from 'node:assert/strict';
import test from 'node:test';
import { CdpClient } from '../native/cdp.mjs';
import { waitReady } from '../native/scroll.mjs';

test('startup navigation can destroy its initial context without failing the regression', async () => {
  let attempts = 0;
  await waitReady({ evaluate: async () => {
    attempts++;
    if (attempts === 1) throw new Error('Execution context was destroyed.');
    return attempts === 3;
  } });
  assert.equal(attempts, 3);
  await assert.rejects(waitReady({ evaluate: async () => { throw new Error('Application failure'); } }), /Application failure/);
  await assert.rejects(waitReady({ evaluate: async () => false }, 1), /Editor startup timeout/);
});

test('shared CDP transport reports protocol errors, pending closure and deadlines', async (t) => {
  const Original = globalThis.WebSocket;
  let socket;
  class Socket extends EventTarget {
    constructor() { super(); socket = this; queueMicrotask(() => this.dispatchEvent(new Event('open'))); }
    send(json) { this.last = JSON.parse(json); }
    reply(value) { this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify({ id: this.last.id, ...value }) })); }
    close() { this.dispatchEvent(new Event('close')); }
  }
  globalThis.WebSocket = Socket;
  t.after(() => { globalThis.WebSocket = Original; });
  const client = new CdpClient('ws://127.0.0.1/test');
  await client.connect();
  const success = client.evaluate('1');
  socket.reply({ result: { result: { value: 1 } } });
  assert.equal(await success, 1);
  const protocolError = client.send('Invalid.method');
  socket.reply({ error: { message: 'Unknown method' } });
  await assert.rejects(protocolError, /Unknown method/);
  const exception = client.evaluate('throw new Error()');
  socket.reply({ result: { exceptionDetails: { text: 'Evaluation failed' } } });
  await assert.rejects(exception, /Evaluation failed/);
  await assert.rejects(client.send('No.response', {}, 10), /timed out/);
  assert.equal(client.pending.size, 0);
  const pending = client.send('Pending.method');
  client.close();
  await assert.rejects(pending, /connection closed/);
  assert.equal(client.pending.size, 0);
});
