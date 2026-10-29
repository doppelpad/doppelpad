// Updates: ask the update endpoints (GitHub) for a newer version, offer it in the app's own
// question, and download and install it.

import { api, errorText } from '../api';
import { t } from '../i18n/index.svelte';
import type { UpdateInfo } from '../types';
import type { Toast } from './app.svelte';
import type { QuestionOptions } from './dialogs.svelte';
import { platform } from './window.svelte';

export class Updater {
  /** The update check runs. */
  checking = $state(false);
  /** While an update downloads: its progress in percent. */
  progress = $state<number | null>(null);

  /** `confirmDiscard`: asks before unsaved changes go with the restart. */
  constructor(
    private readonly host: {
      toast: (text: string, kind?: Toast['kind']) => void;
      confirmAsk: (message: string, options?: QuestionOptions) => Promise<boolean>;
      confirmDiscard: () => Promise<boolean>;
    },
  ) {}

  /** A check or a download is under way. */
  get busy(): boolean {
    return this.checking || this.progress !== null;
  }

  /**
   * Look for a newer version and offer to install it. `manual` (*Check now*) also says when
   * there is none or the check failed; the check at start-up leaves that to the log.
   */
  async check(manual = false) {
    if (this.busy) return;
    this.checking = true;
    let update: UpdateInfo | null;
    try {
      update = await api.updateCheck();
    } catch (e) {
      if (manual) this.host.toast(t('updates.check_failed', { error: errorText(e) }), 'error');
      return;
    } finally {
      this.checking = false;
    }
    if (!update) {
      if (manual) this.host.toast(t('updates.up_to_date'), 'ok');
      return;
    }
    const available = { linux: 'updates.available_linux', macos: 'updates.available_macos', windows: 'updates.available' }[platform];
    const install = await this.host.confirmAsk(t(available, { version: update.version, current: update.current }), {
      title: t('updates.title'),
      details: update.notes,
      ok: t('updates.install'),
      cancel: t('updates.later'),
      kind: 'update',
    });
    if (install) await this.install();
  }

  /** Download and install the update the last check found; the app starts again when it worked. */
  private async install() {
    if (!(await this.host.confirmDiscard())) return;
    this.progress = 0;
    this.host.toast(t('updates.downloading'));
    try {
      await api.updateInstall((percent) => (this.progress = percent));
    } catch (e) {
      this.host.toast(t('updates.install_failed', { error: errorText(e) }), 'error');
    } finally {
      this.progress = null;
    }
  }
}
