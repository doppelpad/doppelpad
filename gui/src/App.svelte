<script lang="ts">
  import { onMount } from 'svelte';
  import { Activity, Gamepad2, List, PenTool, StickyNote } from '@lucide/svelte';
  import { t } from './lib/i18n/index.svelte';
  import { drawingsFor } from './lib/model/devices';
  import { VIEWS } from './lib/model/views';
  import { app } from './lib/state/app.svelte';
  import { isTextEntry, menuOpen, popoverOpen } from './lib/dom';
  import { matchShortcut } from './lib/state/shortcuts';
  import { runShortcut } from './components/shortcutActions';
  import { runPadControls } from './components/padNavigation';
  import ShortcutsDialog from './components/ShortcutsDialog.svelte';
  import FindPanel from './components/FindPanel.svelte';
  import LiveOutput from './components/LiveOutput.svelte';
  import { win } from './lib/state/window.svelte';
  import ProfilePanel from './components/ProfilePanel.svelte';
  import Header from './components/Header.svelte';
  import Toolbar from './components/Toolbar.svelte';
  import BottomBar from './components/BottomBar.svelte';
  import DiagnosticsBar from './components/DiagnosticsBar.svelte';
  import OutputPanel from './components/OutputPanel.svelte';
  import BindingPanel from './components/binding/BindingPanel.svelte';
  import StickPanel from './components/binding/StickPanel.svelte';
  import ControlList from './components/views/ControlList.svelte';
  import ControllerView from './components/views/ControllerView.svelte';
  import KeyboardView from './components/views/KeyboardView.svelte';
  import MouseView from './components/views/MouseView.svelte';
  import ShortcutsPanel from './components/views/ShortcutsPanel.svelte';
  import MacroEditor from './components/views/MacroEditor.svelte';
  import AnalogView from './components/views/AnalogView.svelte';
  import RadialView from './components/views/RadialView.svelte';
  import SectorPanel from './components/radial/SectorPanel.svelte';
  import SettingsDialog from './components/SettingsDialog.svelte';
  import PromptDialog from './components/ui/PromptDialog.svelte';
  import ConfirmDialog from './components/ui/ConfirmDialog.svelte';
  import { modalOpen } from './components/ui/Dialog.svelte';
  import ContextMenu from './components/ui/ContextMenu.svelte';
  import RewasdDialog from './components/RewasdDialog.svelte';
  import WelcomeWizard from './components/WelcomeWizard.svelte';
  import Toasts from './components/ui/Toasts.svelte';
  import Tooltip from './components/ui/Tooltip.svelte';
  import { tip } from './lib/tooltip.svelte';

  let settingsOpen = $state(false);
  let listMode = $state(false);
  /** The live output panel is open. */
  let liveOpen = $state(false);

  /** Front and back drawings for the connected controller model. */
  const drawings = $derived(drawingsFor(app.primary));
  const drawing = $derived(app.view === 'back' ? drawings.back : drawings.front);

  onMount(() => {
    app.padHandler = runPadControls;
    app.init();
    win.init();
  });

  // A focus ring while the controller drives the app.
  $effect(() => {
    document.documentElement.classList.toggle('pad-nav', app.settings.pad_navigation);
  });

  function onKeydown(e: KeyboardEvent) {
    // A text field keeps its own keys (undo, copy, delete); a dialog or menu owns the keyboard.
    const hit = matchShortcut(e, {
      keyboardView: app.view === 'keyboard',
      typing: isTextEntry(e.target),
      overlay: modalOpen() || menuOpen() || popoverOpen(),
    });
    if (hit && runShortcut(hit.id, hit.digit)) e.preventDefault();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="shell">
  <ProfilePanel />

  <section class="main">
    <Header onsettings={() => (settingsOpen = true)} />
    {#if app.profile}
      <Toolbar />
      <DiagnosticsBar />
      <div class="workspace">
        <div class="center">
          {#if VIEWS[app.view].controls}
            <div class="view-tools">
              <button
                class="icon-btn"
                class:active={liveOpen}
                aria-pressed={liveOpen}
                use:tip={t('live_output.title')}
                onclick={() => (liveOpen = !liveOpen)}
              >
                <Activity size={16} />
              </button>
              <button
                class="icon-btn"
                class:active={app.settings.show_notes}
                aria-pressed={app.settings.show_notes}
                use:tip={t(app.settings.show_notes ? 'views.show_actions' : 'views.show_notes')}
                onclick={() => app.setSettings({ show_notes: !app.settings.show_notes })}
              >
                <StickyNote size={16} />
              </button>
              {#if app.view === 'front' || app.view === 'back'}
                <button
                  class="icon-btn view-mode"
                  use:tip={listMode ? t('views.as_drawing') : t('views.as_list')}
                  onclick={() => (listMode = !listMode)}
                >
                  {#if listMode}<PenTool size={16} />{:else}<List size={16} />{/if}
                </button>
              {/if}
            </div>
            {#if liveOpen}<LiveOutput onclose={() => (liveOpen = false)} />{/if}
          {/if}
          {#if app.view === 'output'}
            <OutputPanel />
          {:else if app.view === 'front' || app.view === 'back'}
            {#if listMode}
              <ControlList back={app.view === 'back'} device={drawing} />
            {:else}
              <ControllerView device={drawing} />
            {/if}
          {:else if app.view === 'keyboard'}
            <KeyboardView />
          {:else if app.view === 'mouse'}
            <MouseView />
          {:else if app.view === 'shortcuts'}
            <ShortcutsPanel />
          {:else if app.view === 'macro'}
            <MacroEditor />
          {:else if app.view === 'analog'}
            <AnalogView />
          {:else if app.view === 'radial'}
            <RadialView />
          {/if}
        </div>
        <aside class="side">
          {#if app.view === 'radial'}<SectorPanel />{:else if app.stick}<StickPanel side={app.stick} />{:else}<BindingPanel />{/if}
        </aside>
      </div>
    {:else}
      <div class="empty">
        <Gamepad2 size={56} strokeWidth={1.2} />
        {#if app.gameDir}
          <p>{t('header.no_config')}</p>
          <button class="btn primary" onclick={() => app.newConfig(t('header.default_config_name'))}>{t('header.new_config')}</button>
        {:else}
          <p>{t('header.no_selection')}</p>
        {/if}
      </div>
    {/if}
  </section>

  <div class="bottom"><BottomBar /></div>
</div>

{#if settingsOpen}<SettingsDialog onclose={() => (settingsOpen = false)} />{/if}
{#if app.shortcutsOpen}<ShortcutsDialog onclose={() => (app.shortcutsOpen = false)} />{/if}
{#if app.find && app.profile}<FindPanel />{/if}
<PromptDialog />
<ConfirmDialog />
<RewasdDialog />
{#if !app.settings.welcome_done}<WelcomeWizard />{/if}
<ContextMenu />
<Toasts />
<Tooltip />

<style>
  .shell {
    display: grid;
    grid-template-columns: 264px minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) auto;
    height: 100%;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .bottom {
    grid-column: 1 / -1;
  }
  .workspace {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    min-height: 0;
  }
  .center {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: auto;
  }
  .view-tools {
    position: absolute;
    top: 10px;
    right: 12px;
    z-index: 5;
    display: flex;
    gap: 4px;
  }
  .side {
    border-left: 1px solid var(--line);
    background: var(--bg-1);
    min-height: 0;
    overflow-y: auto;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--text-faint);
  }
  .empty p {
    margin: 0;
    color: var(--text-dim);
  }
</style>
