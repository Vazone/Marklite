/** Native UI semantics, separate from latency measurements in the runner. */
export async function verifyEditorSearch(client) {
  const initial = await client.evaluate(`({
    query: document.querySelector('.find-input-shell input').value,
    counter: document.querySelector('.find-input-shell span').textContent
  })`);
  const count = Number(initial.counter.split('/')[1]);
  if (!(count > 1)) throw new Error('Search semantics fixture needs multiple complete matches');
  const checks = [];
  async function key(key, code, modifiers = 0) {
    await client.send('Input.dispatchKeyEvent', { type: 'keyDown', key, code, modifiers });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, modifiers });
  }
  async function input(selector, text) {
    await client.evaluate(`(() => { const input = document.querySelector(${JSON.stringify(selector)}); input.focus(); input.select(); })()`);
    await client.send('Input.insertText', { text });
  }
  async function counter(expected) {
    return client.evaluate(`(async () => {
      const deadline = performance.now() + 30000;
      for (;;) {
        const value = document.querySelector('.find-input-shell span')?.textContent ?? '';
        if (/^\\d+\\/${expected}$/.test(value)) return value;
        if (performance.now() >= deadline) throw new Error('Expected ${expected} matches; found ' + value);
        await new Promise(resolve => setTimeout(resolve, 10));
      }
    })()`);
  }
  await input('.find-input-shell input', 'a');
  await input('.find-input-shell input', 'MARKLITE_NO_MATCH_0114');
  await counter(0);
  await client.evaluate('new Promise(resolve => setTimeout(resolve, 100))');
  await counter(0);
  checks.push('obsolete dense query cannot overwrite the latest zero-match result');
  await input('.find-input-shell input', initial.query);
  await counter(count);
  await key('Enter', 'Enter');
  const next = await counter(count);
  if (next !== `2/${count}`) throw new Error(`Next-match navigation returned ${next}`);
  checks.push('next-match navigation retains complete result ordering');
  await client.evaluate(`document.querySelector('.replace-toggle').click()`);
  await input('.replace-input-shell input', 'REPLACED');
  await client.evaluate(`document.querySelectorAll('.replace-row button')[0].click()`);
  await counter(count - 1);
  await client.evaluate(`document.querySelector('.cm-content').focus()`);
  await key('z', 'KeyZ', 2);
  await counter(count);
  checks.push('replace one and one undo restore the complete match count');
  await client.evaluate(`document.querySelectorAll('.replace-row button')[1].click()`);
  await counter(0);
  await client.evaluate(`document.querySelector('.cm-content').focus()`);
  await key('z', 'KeyZ', 2);
  await counter(count);
  checks.push('replace all and one undo restore every match');
  await input('.find-input-shell input', 'a');
  await key('Escape', 'Escape');
  await client.evaluate(`(async () => {
    await new Promise(resolve => setTimeout(resolve, 100));
    if (document.querySelector('.find-popover') || document.querySelector('.cm-searchMatch')) {
      throw new Error('Closed search retained UI or decorations');
    }
  })()`);
  checks.push('closing a running search releases decorations without a late result');
  await client.evaluate(`document.querySelector('.cm-content').focus()`);
  await key('End', 'End', 2);
  await client.send('Input.insertText', { text: '\n\n**WORKER_RESULT_0114**' });
  await client.evaluate(`(async () => {
    const deadline = performance.now() + 15000;
    while (![...document.querySelectorAll('.cm-content span')].some(node =>
      node.textContent.includes('WORKER_RESULT_0114') && Number(getComputedStyle(node).fontWeight) >= 700)) {
      if (performance.now() >= deadline) throw new Error('Background parse did not restore semantic highlighting');
      await new Promise(resolve => setTimeout(resolve, 30));
    }
  })()`);
  checks.push('real Worker returned a syntax tree and restored bold highlighting');
  return { initial, checks };
}
