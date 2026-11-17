import { describe, expect, it } from "vitest";
import i18n from "../../i18n";
import { issueMessage } from "./issues";

describe("issueMessage", () => {
  const t = i18n.getFixedT("es");
  it("formats byte and duration limits", () => {
    expect(
      issueMessage(
        { field: "thumbnail", code: "too_big", severity: "error", limit: 2 * 1024 * 1024 },
        t,
        "es",
      ),
    ).toBe("La miniatura supera 2 MB.");
    expect(
      issueMessage(
        { field: "file", code: "needs_verification", severity: "warning", limit: 900 },
        t,
        "es",
      ),
    ).toBe("Dura más de 15 min: tu canal debe estar verificado por teléfono.");
  });
  it("falls back to the raw code for unknown issues", () => {
    expect(issueMessage({ field: "x", code: "y", severity: "error", limit: null }, t, "es")).toBe(
      "x: y",
    );
  });
});
