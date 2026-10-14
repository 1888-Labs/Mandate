import { defineConfig } from "vocs/config"

const repo = "https://github.com/dicethedev/Mandate"

export default defineConfig({
  title: "Mandate",
  titleTemplate: "%s · Mandate",
  description: "Verifiable delegated authorization for AI agents, on Arkiv.",
  // Mandate's indigo, as in the web app.
  accentColor: "light-dark(#181ea9, #7a80ea)",
  iconUrl: "/icon.svg",
  rootDir: ".",
  srcDir: ".",
  // A plain static site: hostable anywhere, no server needed.
  renderStrategy: "full-static",
  socials: [{ icon: "github", link: repo }],
  topNav: [
    { text: "Docs", link: "/docs/introduction", match: "/docs" },
    { text: "Integrate", link: "/docs/integrate" },
    { text: "Arkiv", link: "/arkiv/schema", match: "/arkiv" },
    { text: "GitHub", link: repo },
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
        { text: "CLI", link: "/docs/cli" },
        { text: "Rust library", link: "/docs/library" },
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
