// Entry of the overlay windows over games (gui/src-tauri/src/overlay.rs).
import { mount } from 'svelte';
import '@fontsource/inter/400.css';
import '@fontsource/inter/500.css';
import '@fontsource/inter/600.css';
import './lib/theme/tokens.css';
import Overlay from './components/overlay/Overlay.svelte';

const overlay = mount(Overlay, { target: document.getElementById('app')! });

export default overlay;
