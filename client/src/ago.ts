/// Relative rather than absolute: "three weeks ago" is the fact that
/// matters about something you said, and a date makes the reader do the
/// arithmetic.
export function ago(iso: string): string {
  const days = Math.round((Date.now() - new Date(iso).getTime()) / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 31) return `${Math.round(days / 7)} weeks ago`;
  if (days < 365) return `${Math.round(days / 30)} months ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: "short", year: "numeric" });
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
