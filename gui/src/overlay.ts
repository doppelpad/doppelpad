// Entry of the overlay windows over games (gui/src-tauri/src/overlay.rs).
import { mount } from 'svelte';
import './styles';
import Overlay from './components/overlay/Overlay.svelte';

const overlay = mount(Overlay, { target: document.getElementById('app')! });

export default overlay;
