import { defineConfig } from "vocs/config"

const repo = "https://github.com/1888-Labs/Mandate"

// GitHub Pages serves the docs at https://1888-labs.github.io/Mandate/.
// Override with VOCS_BASE_PATH=/ locally.
const env = (globalThis as typeof globalThis & {
  process?: { env?: Record<string, string | undefined> }
}).process?.env
const basePath = (env?.VOCS_BASE_PATH ?? "/Mandate").replace(/\/+$/, "") || "/"
/** Public files need the base path; Vocs doesn't prefix icon or head URLs. */
const asset = (file: string) => (basePath === "/" ? `/${file}` : `${basePath}/${file}`)
const dashboard = "https://mandate-warden-b4lwr.sprites.app/"

export default defineConfig({
  title: "Mandate",
  titleTemplate: "%s · Mandate",
  description: "A personal agent on a leash: signed, scoped, revocable authority for AI agents on Fly.io Sprites.",
  basePath,
  baseUrl: "https://1888-labs.github.io",
  // 1888 Labs brand kit: Ink on Paper, Signal orange on Ink. Signal fails
  // contrast as text on Paper, so light mode uses Ink as the accent.
  accentColor: "light-dark(#0F1115, #FF5B22)",
  colorScheme: "light dark",
  iconUrl: asset("favicon.svg"),
  head: {
    // baseUrl is only for canonical/OG URLs; no <base> tag, so local previews work.
    base: false,
    link: [
      { rel: "icon", href: asset("favicon.ico"), sizes: "48x48" },
      { rel: "apple-touch-icon", href: asset("apple-touch-icon.png") },
      { rel: "preconnect", href: "https://fonts.googleapis.com" },
      { rel: "preconnect", href: "https://fonts.gstatic.com", crossorigin: "anonymous" },
      {
        rel: "stylesheet",
        href: "https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;700&family=Space+Grotesk:wght@400;500;600;700&display=swap",
      },
    ],
    meta: { themeColor: "#0F1115" },
  },
  rootDir: ".",
  srcDir: ".",
  // A plain static site: hostable anywhere, no server needed.
  renderStrategy: "full-static",
  socials: [{ icon: "github", link: repo }],
  topNav: [
    { text: "Docs", link: "/docs/introduction", match: "/docs" },
    { text: "Warden", link: "/docs/warden" },
    { text: "Self-host", link: "/docs/self-hosting" },
    { text: "Live dashboard", link: dashboard },
  ],
  sidebar: [
    {
      text: "Getting started",
      items: [
        { text: "What is Mandate?", link: "/docs/introduction" },
        { text: "Concepts in 5 minutes", link: "/docs/concepts" },
        { text: "Quick start", link: "/docs/quickstart" },
        { text: "Use cases", link: "/docs/use-cases" },
      ],
    },
    {
      text: "Guides",
      items: [
        { text: "Connect your own agent", link: "/docs/connect-an-agent" },
        { text: "Run with Docker", link: "/docs/self-hosting" },
        { text: "CLI", link: "/docs/cli" },
        { text: "Rust library", link: "/docs/library" },
      ],
    },
    {
      text: "Reference",
      items: [
        { text: "The Warden", link: "/docs/warden" },
        { text: "Deny codes", link: "/docs/deny-codes" },
        { text: "Protocol", link: "/docs/protocol" },
        { text: "Architecture", link: "/docs/architecture" },
      ],
    },
    {
      text: "Security",
      items: [{ text: "Security & threat model", link: "/docs/security" }],
    },
    {
      text: "Project",
      items: [
        { text: "FAQ", link: "/docs/faq" },
        { text: "Roadmap", link: "/docs/roadmap" },
        { text: "Development", link: "/docs/development" },
        { text: "Inspiration", link: "/docs/inspiration" },
      ],
    },
  ],
})
