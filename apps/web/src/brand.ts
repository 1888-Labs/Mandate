// Mandate's identity, in one place. The same tokens drive the CSS
// (styles.css), the docs site (apps/docs/pages/_root.css) and the logo.

export const REPO_URL = "https://github.com/1888-Labs/Mandate"
export const DOCS_URL = `${REPO_URL}/tree/main/docs`

export const COLORS = {
  indigo: "#4b52c7", // primary (dark mode); #181ea9 in light mode
  orange: "#fe7446", // signal: the logo's diamond, live activity, the active path
  ink: "#f0ede8", // warm off-white text on dark
  allow: "#2fbf71",
  deny: "#ef4a4a",
} as const
