import { describe, expect, it } from "vitest";
import { scopeLabels } from "./scopes";

const t = ((k: string) => `T:${k}`) as never;

describe("scopeLabels", () => {
  it("translates known scopes and keeps unknown ones", () => {
    expect(
      scopeLabels(["https://www.googleapis.com/auth/youtube.readonly", "custom.scope"], t),
    ).toEqual(["T:scopes.ytReadonly", "custom.scope"]);
  });
  it("hides identity scopes", () => {
    expect(scopeLabels(["openid", "email"], t)).toEqual([]);
  });
});
