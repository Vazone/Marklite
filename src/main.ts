import { mount } from 'svelte';
import './styles/globals.css';
import './styles/themes.css';
import './styles/markdown-preview.css';
import App from './app/App.svelte';

const app = mount(App, {
  target: document.getElementById('app') as HTMLElement
});

export default app;
