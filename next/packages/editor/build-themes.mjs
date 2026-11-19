// Builds the editor theme stylesheets (light/dark pair per theme) into the
// given folder. They are swapped at runtime by moltenAPI.setTheme, so they
// must exist as standalone files rather than being bundled.
//   node build-themes.mjs <out dir>
import { build } from "esbuild";
import { mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const outDir = resolve(process.argv[2] ?? join(here, "dist-themes"));

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

await build({
  entryPoints: ["frame", "nord", "classic"].flatMap((name) => [
    join(here, "src", "themes", `${name}-light.css`),
    join(here, "src", "themes", `${name}-dark.css`),
  ]),
  bundle: true,
  outdir: outDir,
  minify: true,
  logLevel: "warning",
  loader: { ".woff": "file", ".woff2": "file", ".ttf": "file", ".svg": "dataurl", ".png": "dataurl" },
  assetNames: "assets/[name]-[hash]",
});
