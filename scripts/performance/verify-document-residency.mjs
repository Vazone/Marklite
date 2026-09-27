export async function verifyDocumentResidency(client, openPath, paths) {
  const checks = [];
  const name = paths[0].split(/[\\/]/).at(-1);
  async function loadState() {
    return client.evaluate(`([...document.querySelectorAll('[role="tab"]')].find(tab => tab.textContent.includes(${JSON.stringify(name)})))?.parentElement.dataset.loadState`);
  }
  async function ready(name) {
    await client.evaluate(`(async () => {
      const deadline = performance.now() + 30000;
      while (!document.querySelector('[role="tab"][aria-selected="true"]')?.textContent.includes(${JSON.stringify(name)})
        || !document.querySelector('.cm-content') || document.querySelector('.editor-stage')?.getAttribute('aria-busy') !== 'false') {
        if (performance.now() >= deadline) throw new Error('Tab did not load: ' + ${JSON.stringify(name)});
        await new Promise(resolve => setTimeout(resolve, 20));
      }
      await new Promise(resolve => setTimeout(resolve, 150));
    })()`);
  }
  async function activate(name) {
    await client.evaluate(`(() => {
      const tab = [...document.querySelectorAll('[role="tab"]')].find(tab => tab.textContent.includes(${JSON.stringify(name)}));
      if (!tab) throw new Error('Missing retained tab');
      tab.click();
    })()`);
    await ready(name);
  }
  async function key(key, code, modifiers = 0) {
    await client.send('Input.dispatchKeyEvent', { type: 'keyDown', key, code, modifiers });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, modifiers });
  }

  // Preserve a nonempty native selection across clean eviction and rehydration.
  await key('f', 'KeyF', 2);
  await client.send('Input.insertText', { text: 'MarkLite' });
  await client.evaluate(`(async () => {
    const deadline = performance.now() + 10000;
    while (!/^1\\/\\d+$/.test(document.querySelector('.find-input-shell span')?.textContent ?? '')) {
      if (performance.now() >= deadline) throw new Error('Selection search did not finish');
      await new Promise(resolve => setTimeout(resolve, 20));
    }
  })()`);
  await key('Escape', 'Escape');
  const selection = await client.evaluate('window.getSelection().toString()');
  if (selection !== 'MarkLite') throw new Error(`Unexpected initial selection: ${selection}`);
  for (const path of paths.slice(1, 4)) { await openPath(path); await ready(path.split(/[\\/]/).at(-1)); }
  if (await loadState() !== 'unloaded') throw new Error('Clean tab stayed resident beyond the document budget');
  await activate(paths[0].split(/[\\/]/).at(-1));
  if (await loadState() !== 'loaded') throw new Error('Clean tab did not reload');
  await client.evaluate(`document.querySelector('.cm-content').focus()`);
  if (await client.evaluate('window.getSelection().toString()') !== selection) throw new Error('Selection changed after clean reload');
  checks.push('clean LRU eviction re-reads the file and restores the selection');

  await client.send('Input.insertText', { text: 'UNSAVED' });
  await client.evaluate('new Promise(resolve => setTimeout(resolve, 200))');
  for (const path of paths.slice(1)) { await openPath(path); await ready(path.split(/[\\/]/).at(-1)); }
  if (await loadState() !== 'loaded') throw new Error('Dirty tab was evicted');
  await activate(paths[0].split(/[\\/]/).at(-1));
  if (!(await client.evaluate(`document.querySelector('.cm-content').textContent.includes('UNSAVED')`))) throw new Error('Dirty text was lost');
  checks.push('dirty text survives cache pressure without a disk reload');
  return { checks, tabs: await client.evaluate(`([...document.querySelectorAll('[role="tab"]')].map(tab => ({ title: tab.textContent, loadState: tab.parentElement.dataset.loadState })))`) };
}
