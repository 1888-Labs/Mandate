import { defineConfig } from "vocs/config"

const repo = "https://github.com/1888-Labs/Mandate"

// GitHub Pages serves the docs at https://1888-labs.github.io/Mandate/ and the
// live demo UI under /Mandate/demo/. Override with VOCS_BASE_PATH=/ locally.
const env = (globalThis as typeof globalThis & {
  process?: { env?: Record<string, string | undefined> }
}).process?.env
const basePath = (env?.VOCS_BASE_PATH ?? "/Mandate").replace(/\/+$/, "") || "/"
/** Public files need the base path; Vocs doesn't prefix icon or head URLs. */
const asset = (file: string) => (basePath === "/" ? `/${file}` : `${basePath}/${file}`)
const demo = "https://1888-labs.github.io/Mandate/demo/"

export default defineConfig({
  title: "Mandate",
  titleTemplate: "%s · Mandate",
  description: "Verifiable delegated authorization for AI agents, on Arkiv.",
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
    { text: "API", link: "/docs/api" },
    { text: "Self-host", link: "/docs/self-hosting" },
    { text: "Arkiv", link: "/arkiv/schema", match: "/arkiv" },
    { text: "Live demo", link: demo },
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
        { text: "Integrate in 10 minutes", link: "/docs/integrate" },
        { text: "Public test API", link: "/docs/public-api" },
        { text: "CLI", link: "/docs/cli" },
        { text: "Rust library", link: "/docs/library" },
      ],
    },
    {
      text: "Self-hosting",
      items: [
        { text: "Run with Docker", link: "/docs/self-hosting" },
        { text: "Deploy to production", link: "/docs/deployment" },
        { text: "Configuration", link: "/docs/configuration" },
        { text: "Troubleshooting", link: "/docs/troubleshooting" },
      ],
    },
    {
      text: "Reference",
      items: [
        { text: "HTTP API", link: "/docs/api" },
        { text: "Deny codes", link: "/docs/deny-codes" },
        { text: "Protocol", link: "/docs/protocol" },
        { text: "Architecture", link: "/docs/architecture" },
      ],
    },
    {
      text: "Arkiv",
      items: [
        { text: "Schema & queries", link: "/arkiv/schema" },
        { text: "Friction report", link: "/arkiv/friction" },
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
        { text: "Development", link: "/docs/development" },
        { text: "Inspiration", link: "/docs/inspiration" },
      ],
    },
  ],
})
