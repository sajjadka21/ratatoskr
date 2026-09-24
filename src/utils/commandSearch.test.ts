import { describe, expect, it } from "vitest";

import { matchScore, searchCommands, type PaletteCommand } from "./commandSearch";

const command = (id: string, label: string, keywords = "", group: PaletteCommand["group"] = "action"): PaletteCommand => ({
  id,
  label,
  keywords,
  group,
  run: () => {},
});

describe("command search", () => {
  it("needs every character in order", () => {
    expect(matchScore("pal", "Pause all")).not.toBeNull();
    expect(matchScore("lap", "Pause all")).toBeNull();
  });

  it("ignores letters scattered across a long text", () => {
    expect(matchScore("pause", "ubuntu-26.04-desktop-amd64.iso releases.ubuntu.com")).toBeNull();
    expect(matchScore("ubu", "ubuntu-26.04-desktop-amd64.iso")).not.toBeNull();
  });

  it("prefers word starts and runs of characters", () => {
    const results = searchCommands(
      [command("parent", "Open parent folder"), command("pause", "Pause all")],
      "pa",
    );
    expect(results[0].id).toBe("pause");
  });

  it("finds Persian commands by their English keywords", () => {
    const results = searchCommands([command("pause", "توقف همه", "pause all stop")], "pause");
    expect(results.map((item) => item.id)).toEqual(["pause"]);
  });

  it("shows actions but not every download when nothing is typed", () => {
    const results = searchCommands(
      [command("a", "Add link"), command("d", "ubuntu.iso", "", "download")],
      "",
    );
    expect(results.map((item) => item.id)).toEqual(["a"]);
  });
});
