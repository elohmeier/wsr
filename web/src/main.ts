import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import { prepareIsolation } from './isolation';
prepareIsolation().then(ready => {
  if (ready) mount(App, { target: document.getElementById('app')! });
}).catch(error => {
  document.getElementById('app')!.textContent = error.message;
});
