<script lang="ts">
  // An overlay window's content in the chosen corner of the window, at the chosen opacity.
  import type { Snippet } from 'svelte';
  import type { Corner } from '../../lib/types';

  let { corner, opacity, children }: { corner: Corner; /** In percent. */ opacity: number; children: Snippet } = $props();
</script>

<div class="frame" class:right={corner.endsWith('right')} class:bottom={corner.startsWith('bottom')} style:opacity={opacity / 100}>
  {@render children()}
</div>

<style>
  .frame {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: flex-start;
    justify-content: flex-start;
    padding: 6px;
  }
  .frame.right {
    justify-content: flex-end;
  }
  .frame.bottom {
    align-items: flex-end;
  }
</style>
