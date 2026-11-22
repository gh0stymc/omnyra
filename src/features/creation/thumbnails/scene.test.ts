import { describe, expect, it } from "vitest";
import {
  addLayer,
  assetIds,
  coverTransform,
  duplicateLayer,
  emptyScene,
  hitTest,
  imageLayer,
  moveLayer,
  parseScene,
  removeLayer,
  resizeScene,
  shapeLayer,
  textLayer,
  updateLayer,
} from "./scene";

describe("thumbnail scene", () => {
  it("adds, updates, reorders, duplicates and removes layers", () => {
    let s = emptyScene();
    const a = textLayer(s, "A");
    const b = shapeLayer(s, "rect");
    s = addLayer(addLayer(s, a), b);
    expect(s.layers.map((l) => l.id)).toEqual([a.id, b.id]);
    s = moveLayer(s, a.id, "up");
    expect(s.layers.map((l) => l.id)).toEqual([b.id, a.id]);
    expect(moveLayer(s, a.id, "up")).toBe(s);
    s = updateLayer(s, a.id, { text: "B" });
    expect(s.layers[1].text).toBe("B");
    s = duplicateLayer(s, b.id);
    expect(s.layers).toHaveLength(3);
    expect(s.layers[1].x).toBe(b.x + 20);
    s = removeLayer(s, b.id);
    expect(s.layers).toHaveLength(2);
  });

  it("hit-tests the top-most visible layer", () => {
    let s = emptyScene(100, 100);
    const back = { ...shapeLayer(s, "rect"), x: 0, y: 0, w: 100, h: 100 };
    const front = { ...shapeLayer(s, "ellipse"), x: 10, y: 10, w: 20, h: 20 };
    s = addLayer(addLayer(s, back), front);
    expect(hitTest(s, 15, 15)?.id).toBe(front.id);
    expect(hitTest(s, 50, 50)?.id).toBe(back.id);
    s = updateLayer(s, front.id, { visible: false });
    expect(hitTest(s, 15, 15)?.id).toBe(back.id);
    expect(hitTest(s, 500, 5)).toBeNull();
  });

  it("covers other aspect ratios keeping the centre", () => {
    const s = emptyScene(1280, 720);
    const t = coverTransform(s, 1080, 1920);
    expect(t.scale).toBeCloseTo(1920 / 720);
    expect(t.dy).toBeCloseTo(0);
    expect(t.dx).toBeLessThan(0);
    expect(coverTransform(s, 640, 360)).toEqual({ scale: 0.5, dx: 0, dy: 0 });
  });

  it("resizes keeping relative positions and lists needed assets", () => {
    let s = emptyScene(1000, 1000);
    s = addLayer(s, { ...textLayer(s), x: 400, y: 400, w: 200, h: 200, size: 100 });
    s = addLayer(s, imageLayer(s, "asset-1", "skin"));
    s = { ...s, background: { color: "#000", assetId: "bg" } };
    const r = resizeScene(s, 500, 1000);
    expect(r.layers[0]).toMatchObject({ x: 200, y: 450, w: 100, h: 100, size: 50 });
    expect(assetIds(s).sort()).toEqual(["asset-1", "bg"]);
  });

  it("parses saved scenes defensively", () => {
    expect(parseScene(JSON.stringify(emptyScene()))?.width).toBe(1280);
    expect(parseScene("{}")).toBeNull();
    expect(parseScene("nope")).toBeNull();
  });
});
