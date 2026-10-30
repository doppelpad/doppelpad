// The app's own dialogs: questions (OK and Cancel, maybe a second answer), hints with OK only,
// and prompts for a name or a longer text. Never the system's message box (in the webview,
// `window.confirm` returns a promise without asking). They wait in one queue and show one at a
// time, in the order they were asked; each promise settles when its dialog is answered.

/**
 * Questions that may be silenced: only ones whose change can be undone. Deleting games or
 * configs and discarding unsaved changes always ask.
 */
export const QUESTION_IDS = ['power_command', 'clear_mappings', 'whole_stick_replace', 'steam_input'] as const;
export type QuestionId = (typeof QUESTION_IDS)[number];

/**
 * What a question (ConfirmDialog) can show and offer besides its message; without options it
 * asks with a warning sign, OK and Cancel.
 */
export interface QuestionOptions {
  title?: string;
  /** Longer text below the message, kept as written (line breaks too), e.g. release notes. */
  details?: string | null;
  /** Rows under the message: a line of code (a command line, say) and notes under it. */
  items?: { code: string; notes: string[] }[];
  /** The label of the OK button. */
  ok?: string;
  /** The label of the Cancel button. */
  cancel?: string;
  /** A second answer, between Cancel and OK. */
  alt?: string;
  /** The sign before the title. */
  kind?: 'warning' | 'update';
  /** A question that can be silenced ("Don't ask again"); a silenced one answers OK at once. */
  id?: QuestionId;
}

/** The button a question was answered with; `null` for Cancel (and Escape). */
export type Answer = 'ok' | 'alt' | null;

/** A question or a hint waiting in the queue. */
export interface Question extends QuestionOptions {
  type: 'question';
  message: string;
  /** Something to read, not to answer: only OK, and "Don't show again" with an id. */
  info?: boolean;
  resolve: (answer: Answer) => void;
}

/** A prompt waiting in the queue; `value` is what the field holds. */
export interface Prompt {
  type: 'prompt';
  title: string;
  value: string;
  multiline: boolean;
  resolve: (value: string | null) => void;
}

export class Dialogs {
  /** What waits to be answered; the first one shows. */
  private queue = $state<(Question | Prompt)[]>([]);
  /** The question or hint showing now. */
  question = $derived(this.queue[0]?.type === 'question' ? this.queue[0] : null);
  /** The prompt showing now. */
  prompt = $derived(this.queue[0]?.type === 'prompt' ? this.queue[0] : null);

  /** `silenced`: the questions not to ask again; `silence` adds one to them. */
  constructor(private readonly settings: { silenced: () => readonly string[]; silence: (id: QuestionId) => void }) {}

  private silenced(id: QuestionId | undefined): boolean {
    return !!id && this.settings.silenced().includes(id);
  }

  /** Ask for a text (a name, a description); `null` when cancelled or left empty. */
  ask(title: string, value = '', multiline = false): Promise<string | null> {
    return new Promise((resolve) => this.queue.push({ type: 'prompt', title, value, multiline, resolve }));
  }

  /** Ask a question; which answer it got. With `options.id` it offers "Don't ask again", and a silenced id answers OK at once. */
  choose(message: string, options: QuestionOptions = {}): Promise<Answer> {
    if (this.silenced(options.id)) return Promise.resolve('ok');
    return new Promise((resolve) => this.queue.push({ ...options, type: 'question', message, resolve }));
  }

  /** Ask an OK/Cancel question; `true` for OK. */
  async confirmAsk(message: string, options: QuestionOptions = {}): Promise<boolean> {
    return (await this.choose(message, options)) === 'ok';
  }

  /** Tell something; with an `id` it offers "Don't show again", and a silenced id shows nothing. */
  inform(message: string, id?: QuestionId): Promise<void> {
    if (this.silenced(id)) return Promise.resolve();
    return new Promise((resolve) => this.queue.push({ type: 'question', message, id, info: true, resolve: () => resolve() }));
  }

  /** Answer the question showing now; `silence`: its "Don't ask again" is ticked, which counts with OK. */
  answerQuestion(answer: Answer, silence = false) {
    const q = this.question;
    if (!q) return;
    this.queue.shift();
    if (answer === 'ok' && silence && q.id) this.settings.silence(q.id);
    q.resolve(answer);
  }

  /** Answer the prompt showing now: with what its field holds (`ok`), or cancel it. */
  answerPrompt(ok: boolean) {
    const p = this.prompt;
    if (!p) return;
    this.queue.shift();
    p.resolve(ok ? p.value.trim() || null : null);
  }
}
