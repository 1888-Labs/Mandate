import { DOCS_URL, REPO_URL } from "../brand"
import type { LiveStatus } from "../hooks/useLiveEvents"

const LABEL: Record<LiveStatus, string> = {
  off: "live events off",
  connecting: "connecting",
  live: "live",
  reconnecting: "reconnecting",
}

const SECTIONS = [
  ["graph", "Graph"],
  ["check", "Check"],
  ["sandbox", "Sandbox"],
  ["activity", "Activity"],
] as const

export function Header({ live, pulse }: { live: LiveStatus; pulse: number }) {
  return (
    <header className="topbar">
      <a className="brand" href="#top" aria-label="Mandate home">
        <svg className="mark" viewBox="0 0 16 16" aria-hidden="true">
          <path d="M8 1l7 7-7 7-7-7z" />
        </svg>
        <span className="wordmark">MANDATE</span>
      </a>
      <nav className="nav" aria-label="Sections">
        {SECTIONS.map(([id, label]) => (
          <a key={id} href={`#${id}`}>
            {label}
          </a>
        ))}
      </nav>
      <div className="top-actions">
        <span key={pulse} className={`chip live ${live === "live" ? "on" : ""} ${pulse ? "flash" : ""}`} title="Arkiv entity events over WebSocket">
          <span className="dot" />
          {LABEL[live]}
        </span>
        <a className="chip link" href={DOCS_URL} target="_blank" rel="noopener">
          Docs
        </a>
        <a className="chip link" href={REPO_URL} target="_blank" rel="noopener">
          GitHub ↗
        </a>
      </div>
    </header>
  )
}
