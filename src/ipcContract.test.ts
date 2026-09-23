import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

/**
 * These tests guard the boundary the compiler cannot see: command names and
 * argument names travel to Rust as strings, so a rename on either side fails
 * silently at runtime. Reading both sides here turns that into a failed test.
 */

const FRONTEND_ROOT = join(process.cwd(), "src");
const BACKEND_ROOT = join(process.cwd(), "src-tauri", "src");

function sourceFiles(directory: string): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry);

    if (statSync(path).isDirectory()) {
      return sourceFiles(path);
    }

    return /\.tsx?$/.test(entry) && !/\.test\.tsx?$/.test(entry) ? [path] : [];
  });
}

function frontendSource(): string {
  return sourceFiles(FRONTEND_ROOT)
    .map((path) => readFileSync(path, "utf8"))
    .join("\n");
}

/** Every Rust source of the desktop host; events live in several modules. */
function backendSource(): string {
  return readdirSync(BACKEND_ROOT)
    .filter((entry) => entry.endsWith(".rs"))
    .map((entry) => readFileSync(join(BACKEND_ROOT, entry), "utf8"))
    .join("\n");
}

/** Commands registered with Tauri, in registration order. */
function registeredCommands(rust: string): string[] {
  const handler = /generate_handler!\[([\s\S]*?)\]/.exec(rust);

  if (!handler) {
    throw new Error("could not find the Tauri command registration");
  }

  return handler[1]
    .split(",")
    .map((name) => name.trim())
    .filter(Boolean);
}

/** Parameter names of one `#[tauri::command]` function. */
function commandParameters(rust: string, command: string): string[] {
  const signature = new RegExp(
    `fn ${command}\\(([\\s\\S]*?)\\)\\s*->`,
    "m",
  ).exec(rust);

  if (!signature) {
    return [];
  }

  return signature[1]
    .split(",")
    .map((parameter) => parameter.trim())
    .filter(Boolean)
    .map((parameter) => parameter.split(":")[0].trim())
    // `app` and `state` are injected by Tauri, never sent by the caller.
    .filter((name) => name !== "app" && name !== "state");
}

/** Every `invoke("name", { ... })` call found in the frontend. */
function invocations(
  typescript: string,
): Array<{ command: string; argumentNames: string[] }> {
  const calls: Array<{ command: string; argumentNames: string[] }> = [];
  const pattern = /invoke(?:<[^>]*>)?\(\s*"([a-z_]+)"/g;

  for (
    let match = pattern.exec(typescript);
    match !== null;
    match = pattern.exec(typescript)
  ) {
    calls.push({
      command: match[1],
      argumentNames: argumentNamesAt(
        typescript,
        pattern.lastIndex,
      ),
    });
  }

  return calls;
}

/**
 * Reads the top-level keys of the argument object that follows a command
 * name. Returns an empty list for a call with no arguments.
 */
function argumentNamesAt(source: string, from: number): string[] {
  const rest = source.slice(from);
  const opening = /^\s*,\s*\{/.exec(rest);

  if (!opening) {
    return [];
  }

  let depth = 0;
  let end = -1;

  for (let index = opening[0].length - 1; index < rest.length; index += 1) {
    if (rest[index] === "{") depth += 1;
    if (rest[index] === "}") {
      depth -= 1;

      if (depth === 0) {
        end = index;
        break;
      }
    }
  }

  if (end === -1) {
    return [];
  }

  const body = rest.slice(opening[0].length, end);
  const names: string[] = [];
  let nesting = 0;
  let token = "";

  for (const character of body) {
    if (character === "{" || character === "[" || character === "(") nesting += 1;
    if (character === "}" || character === "]" || character === ")") nesting -= 1;

    if (character === "," && nesting === 0) {
      names.push(token);
      token = "";
      continue;
    }

    token += character;
  }

  names.push(token);

  return names
    .map((entry) => entry.split(":")[0].trim())
    .filter((entry) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(entry));
}

function toSnakeCase(value: string): string {
  return value.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
}

describe("IPC contract", () => {
  const rust = backendSource();
  const typescript = frontendSource();
  const commands = registeredCommands(rust);
  const calls = invocations(typescript);

  it("finds the commands the frontend actually calls", () => {
    expect(calls.length).toBeGreaterThan(0);
    expect(commands.length).toBeGreaterThan(0);
  });

  it("only calls commands that are registered with Tauri", () => {
    const unregistered = [
      ...new Set(
        calls
          .map((call) => call.command)
          .filter((command) => !commands.includes(command)),
      ),
    ];

    expect(unregistered).toEqual([]);
  });

  it("passes argument names the command actually declares", () => {
    const mismatches: string[] = [];

    for (const call of calls) {
      const parameters = commandParameters(rust, call.command);

      for (const argument of call.argumentNames) {
        if (!parameters.includes(toSnakeCase(argument))) {
          mismatches.push(`${call.command}.${argument}`);
        }
      }
    }

    expect(mismatches).toEqual([]);
  });

  it("listens to the event names the backend publishes", () => {
    const published = [
      ...rust.matchAll(/const [A-Z_]+_EVENT: &str = "([a-z-]+)";/g),
    ].map((match) => match[1]);

    const listened = [
      ...typescript.matchAll(/const [A-Z_]+_EVENT = "([a-z-]+)";/g),
    ].map((match) => match[1]);

    expect(published.length).toBeGreaterThan(0);
    expect(listened.sort()).toEqual(published.sort());
  });
});
