import type { Toast } from "../hooks/useToasts"

export function Toaster({ toasts, onDismiss }: { toasts: Toast[]; onDismiss: (id: number) => void }) {
  return (
    <div className="toaster" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.tone}`}>
          <span className="toast-icon" aria-hidden="true">{t.tone === "ok" ? "✓" : t.tone === "bad" ? "✕" : "◆"}</span>
          <div className="toast-body">
            <b>{t.title}</b>
            {t.detail && <span>{t.detail}</span>}
            {t.href && (
              <a href={t.href} target="_blank" rel="noopener">
                View on Arkiv ↗
              </a>
            )}
          </div>
          <button className="toast-close" onClick={() => onDismiss(t.id)} aria-label="Dismiss">
            ×
          </button>
        </div>
      ))}
    </div>
  )
}
