/** "1.234,56" / "1234.56" / "1234" → cents; null when empty or invalid. */
export function parseMoney(input: string): number | null {
  const s = input.trim().replace(/\s/g, "");
  if (!s) return null;
  const lastComma = s.lastIndexOf(",");
  const lastDot = s.lastIndexOf(".");
  const decimalSep = lastComma > lastDot ? "," : ".";
  const [intPart, frac = ""] = s.split(decimalSep).length > 2 ? [s, ""] : s.split(decimalSep);
  // A single separator followed by exactly 3 digits is a thousands separator ("1.500").
  const fracIsThousands =
    frac.length === 3 && !(decimalSep === "," ? s.includes(".") : s.includes(","));
  const cleanInt = (fracIsThousands ? intPart + frac : intPart).replace(/[.,]/g, "");
  const cleanFrac = fracIsThousands ? "" : frac;
  if (!/^\d+$/.test(cleanInt || "0") || !/^\d{0,2}$/.test(cleanFrac)) return null;
  return Number(cleanInt || "0") * 100 + Number(cleanFrac.padEnd(2, "0") || "0");
}

export function formatMoney(
  cents: number | null | undefined,
  currency: string | null | undefined,
  lang: string,
) {
  if (cents == null) return "—";
  try {
    return (cents / 100).toLocaleString(lang, { style: "currency", currency: currency || "EUR" });
  } catch {
    return `${(cents / 100).toFixed(2)} ${currency ?? ""}`.trim();
  }
}
