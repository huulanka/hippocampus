// How a resolved point in time is written next to an observation.
//
// The precision is honoured rather than rounded away: "next summer" was
// never a date, and printing one would invent an accuracy the speaker did
// not have. A memory system that quietly sharpens what you said is worse
// than one that stays vague.

export interface Happened {
  happened_on: string | null;
  happened_at: string | null;
  happened_precision: string | null;
}

export function whenLabel(happened: Happened): string | null {
  if (!happened.happened_on) return null;
  // Parsed as local noon: a bare "2026-09-22" is parsed as UTC midnight by
  // the Date constructor, which lands on the previous day west of
  // Greenwich and would print the wrong date.
  const day = new Date(`${happened.happened_on}T12:00:00`);

  switch (happened.happened_precision) {
    case "time":
      return happened.happened_at
        ? new Date(happened.happened_at).toLocaleString(undefined, {
            weekday: "short",
            day: "numeric",
            month: "short",
            hour: "2-digit",
            minute: "2-digit",
          })
        : dayLabel(day);
    case "week":
      return `week of ${day.toLocaleDateString(undefined, { day: "numeric", month: "short" })}`;
    case "month":
      return day.toLocaleDateString(undefined, { month: "long", year: "numeric" });
    case "year":
      return String(day.getFullYear());
    default:
      return dayLabel(day);
  }
}

function dayLabel(day: Date): string {
  return day.toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: day.getFullYear() === new Date().getFullYear() ? undefined : "numeric",
  });
}
