import '@fontsource/manrope/index.css';
import { mount } from 'svelte';
import './lib/styles/global.css';
import App from './App.svelte';

const app = mount(App, {
  target: document.getElementById('app') as HTMLElement
});

export default app;
