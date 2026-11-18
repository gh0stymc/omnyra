/** Local-time calendar helpers. Everything stored is UTC (RFC 3339); the UI works in the
 * user's local time and converts at the edges. */

export function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

export function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n, d.getHours(), d.getMinutes());
}

/** Monday of the week containing `d`. */
export function startOfWeek(d: Date): Date {
  const day = (d.getDay() + 6) % 7; // 0 = Monday
  return addDays(startOfDay(d), -day);
}

/** 6 weeks (42 days) starting on the Monday on or before the 1st of `d`'s month. */
export function monthGrid(d: Date): Date[] {
  const first = startOfWeek(new Date(d.getFullYear(), d.getMonth(), 1));
  return Array.from({ length: 42 }, (_, i) => addDays(first, i));
}

/** `YYYY-MM-DD` of the local day. */
export function dayKey(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function sameDay(a: Date, b: Date): boolean {
  return dayKey(a) === dayKey(b);
}

/** Moves an instant to another local day, keeping its local time of day; with `hour`,
 * also sets the hour (keeping minutes). Returns RFC 3339 UTC. */
export function moveTo(atIso: string, day: Date, hour?: number): string {
  const at = new Date(atIso);
  const target = new Date(
    day.getFullYear(),
    day.getMonth(),
    day.getDate(),
    hour ?? at.getHours(),
    at.getMinutes(),
  );
  return target.toISOString();
}

/** `<input type="datetime-local">` value for an ISO instant, in local time. */
export function toLocalInput(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${dayKey(d)}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** Inverse of `toLocalInput` (empty → null). */
export function fromLocalInput(v: string): string | null {
  return v ? new Date(v).toISOString() : null;
}
