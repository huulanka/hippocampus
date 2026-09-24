/// Relative rather than absolute: "three weeks ago" is the fact that
/// matters about something you said, and a date makes the reader do the
/// arithmetic.
export function ago(iso: string, now = Date.now()): string {
  const days = calendarDaysBetween(iso, now);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 31) return `${Math.round(days / 7)} weeks ago`;
  if (days < 365) return `${Math.round(days / 30)} months ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: "short", year: "numeric" });
}

/// Whole calendar days between a moment and now, in local time. Counted
/// from midnight to midnight, not as elapsed hours divided by 24: said at
/// 08:15 is still "today" at 22:00, and said at 23:50 is "yesterday" ten
/// minutes after midnight.
export function calendarDaysBetween(iso: string, now = Date.now()): number {
  const then = new Date(iso);
  const today = new Date(now);
  const a = Date.UTC(then.getFullYear(), then.getMonth(), then.getDate());
  const b = Date.UTC(today.getFullYear(), today.getMonth(), today.getDate());
  return Math.round((b - a) / 86_400_000);
}

/// How far off a moment is, for a meeting: "in 8 min", "in 1 h 20",
/// "now". Minutes are what a meeting's lead is measured in.
export function until(iso: string, now = Date.now()): string {
  const minutes = Math.round((new Date(iso).getTime() - now) / 60_000);
  if (minutes <= 0) return "now";
  if (minutes < 60) return `in ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `in ${hours} h` : `in ${hours} h ${rest}`;
}

export function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}
