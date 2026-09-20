import { createClassComponent } from 'svelte/legacy';
import { mount, tick, unmount, type SvelteComponent } from 'svelte';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { defaultSettings } from '../../lib/tauriApi';
import SettingsDialog from './SettingsDialog.svelte';

let target: HTMLDivElement;
let component: ReturnType<typeof mount> | undefined;
let legacyComponent: (SvelteComponent & { $set(props: Record<string, unknown>): void; $destroy(): void }) | undefined;

afterEach(async () => {
  if (component) await unmount(component);
  legacyComponent?.$destroy();
  component = undefined;
  legacyComponent = undefined;
  target?.remove();
});

describe('SettingsDialog modal contract', () => {
  test('focuses inside and handles Escape without a window-level listener', async () => {
    const onClose = vi.fn();
    target = document.createElement('div');
    document.body.append(target);
    component = mount(SettingsDialog, {
      target,
      props: {
        open: true,
        settings: defaultSettings,
        onSave: vi.fn(),
        onReset: vi.fn(),
        onClose
      }
    });
    await tick();
    const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
    await vi.waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));

    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));

    expect(onClose).toHaveBeenCalledOnce();
  });

  test('does not expose the removed content zoom setting', async () => {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(SettingsDialog, {
      target,
      props: {
        open: true,
        settings: defaultSettings,
        onSave: vi.fn(),
        onReset: vi.fn(),
        onClose: vi.fn()
      }
    });
    await tick();

    expect(target.querySelector('input[type="range"]')).toBeNull();
    expect(target.textContent).not.toContain('Editor and preview zoom');
  });

  test('offers the extensible language registry and saves simplified Chinese', async () => {
    const onSave = vi.fn();
    target = document.createElement('div');
    document.body.append(target);
    component = mount(SettingsDialog, {
      target,
      props: {
        open: true,
        settings: defaultSettings,
        onSave,
        onReset: vi.fn(),
        onClose: vi.fn()
      }
    });
    await tick();

    const language = target.querySelector<HTMLSelectElement>('select')!;
    expect([...language.options].map((option) => [option.value, option.textContent])).toEqual([
      ['en', 'English'],
      ['zh-CN', '简体中文']
    ]);
    language.value = 'zh-CN';
    language.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
    [...target.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) => button.textContent === 'Save settings')!
      .click();

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ language: 'zh-CN' }));
  });

  test('disables persistence actions while an operation is pending', async () => {
    let resolveSave!: (settings: typeof defaultSettings) => void;
    const onSave = vi.fn(() => new Promise<typeof defaultSettings>((resolve) => {
      resolveSave = resolve;
    }));
    target = document.createElement('div');
    document.body.append(target);
    component = mount(SettingsDialog, {
      target,
      props: {
        open: true,
        settings: defaultSettings,
        onSave,
        onReset: vi.fn(),
        onClose: vi.fn()
      }
    });
    await tick();
    const actions = [...target.querySelectorAll<HTMLButtonElement>('.dialog-footer button')];

    actions[1].click();
    await tick();
    expect(actions.every((button) => button.disabled)).toBe(true);

    resolveSave(defaultSettings);
    await vi.waitFor(() => expect(actions.every((button) => !button.disabled)).toBe(true));
  });

  test('does not let a response from a closed dialog round overwrite a reopened draft', async () => {
    let resolveSave!: (settings: typeof defaultSettings) => void;
    const onSave = vi.fn(() => new Promise<typeof defaultSettings>((resolve) => {
      resolveSave = resolve;
    }));
    target = document.createElement('div');
    document.body.append(target);
    legacyComponent = createClassComponent({
      component: SettingsDialog,
      target,
      props: {
        open: true,
        settings: defaultSettings,
        onSave,
        onReset: vi.fn(),
        onClose: vi.fn()
      }
    }) as typeof legacyComponent;
    await tick();
    const cornerRadius = () => target.querySelector<HTMLInputElement>('input[type="number"]')!;
    cornerRadius().value = '7';
    cornerRadius().dispatchEvent(new Event('input', { bubbles: true }));
    target.querySelectorAll<HTMLButtonElement>('.dialog-footer button')[1].click();
    await tick();

    legacyComponent!.$set({ open: false });
    await tick();
    legacyComponent!.$set({
      open: true,
      settings: { ...defaultSettings, cornerRadius: 12 }
    });
    await tick();
    expect(cornerRadius().value).toBe('12');
    cornerRadius().value = '14';
    cornerRadius().dispatchEvent(new Event('input', { bubbles: true }));

    resolveSave({ ...defaultSettings, cornerRadius: 7 });
    await tick();
    await Promise.resolve();
    expect(cornerRadius().value).toBe('14');
  });
});
