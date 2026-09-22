import type { DownloadListItem, TaskAction } from "../types/download";

/**
 * The actions a task accepts in its current state.
 *
 * This mirrors the canonical state machine in `dm-common`: the backend is
 * still the authority and will refuse anything illegal, but offering a button
 * that is guaranteed to fail is its own kind of bug.
 */
export function availableActions(
  item: DownloadListItem,
): TaskAction[] {
  switch (item.status.toLowerCase()) {
    case "created":
      return ["start", "cancel"];

    case "queued":
      return ["cancel"];

    case "probing":
    case "downloading":
      return ["pause", "cancel"];

    case "paused":
      return ["resume", "restart", "cancel"];

    case "retrying":
      return ["resume", "cancel"];

    case "failed":
    case "cancelled":
      return ["retry", "restart"];

    default:
      return [];
  }
}

/**
 * The one action worth putting directly on the row. Everything else stays in
 * the menu so a list of hundreds of rows keeps a single, predictable control.
 */
export function primaryAction(
  item: DownloadListItem,
): TaskAction | null {
  const actions = availableActions(item);

  for (const candidate of [
    "pause",
    "resume",
    "start",
    "retry",
    "restart",
  ] as const) {
    if (actions.includes(candidate)) {
      return candidate;
    }
  }

  return null;
}

export const ACTION_LABELS: Record<TaskAction, string> = {
  start: "Start",
  pause: "Pause",
  resume: "Resume",
  cancel: "Cancel",
  retry: "Retry",
  restart: "Restart from zero",
};
