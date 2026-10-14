// Copy the repository's markdown into the docs site, so `docs/` and `arkiv/`
// stay the single source of truth (they also read well on GitHub).
//
//   docs/*.md          → pages/docs/*.md
//   arkiv/schema.md    → pages/arkiv/schema.md
//   arkiv/friction.md  → pages/arkiv/friction.md
//
// Links are rewritten: links between these pages stay inside the site, and
// links to code or other repo files point at GitHub.

import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs"
import { dirname, join, normalize, relative } from "node:path"
import { fileURLToPath } from "node:url"

const site = fileURLToPath(new URL("..", import.meta.url))
const repo = join(site, "..", "..")
const github = "https://github.com/1888-Labs/Mandate"

const sources = [
  ...readdirSync(join(repo, "docs"))
    .filter((f) => f.endsWith(".md"))
    .map((f) => [`docs/${f}`, `docs/${f}`]),
  ["arkiv/schema.md", "arkiv/schema.md"],
  ["arkiv/friction.md", "arkiv/friction.md"],
]
/** Repo path of a synced page → its URL on the site. */
const routes = new Map(sources.map(([from]) => [from, `/${from.replace(/\.md$/, "")}`]))
routes.set("README.md", "/docs/introduction")

function rewrite(target, fromFile) {
  if (/^(https?:|mailto:|#)/.test(target)) return target
  const [path, anchor] = target.split("#")
  const resolved = normalize(join(dirname(fromFile), path)).replaceAll("\\", "/")
  const route = routes.get(resolved)
  if (route) return anchor ? `${route}#${anchor}` : route
  const abs = join(repo, resolved)
  if (!existsSync(abs) || relative(repo, abs).startsWith("..")) return target
  const kind = statSync(abs).isDirectory() ? "tree" : "blob"
  return `${github}/${kind}/main/${resolved}${anchor ? `#${anchor}` : ""}`
}

for (const [from, to] of sources) {
  let text = readFileSync(join(repo, from), "utf8")
  // The "back to README" links make no sense inside the site.
  text = text.replace(/^\[← Back to the README\]\([^)]*\)\n\n?/m, "")
  // Rewrite markdown links, but never inside fenced code blocks.
  text = text
    .split(/(```[\s\S]*?```)/g)
    .map((chunk, i) => (i % 2 ? chunk : chunk.replace(/\]\(([^)\s]+)\)/g, (_, t) => `](${rewrite(t, from)})`)))
    .join("")
  const out = join(site, "pages", to)
  mkdirSync(dirname(out), { recursive: true })
  writeFileSync(out, text)
}
console.log(`synced ${sources.length} pages from the repository`)
