// The app's questions, hints and prompts wait in one queue and show one at a time.

import { describe, expect, it } from 'vitest';
import { Dialogs, QUESTION_IDS, type QuestionId } from './dialogs.svelte';
import { has } from '../i18n/index.svelte';

/** A queue whose silenced questions live in a plain list. */
function dialogs(silenced: QuestionId[] = []) {
  return new Dialogs({ silenced: () => silenced, silence: (id) => silenced.push(id) });
}

describe('dialogs', () => {
  it('a second question waits until the first is answered; neither is lost', async () => {
    const d = dialogs();
    const first = d.confirmAsk('Open?');
    const second = d.choose('Keep?', { alt: 'Keep' });
    expect(d.question?.message).toBe('Open?');
    d.answerQuestion('ok');
    expect(d.question?.message).toBe('Keep?');
    d.answerQuestion('alt');
    await expect(first).resolves.toBe(true);
    await expect(second).resolves.toBe('alt');
    expect(d.question).toBeNull();
  });

  it('a prompt asked while another is open is asked after it', async () => {
    const d = dialogs();
    const first = d.ask('Name?', 'a');
    const second = d.ask('Other name?');
    expect(d.prompt?.title).toBe('Name?');
    d.prompt!.value = '  typed  ';
    d.answerPrompt(true);
    expect(d.prompt?.title).toBe('Other name?');
    d.answerPrompt(false);
    await expect(first).resolves.toBe('typed');
    await expect(second).resolves.toBeNull();
  });

  it('a hint waits behind a question and shows right after it', async () => {
    const d = dialogs();
    const question = d.confirmAsk('Delete?');
    const hint = d.inform('Steam takes the controller.');
    const name = d.ask('Name?');
    expect(d.question?.info).toBeFalsy();
    d.answerQuestion(null);
    expect(d.question).toMatchObject({ message: 'Steam takes the controller.', info: true });
    expect(d.prompt).toBeNull();
    d.answerQuestion('ok');
    expect(d.prompt?.title).toBe('Name?');
    d.answerPrompt(false);
    await expect(question).resolves.toBe(false);
    await expect(hint).resolves.toBeUndefined();
    await expect(name).resolves.toBeNull();
  });

  it('"Don\'t ask again" counts with OK only; a silenced question answers OK at once', async () => {
    const silenced: QuestionId[] = [];
    const d = dialogs(silenced);
    const cancelled = d.confirmAsk('Clear?', { id: 'clear_mappings' });
    d.answerQuestion(null, true);
    await expect(cancelled).resolves.toBe(false);
    expect(silenced).toEqual([]);
    const ok = d.confirmAsk('Clear?', { id: 'clear_mappings' });
    d.answerQuestion('ok', true);
    await expect(ok).resolves.toBe(true);
    expect(silenced).toEqual(['clear_mappings']);
    await expect(d.confirmAsk('Clear?', { id: 'clear_mappings' })).resolves.toBe(true);
    await expect(d.inform('Again?', 'clear_mappings')).resolves.toBeUndefined();
    expect(d.question).toBeNull();
  });

  it('every question that can be silenced has a name in the settings', () => {
    expect(QUESTION_IDS.filter((id) => !has(`questions.ids.${id}`))).toEqual([]);
  });
});
