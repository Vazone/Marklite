export async function verifyScrollFeedback(client, split) {
  await client.evaluate(`(async () => {
    const deadline = performance.now() + 30000;
    while (!document.querySelector('.preview-content')?.textContent.trim()) {
      if (performance.now() >= deadline) throw new Error('Preview did not render');
      await new Promise(resolve => setTimeout(resolve, 30));
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  })()`);
  const sample = () => client.evaluate(`(() => {
    const preview = document.querySelector('.preview-content');
    return { preview: preview.scrollTop, editor: document.querySelector('.cm-scroller')?.scrollTop ?? null,
      segments: preview.querySelectorAll('[data-preview-segment]').length };
  })()`);
  async function wheel(deltaY, count) {
    const point = await client.evaluate(`(() => {
      const rect = document.querySelector('.preview-content').getBoundingClientRect();
      return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
    })()`);
    for (let index = 0; index < count; index++) {
      await client.send('Input.dispatchMouseEvent', { type: 'mouseWheel', ...point, deltaX: 0, deltaY });
      await new Promise(resolve => setTimeout(resolve, 8));
    }
    await new Promise(resolve => setTimeout(resolve, 1000));
  }
  const before = await sample();
  await wheel(700, 30);
  const down = await sample();
  await new Promise(resolve => setTimeout(resolve, 1200));
  const settled = await sample();
  if (down.preview <= before.preview + 100) throw new Error('Fast wheel did not move preview');
  if (Math.abs(settled.preview - down.preview) > 2) throw new Error(`Preview moved after wheel stopped: ${JSON.stringify({ down, settled })}`);
  if (split && !(down.editor > before.editor)) throw new Error('Preview wheel did not synchronize the editor');
  await wheel(-400, 12);
  const up = await sample();
  if (!(up.preview < settled.preview)) throw new Error('Reverse wheel did not move upward');
  await new Promise(resolve => setTimeout(resolve, 800));
  const final = await sample();
  if (Math.abs(final.preview - up.preview) > 2) throw new Error('Reverse wheel kept scrolling after it stopped');
  return { before, down, settled, up, final };
}
