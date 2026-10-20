<script lang="ts">
  let {
    checked = false,
    label = '',
    disabled = false,
    onchange,
  }: { checked?: boolean; label?: string; disabled?: boolean; onchange?: (v: boolean) => void } = $props();
</script>

<label class="toggle" class:disabled>
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    class="track"
    class:on={checked}
    {disabled}
    onclick={() => onchange?.(!checked)}
  >
    <span class="thumb"></span>
  </button>
  {#if label}<span class="text">{label}</span>{/if}
</label>

<style>
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  .toggle.disabled {
    opacity: 0.5;
    cursor: default;
  }
  .track {
    width: 34px;
    height: 18px;
    border-radius: 9px;
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    position: relative;
    flex: none;
    transition: background 0.15s;
  }
  .track.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text);
    transition: transform 0.15s;
  }
  .track.on .thumb {
    transform: translateX(16px);
    background: #04221f;
  }
  .text {
    color: var(--text-dim);
  }
</style>
