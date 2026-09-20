import { mount, unmount } from 'svelte';
import { expect, test } from 'vitest';
import ExportProgress from './ExportProgress.svelte';

test('the last finished chapter does not show overall completion before commit', async () => {
  const target = document.createElement('div');
  const component = mount(ExportProgress, { target, props: { progress: {
    jobId:'job', format:'png', stage:'writing', status:'running', work:{kind:'chapter',index:2,total:2,completed:2}
  } } });
  expect(target.textContent).toContain('Chapter 2 of 2');
  expect(target.querySelector('progress')?.hasAttribute('value')).toBe(false);
  await unmount(component);
});
