export type PaletteCommand = {
  id: string;
  label: string;
  /** Right-aligned hint, such as a shortcut or a download's status. */
  hint?: string;
  /** Extra words that should find this command, in any language. */
  keywords?: string;
  group: "action" | "go" | "download";
  run: () => void;
};

/**
 * Scores how well `query` matches `text`: every query character must appear
 * in order. Consecutive matches and matches at word starts score higher,
 * so "pa" ranks "Pause all" above "Open parent folder". Null means no match.
 */
export function matchScore(query: string, text: string): number | null {
  const needle = query.trim().toLowerCase();
  if (!needle) return 0;
  const haystack = text.toLowerCase();
  let score = 0;
  let position = -1;
  let previous = -2;
  let adjacent = 0;
  let letters = 0;
  for (const character of needle) {
    if (character === " ") continue;
    const found = haystack.indexOf(character, position + 1);
    if (found < 0) return null;
    letters += 1;
    score += 1;
    if (found === previous + 1) {
      score += 3;
      adjacent += 1;
    }
    if (found === 0 || /[\s\-_./·]/.test(haystack[found - 1] ?? "")) score += 2;
    previous = found;
    position = found;
  }
  // Letters scattered all over a long text are a coincidence, not a match:
  // at least half of them must follow one another.
  if (letters > 2 && adjacent < Math.floor((letters - 1) / 2)) return null;
  // Shorter texts win ties: the match is a larger part of them.
  return score - haystack.length / 100;
}

/** The commands that match, best first; everything when the query is empty. */
export function searchCommands(commands: PaletteCommand[], query: string, limit = 50): PaletteCommand[] {
  if (!query.trim()) return commands.filter((command) => command.group !== "download").slice(0, limit);
  return commands
    .map((command) => ({
      command,
      score: matchScore(query, `${command.label} ${command.keywords ?? ""}`),
    }))
    .filter((entry): entry is { command: PaletteCommand; score: number } => entry.score !== null)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((entry) => entry.command);
}
