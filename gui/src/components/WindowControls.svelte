<script lang="ts">
  import { t } from '../lib/i18n/index.svelte';
  import { platform, win } from '../lib/state/window.svelte';
</script>

<!-- macOS keeps its native traffic lights (see main.rs); Windows and Linux draw their own. -->
{#if platform === 'windows'}
  <div class="controls win" class:inactive={!win.focused}>
    <button tabindex="-1" title={t('window.minimize')} aria-label={t('window.minimize')} onclick={() => win.minimize()}>
      &#xE921;
    </button>
    <button
      tabindex="-1"
      title={win.maximized ? t('window.restore') : t('window.maximize')}
      aria-label={win.maximized ? t('window.restore') : t('window.maximize')}
      onclick={() => win.toggleMaximize()}
    >
      {#if win.maximized}&#xE923;{:else}&#xE922;{/if}
    </button>
    <button tabindex="-1" class="close" title={t('window.close')} aria-label={t('window.close')} onclick={() => win.close()}>
      &#xE8BB;
    </button>
  </div>
{:else if platform === 'linux'}
  <div class="controls gtk" class:inactive={!win.focused}>
    <button tabindex="-1" title={t('window.minimize')} aria-label={t('window.minimize')} onclick={() => win.minimize()}>
      <svg viewBox="0 0 16 16"><path d="M4.5 10.5h7" /></svg>
    </button>
    <button
      tabindex="-1"
      title={win.maximized ? t('window.restore') : t('window.maximize')}
      aria-label={win.maximized ? t('window.restore') : t('window.maximize')}
      onclick={() => win.toggleMaximize()}
    >
      {#if win.maximized}
        <svg viewBox="0 0 16 16"><path d="M4.5 6.5h5v5h-5z M6.5 6.5v-2h5v5h-2" /></svg>
      {:else}
        <svg viewBox="0 0 16 16"><path d="M4.5 4.5h7v7h-7z" /></svg>
      {/if}
    </button>
    <button tabindex="-1" title={t('window.close')} aria-label={t('window.close')} onclick={() => win.close()}>
      <svg viewBox="0 0 16 16"><path d="M5 5l6 6M11 5l-6 6" /></svg>
    </button>
  </div>
{/if}

<style>
  .controls {
    display: flex;
    align-self: stretch;
    align-items: center;
    /* Stay usable above dialog backdrops. */
    position: relative;
    z-index: 110;
  }

  /* Windows 11 caption buttons: full-height, 46 px wide, Segoe glyphs, red close. */
  .win button {
    width: 46px;
    height: 100%;
    font-family: 'Segoe Fluent Icons', 'Segoe MDL2 Assets';
    font-size: 10px;
    color: var(--text);
    transition: background 0.08s;
  }
  .win button:hover {
    background: rgba(255, 255, 255, 0.07);
  }
  .win button:active {
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-dim);
  }
  .win .close:hover {
    background: #c42b1c;
    color: #fff;
  }
  .win .close:active {
    background: #c84031;
    color: rgba(255, 255, 255, 0.8);
  }
  .win.inactive button:not(:hover) {
    color: var(--text-faint);
  }

  /* GNOME/libadwaita title buttons: small round buttons with symbolic icons. */
  .gtk {
    gap: 10px;
    margin-left: 6px;
  }
  .gtk button {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.08);
    color: var(--text);
  }
  .gtk button:hover {
    background: rgba(255, 255, 255, 0.14);
  }
  .gtk button:active {
    background: rgba(255, 255, 255, 0.22);
  }
  .gtk.inactive button:not(:hover) {
    color: var(--text-faint);
    background: rgba(255, 255, 255, 0.04);
  }
  .gtk svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
