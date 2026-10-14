import type { LiveStatus } from "../hooks/useLiveEvents"

const LABEL: Record<LiveStatus, string> = {
  off: "live events off",
  connecting: "connecting…",
  live: "live",
  reconnecting: "reconnecting…",
}

export function Header({ live, pulse }: { live: LiveStatus; pulse: number }) {
  return (
    <header className="topbar">
      <div className="brand">
        <span className="logo" aria-hidden="true">◆</span>
        <span className="name">Mandate</span>
        <span className="tag">authorization for AI agents</span>
      </div>
      <div className="statusbar">
        <span className="chip">Arkiv · Tiramisu</span>
        <span key={pulse} className={`chip live ${live === "live" ? "on" : ""} ${pulse ? "flash" : ""}`}>
          <span className="dot" />
          <span>{LABEL[live]}</span>
        </span>
        <a className="chip link" href="https://github.com/dicethedev/Mandate" target="_blank" rel="noopener">
          GitHub ↗
        </a>
      </div>
    </header>
  )
}
