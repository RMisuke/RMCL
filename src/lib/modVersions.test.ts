import { describe, expect, it } from "vitest";
import type { ModVersion } from "../ipc/types";
import {
  baseGameVersion,
  compareGameVersions,
  filterModVersions,
  groupModVersions,
  isSnapshotGameVersion,
  primaryGameVersion,
  sortGameVersionsDesc,
  versionChannel,
} from "./modVersions";

function version(partial: Partial<ModVersion> & Pick<ModVersion, "version_number" | "game_versions">): ModVersion {
  return {
    id: partial.id ?? partial.version_number,
    name: partial.name ?? partial.version_number,
    version_number: partial.version_number,
    game_versions: partial.game_versions,
    loaders: partial.loaders ?? ["fabric"],
    download_url: null,
    filename: null,
    sha1: null,
    published: partial.published ?? null,
    version_type: partial.version_type ?? "release",
    file_size: partial.file_size ?? null,
  };
}

describe("game version ordering", () => {
  it("orders numeric releases correctly (1.21.11 > 1.21.8)", () => {
    const sorted = sortGameVersionsDesc(["1.21.8", "1.21.11", "1.20.4", "1.21.5"]);
    expect(sorted).toEqual(["1.21.11", "1.21.8", "1.21.5", "1.20.4"]);
    expect(compareGameVersions("1.21.11", "1.21.8")).toBeGreaterThan(0);
  });

  it("keeps releases above snapshots and detects both snapshot forms", () => {
    expect(isSnapshotGameVersion("25w45a")).toBe(true);
    expect(isSnapshotGameVersion("1.21.5-pre2")).toBe(true);
    expect(isSnapshotGameVersion("26.2-rc-1")).toBe(true);
    expect(isSnapshotGameVersion("1.21.5")).toBe(false);
    expect(baseGameVersion("1.21.5-rc1")).toBe("1.21.5");
    expect(baseGameVersion("26.2-rc-1")).toBe("26.2");
    const sorted = sortGameVersionsDesc(["25w45a", "1.21.5-pre2", "1.21.5"]);
    expect(sorted[0]).toBe("1.21.5");
  });
});

describe("version grouping", () => {
  const versions: ModVersion[] = [
    version({ version_number: "3.0.0", game_versions: ["1.21.11"], published: "2026-01-10T00:00:00Z" }),
    version({ version_number: "2.0.0", game_versions: ["1.21.11", "1.21.8"], published: "2025-12-01T00:00:00Z" }),
    version({ version_number: "1.0.0", game_versions: ["1.21.8"], published: "2025-10-01T00:00:00Z" }),
  ];

  it("groups by the newest supported release and keeps versions newest-first", () => {
    const groups = groupModVersions(versions);
    expect(groups.map((g) => g.gameVersion)).toEqual(["1.21.11", "1.21.8"]);
    expect(groups[0].versions.map((v) => v.version_number)).toEqual(["3.0.0", "2.0.0"]);
    expect(groups[0].latest.version_number).toBe("3.0.0");
  });

  it("re-groups around an explicit game-version filter", () => {
    expect(primaryGameVersion(versions[1])).toBe("1.21.11");
    expect(primaryGameVersion(versions[1], "1.21.8")).toBe("1.21.8");
    const filtered = filterModVersions(versions, { gameVersion: "1.21.8" });
    const groups = groupModVersions(filtered, "1.21.8");
    expect(groups).toHaveLength(1);
    expect(groups[0].gameVersion).toBe("1.21.8");
    expect(groups[0].versions.map((v) => v.version_number)).toEqual(["2.0.0", "1.0.0"]);
  });

  it("filters by game version, loader aliases and release channel", () => {
    const beta = version({
      version_number: "4.0.0-beta.1",
      game_versions: ["1.21.11"],
      loaders: ["neoforge"],
      version_type: "beta",
    });
    const all = [...versions, beta];
    expect(filterModVersions(all, { loader: "neo_forge" })).toHaveLength(1);
    expect(filterModVersions(all, { channel: "beta" })[0].version_number).toBe("4.0.0-beta.1");
    expect(filterModVersions(all, { gameVersion: "1.21.8", channel: "release" })).toHaveLength(2);
    expect(versionChannel(beta)).toBe("beta");
  });
});
