const SKIPPED_KEY = "ratatoskr.update.skipped";

export type OfferedUpdate = { version: string; currentVersion?: string; notes?: string | null };

/** The version the user chose not to be asked about again, if any. */
export function skippedVersion(storage: Pick<Storage, "getItem"> | undefined): string | null {
  try {
    return storage?.getItem(SKIPPED_KEY) ?? null;
  } catch {
    return null;
  }
}

export function skipVersion(storage: Pick<Storage, "setItem"> | undefined, version: string): void {
  try {
    storage?.setItem(SKIPPED_KEY, version);
  } catch {
    // Remembering the choice is a convenience; failing to do so only means being asked again.
  }
}

/** Ask unless this exact version was skipped; a newer one asks again. */
export function shouldAsk(update: OfferedUpdate, skipped: string | null): boolean {
  return update.version.trim() !== "" && update.version !== skipped;
}
