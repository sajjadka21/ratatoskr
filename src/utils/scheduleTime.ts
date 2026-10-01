/** `YYYY-MM-DDTHH:mm` in the viewer's local time, as a <input type="datetime-local"> wants it. */
export function toLocalInput(unixSeconds: number | null | undefined): string {
  if (!unixSeconds || unixSeconds <= 0) return "";
  const date = new Date(unixSeconds * 1000);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** The Unix second for a datetime-local value, or null when it is empty or not a date. */
export function fromLocalInput(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/.test(value)) return null;
  const milliseconds = new Date(value).getTime();
  return Number.isFinite(milliseconds) ? Math.floor(milliseconds / 1000) : null;
}

export type OnceCheck = "ok" | "needsStart" | "endBeforeStart";

/** A one-time run needs a start; an end, when given, must come after it. */
export function checkOnce(start: string, end: string): OnceCheck {
  const startAt = fromLocalInput(start);
  if (startAt === null) return "needsStart";
  const stopAt = fromLocalInput(end);
  if (end !== "" && (stopAt === null || stopAt <= startAt)) return "endBeforeStart";
  return "ok";
}
