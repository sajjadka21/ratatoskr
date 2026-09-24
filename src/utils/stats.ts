import type { FileKind } from "./fileKind";
import { kindOfExtension } from "./fileKind";
import type { ActivityDay, NamedTotal } from "../types/download";

/** Extensions folded into the kinds the rest of the interface uses. */
export function kindTotals(extensions: NamedTotal[]): Array<NamedTotal & { name: FileKind }> {
  const totals = new Map<FileKind, NamedTotal & { name: FileKind }>();
  for (const extension of extensions) {
    const kind = kindOfExtension(extension.name);
    const entry = totals.get(kind) ?? { name: kind, count: 0, bytes: 0 };
    entry.count += extension.count;
    entry.bytes += extension.bytes;
    totals.set(kind, entry);
  }
  return [...totals.values()].sort((a, b) => b.bytes - a.bytes || a.name.localeCompare(b.name));
}

/**
 * A round top for the daily chart and the grid lines under it: 1, 2 or 5
 * times a power of 1024-ish steps would read oddly, so the top is chosen in
 * the unit the largest day is shown in.
 */
export function chartScale(days: ActivityDay[]): { max: number; ticks: number[] } {
  const largest = Math.max(0, ...days.map((day) => day.domesticBytes + day.internationalBytes));
  if (largest <= 0) return { max: 0, ticks: [] };
  const unit = 1024 ** Math.min(4, Math.floor(Math.log(largest) / Math.log(1024)));
  const inUnit = largest / unit;
  const magnitude = 10 ** Math.floor(Math.log10(inUnit));
  const step = [1, 2, 2.5, 5, 10].map((factor) => factor * magnitude).find((candidate) => inUnit / candidate <= 4)!;
  const top = Math.ceil(inUnit / step) * step;
  const ticks: number[] = [];
  for (let value = step; value <= top + step / 1e6; value += step) ticks.push(value * unit);
  return { max: top * unit, ticks };
}

/** Which days get a label under the chart: never more than about seven. */
export function labelledDays(count: number): Set<number> {
  const every = Math.max(1, Math.ceil(count / 7));
  const labelled = new Set<number>();
  for (let index = count - 1; index >= 0; index -= every) labelled.add(index);
  return labelled;
}
