import { test } from "node:test";
import assert from "node:assert/strict";
import { manifest } from "./latest-json.mjs";

const files = [
  "Omnyra_1.0.0_amd64.AppImage",
  "Omnyra_1.0.0_amd64.AppImage.sig",
  "Omnyra_1.0.0_x64-setup.exe",
  "Omnyra_1.0.0_x64-setup.exe.sig",
  "x.deb",
];
const read = (f) => `SIG(${f})\n`;

test("builds platforms from signed bundles", () => {
  const m = manifest(files, read, "v1.0.0", "https://h/d", "2026-11-22T00:00:00Z");
  assert.equal(m.version, "1.0.0");
  assert.equal(m.platforms["linux-x86_64"].signature, "SIG(Omnyra_1.0.0_amd64.AppImage.sig)");
  assert.equal(m.platforms["windows-x86_64"].url, "https://h/d/Omnyra_1.0.0_x64-setup.exe");
});

test("fails without signatures or bundles", () => {
  assert.throws(() => manifest(["a.AppImage"], read, "v1", "b"), /missing signature/);
  assert.throws(() => manifest(["x.deb"], read, "v1", "b"), /no updatable/);
});
