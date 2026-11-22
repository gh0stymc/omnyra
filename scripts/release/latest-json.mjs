// Builds the Tauri updater manifest (latest.json) from the signed bundles in a folder.
//   node scripts/release/latest-json.mjs <dir> <tag> <download-base-url>
// Linux updates through the AppImage, Windows through the NSIS installer (Tauri's defaults).
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

export function manifest(files, read, tag, base, pubDate = new Date().toISOString()) {
  const find = (re) => files.find((f) => re.test(f));
  const targets = {
    "linux-x86_64": find(/\.AppImage$/),
    "windows-x86_64": find(/-setup\.exe$/),
  };
  const platforms = {};
  for (const [platform, file] of Object.entries(targets)) {
    if (!file) continue;
    const sig = files.find((f) => f === `${file}.sig`);
    if (!sig) throw new Error(`missing signature for ${file}`);
    platforms[platform] = {
      signature: read(sig).trim(),
      url: `${base}/${encodeURIComponent(file)}`,
    };
  }
  if (!Object.keys(platforms).length) throw new Error("no updatable bundles found");
  return { version: tag.replace(/^v/, ""), notes: `Omnyra ${tag}`, pub_date: pubDate, platforms };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [dir, tag, base] = process.argv.slice(2);
  if (!dir || !tag || !base) {
    console.error("usage: latest-json.mjs <dir> <tag> <base-url>");
    process.exit(2);
  }
  const files = readdirSync(dir);
  const out = manifest(files, (f) => readFileSync(join(dir, f), "utf8"), tag, base);
  process.stdout.write(JSON.stringify(out, null, 2) + "\n");
}
