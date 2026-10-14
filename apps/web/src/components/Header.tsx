import { DOCS_URL, REPO_URL } from "../brand"
import type { LiveStatus } from "../hooks/useLiveEvents"

const LABEL: Record<LiveStatus, string> = {
  live: "Live",
  connecting: "Connecting",
  reconnecting: "Reconnecting",
  off: "Offline",
}

export function Header({ live }: { live: LiveStatus }) {
  return (
    <header className="header">
      <span className="wordmark">
        <span className="diamond" aria-hidden="true" /> Mandate
      </span>
      <nav>
        <span className={`live is-${live}`} title="Arkiv entity events, pushed to this page as they land">
          <span className="dot" aria-hidden="true" />
          {LABEL[live]}
        </span>
        <a href={DOCS_URL} target="_blank" rel="noopener">
          Docs
        </a>
        <a href={REPO_URL} target="_blank" rel="noopener">
          GitHub
        </a>
      </nav>
    </header>
  )
}
