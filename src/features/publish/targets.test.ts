import { describe, expect, it } from "vitest";
import { captionOf, draftFor, isComplete, localToIso, type TargetState } from "./targets";

const target = (over: Partial<TargetState> = {}): TargetState => ({
  accountId: "a",
  platform: "tiktok",
  custom: false,
  text: { title: "Own", description: "", tags: "" },
  visibility: "private",
  madeForKids: null,
  render: {
    aspect: "vertical",
    fit: "crop",
    startS: null,
    endS: null,
    maxDurationS: 600,
    maxSizeMb: null,
  },
  scheduleLocal: "",
  ...over,
});

describe("multi-network targets", () => {
  const common = { title: "Hola", description: "Desc #mc", tags: "mc, speed run" };
  it("uses common or custom text", () => {
    expect(draftFor(target(), common, "/v.mp4")).toMatchObject({
      title: "Hola",
      tags: ["mc", "speed run"],
    });
    expect(draftFor(target({ custom: true }), common, "/v.mp4").title).toBe("Own");
  });
  it("builds captions like the backend", () => {
    expect(captionOf(common)).toBe("Hola\n\nDesc #mc\n\n#speedrun");
  });
  it("requires made-for-kids only on YouTube", () => {
    expect(isComplete(target())).toBe(true);
    expect(isComplete(target({ platform: "youtube" }))).toBe(false);
    expect(isComplete(target({ platform: "youtube", madeForKids: false }))).toBe(true);
  });
  it("converts local schedule to UTC", () => {
    expect(localToIso("")).toBeNull();
    expect(localToIso("2026-11-22T18:30")).toBe(new Date("2026-11-22T18:30").toISOString());
  });
});
