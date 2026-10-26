import { describe, expect, it } from "vitest";
import { describeExit } from "./exitCodes";

describe("describeExit", () => {
  it("0 is normal", () => {
    expect(describeExit(0)).toMatchObject({ severity: "normal", key: "logs.exit.normal" });
  });

  it("known signals map to dedicated keys", () => {
    expect(describeExit(134)).toMatchObject({ severity: "fatal", key: "logs.exit.sigabrt" });
    expect(describeExit(137)).toMatchObject({ severity: "fatal", key: "logs.exit.sigkill" });
    expect(describeExit(139)).toMatchObject({ severity: "fatal", key: "logs.exit.sigsegv" });
    expect(describeExit(143)).toMatchObject({ severity: "error", key: "logs.exit.sigterm" });
  });

  it("the 128+signal range is fatal with the signal number", () => {
    expect(describeExit(133)).toMatchObject({
      severity: "fatal",
      key: "logs.exit.signal",
      params: { code: 133, signal: 5 },
    });
  });

  it("plain app errors and unknown codes", () => {
    expect(describeExit(1)).toMatchObject({ severity: "error", key: "logs.exit.appError" });
    expect(describeExit(42)).toMatchObject({ severity: "error", key: "logs.exit.abnormal" });
  });
});
