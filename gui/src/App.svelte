<script lang="ts">
  import { onMount } from 'svelte';
  import { Gamepad2, List, PenTool } from '@lucide/svelte';
  import { t } from './lib/i18n/index.svelte';
  import { drawingsFor } from './lib/model/devices';
  import { app } from './lib/state/app.svelte';
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
  import SettingsDialog from './components/SettingsDialog.svelte';
  import PromptDialog from './components/ui/PromptDialog.svelte';
  import RewasdDialog from './components/RewasdDialog.svelte';
  import WelcomeWizard from './components/WelcomeWizard.svelte';
  import Toasts from './components/ui/Toasts.svelte';

  let settingsOpen = $state(false);
  let listMode = $state(false);

  /** Front and back drawings for the connected controller model. */
  const drawings = $derived(drawingsFor(app.primary));
  const drawing = $derived(app.view === 'back' ? drawings.back : drawings.front);

  onMount(() => {
    app.init();
    win.init();
  });

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (app.dirty) app.save();
    }
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
          {#if app.view === 'output'}
            <OutputPanel />
          {:else if app.view === 'front' || app.view === 'back'}
            <button
              class="icon-btn view-mode"
              title={listMode ? t('views.as_drawing') : t('views.as_list')}
              onclick={() => (listMode = !listMode)}
            >
              {#if listMode}<PenTool size={16} />{:else}<List size={16} />{/if}
            </button>
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
          {/if}
        </div>
        <aside class="side">
          {#if app.stick}<StickPanel side={app.stick} />{:else}<BindingPanel />{/if}
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
<PromptDialog />
<RewasdDialog />
{#if !app.settings.welcome_done}<WelcomeWizard />{/if}
<Toasts />

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
  .view-mode {
    position: absolute;
    top: 10px;
    right: 12px;
    z-index: 5;
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
