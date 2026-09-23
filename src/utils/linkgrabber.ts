import type { AddDownloadAction } from "../components/downloads/AddDownloadModal";

/**
 * Batches larger than this start through a queue instead of all at once. The
 * engine limits how many transfers run, but starting hundreds of tasks
 * directly would still put every one of them into "checking" together.
 */
export const LARGE_BATCH = 20;

/** Largest dropped file read, so a stray video dropped by mistake is refused. */
export const MAX_DROPPED_FILE_BYTES = 2 * 1024 * 1024;

const TEXT_FILE = /\.(txt|text|html?|csv|list|lst|urls?)$/i;

export type BatchAction = {
  action: AddDownloadAction;
  label: string;
  /** Explains why the batch is routed differently, when it is. */
  note: string | null;
};

/**
 * What "start" does for a LinkGrabber selection: the chosen queue if there is
 * one, the default queue for large batches, and a direct start otherwise.
 */
export function batchAction(
  count: number,
  chosenQueueId: string | null,
  defaultQueueId: string | null,
): BatchAction {
  if (chosenQueueId) {
    return {
      action: { kind: "queue", queueId: chosenQueueId },
      label: "Send to queue",
      note: null,
    };
  }

  if (count > LARGE_BATCH && defaultQueueId) {
    return {
      action: { kind: "queue", queueId: defaultQueueId },
      label: `Start ${count} via queue`,
      note: `More than ${LARGE_BATCH} links go through the Default Queue, so they run a few at a time.`,
    };
  }

  return { action: { kind: "start-now" }, label: "Start selected", note: null };
}

type DroppedFile = { name: string; type: string; size: number; text(): Promise<string> };

type Dropped = {
  files: ArrayLike<DroppedFile>;
  getData(format: string): string;
};

/**
 * Reads links out of something dropped onto the input: text files are read,
 * anything else (images, archives) is refused, and a dragged link or piece
 * of text is used as it is.
 */
export async function readDroppedText(dropped: Dropped): Promise<string> {
  const files = Array.from(dropped.files);

  if (files.length === 0) {
    return dropped.getData("text/uri-list") || dropped.getData("text/plain") || "";
  }

  const texts: string[] = [];

  for (const file of files) {
    if (!file.type.startsWith("text/") && !TEXT_FILE.test(file.name)) {
      throw new Error(`${file.name} is not a text file, so no links were read from it.`);
    }

    if (file.size > MAX_DROPPED_FILE_BYTES) {
      throw new Error(`${file.name} is larger than 2 MB, so it was not read.`);
    }

    texts.push(await file.text());
  }

  return texts.join("\n");
}
