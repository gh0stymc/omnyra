import { describe, expect, it } from "vitest";
import { BUILT_IN, generate, hashtags, render, toHashtag } from "./templates";

describe("text templates", () => {
  it("renders vars, upper-case vars and cleans empty parts", () => {
    expect(render("{{tema}} | {{juego}}", { tema: "Speedrun", juego: "Minecraft" })).toBe(
      "Speedrun | Minecraft",
    );
    expect(render("{{tema}} | {{juego}}", { tema: "Speedrun" })).toBe("Speedrun");
    expect(render("{{TEMA}} en {{juego}}", { tema: "hardcore", juego: "MC" })).toBe(
      "HARDCORE en MC",
    );
    expect(render("¿{{tema}}? ({{juego}})", { tema: "Qué pasa" })).toBe("¿Qué pasa?");
    expect(render("{{serie}} #{{episodio}}: {{tema}}", { tema: "Fin" })).toBe("Fin");
  });

  it("builds hashtags without accents, in CamelCase, within limits", () => {
    expect(toHashtag("Minecraft")).toBe("#minecraft");
    expect(toHashtag("Speed run fácil")).toBe("#SpeedRunFacil");
    expect(toHashtag("!!")).toBe("");
    const tags = hashtags({ juego: "Minecraft", palabras: "hardcore, mods, minecraft" }, "tiktok");
    expect(tags).toEqual(["#minecraft", "#hardcore", "#mods", "#fyp", "#parati"]);
    expect(hashtags({ palabras: "a1, b2, c3, d4, e5, f6" }, "youtube")).toHaveLength(5);
  });

  it("generates per network and flags long titles", () => {
    const vars = { tema: "x".repeat(120), juego: "MC" };
    const yt = generate(BUILT_IN, "title", "youtube", vars);
    expect(yt.every((r) => r.tooLong)).toBe(true);
    const tw = generate(BUILT_IN, "title", "twitch", { tema: "Hoy", juego: "MC" });
    expect(tw.some((r) => r.text.startsWith("🔴"))).toBe(true);
    expect(tw.some((r) => r.template.platform === "tiktok")).toBe(false);
    const desc = generate(BUILT_IN, "description", "tiktok", { tema: "Truco", juego: "Minecraft" });
    expect(desc[0].text).toBe("Truco 👀 #minecraft #truco #fyp #parati");
  });
});
