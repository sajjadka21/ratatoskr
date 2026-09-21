export function extractHttpUrls(
  value: string,
): string[] {
  const matches =
    value.match(/https?:\/\/[^\s<>"'`]+/gi) ?? [];

  const unique = new Set<string>();

  for (const match of matches) {
    const candidate = match.replace(
      /[),.;!?}\]]+$/g,
      "",
    );

    try {
      const url = new URL(candidate);

      if (
        url.protocol === "http:" ||
        url.protocol === "https:"
      ) {
        unique.add(url.toString());
      }
    } catch {
      // Ignore invalid URL-like clipboard content.
    }
  }

  return [...unique];
}
