import { mount } from 'svelte';
import App from './App.svelte';
import { initializeSession } from './api';
import './style.css';

void initializeSession().then(() => mount(App, { target: document.getElementById('app')! }));
