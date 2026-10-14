import { defineConfig } from "vocs/config"

const repo = "https://github.com/dicethedev/Mandate"

export default defineConfig({
  title: "Mandate",
  titleTemplate: "%s · Mandate",
  description: "Verifiable delegated authorization for AI agents, on Arkiv.",
  // Arkiv's indigo, as in the web app.
  accentColor: "light-dark(#181ea9, #8a8ff0)",
  iconUrl: "/icon.svg",
  rootDir: ".",
  srcDir: ".",
  // A plain static site: hostable anywhere, no server needed.
  renderStrategy: "full-static",
  socials: [{ icon: "github", link: repo }],
  topNav: [
    { text: "Docs", link: "/docs/introduction", match: "/docs" },
    { text: "Arkiv", link: "/arkiv/schema", match: "/arkiv" },
    { text: "GitHub", link: repo },
  ],
  sidebar: [
    {
      text: "Introduction",
      items: [
        { text: "What is Mandate?", link: "/docs/introduction" },
        { text: "Quick start", link: "/docs/quickstart" },
      ],
    },
    {
      text: "Concepts",
      items: [
        { text: "Protocol", link: "/docs/protocol" },
        { text: "Security model", link: "/docs/security" },
        { text: "Architecture", link: "/docs/architecture" },
      ],
    },
    {
      text: "Using Mandate",
      items: [
        { text: "HTTP API", link: "/docs/api" },
        { text: "CLI", link: "/docs/cli" },
        { text: "Rust library", link: "/docs/library" },
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
      text: "Project",
      items: [
        { text: "Development", link: "/docs/development" },
        { text: "Inspiration", link: "/docs/inspiration" },
      ],
    },
  ],
})
