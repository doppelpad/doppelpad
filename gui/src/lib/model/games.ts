// A game's settings: the programs that switch to it.

/** Whether `programs` has `exe` already; program file names are the same whatever their case. */
export const hasProgram = (programs: readonly string[], exe: string) => {
  const e = exe.trim().toLowerCase();
  return programs.some((p) => p.toLowerCase() === e);
};

/** `programs` with `exe` added at the end, unless it is empty or there already. */
export function addProgram(programs: string[], exe: string): string[] {
  const e = exe.trim();
  return e && !hasProgram(programs, e) ? [...programs, e] : programs;
}
