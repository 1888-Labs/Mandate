import type { Hex } from "../types"

export type SandboxItem = {
  name: string
  address: Hex | null
  mandateId: Hex | null
  phase: "pending" | "active" | "revoked"
  working: string | null
  error: string | null
}

type Props = {
  items: SandboxItem[]
  creating: boolean
  error: string | null
  maxAmount: number
  onCreate: () => void
  onRevoke: (item: SandboxItem) => void
  onCheck: (agent: Hex) => void
  nameOf: (address: string) => string
}

export function Sandbox({ items, creating, error, maxAmount, onCreate, onRevoke, onCheck, nameOf }: Props) {
  return (
    <section className="card" id="sandbox">
      <h2>Try revocation live</h2>
      <p className="hint">
        FinanceAgent delegates up to <b>${maxAmount} → AWS</b> to a fresh sandbox agent for 2 hours, published to Arkiv.
        Then revoke it and watch it stop working, with no refresh.
      </p>
      <button className="primary" onClick={onCreate} disabled={creating}>
        {creating ? "Publishing…" : "Create a sandbox agent"}
      </button>
      {error && <p className="error">{error}</p>}
      <div className="sandbox">
        {items.map((s, i) => {
          const name = s.address ? nameOf(s.address) : "Sandbox agent"
          return (
            <div className="sandbox-item" key={s.address ?? `pending-${i}`}>
              <div className="edge-top">
                <b>{name}</b>
                {s.phase !== "pending" && (
                  <span className={`status ${s.phase === "revoked" ? "revoked" : "active"}`}>
                    {s.phase === "revoked" ? "REVOKED" : "ACTIVE"}
                  </span>
                )}
              </div>
              <span className="meta">FinanceAgent → {name}: transfer ≤ ${maxAmount} → AWS</span>
              {s.working && (
                <span className="working">
                  <span className="spinner" />
                  {s.working}
                </span>
              )}
              {s.error && <span className="error">{s.error}</span>}
              {s.address && (
                <div className="actions">
                  <button className="small" onClick={() => s.address && onCheck(s.address)}>
                    Check $10 → AWS
                  </button>
                  {s.phase === "active" && (
                    <button className="small danger" disabled={Boolean(s.working)} onClick={() => onRevoke(s)}>
                      Revoke
                    </button>
                  )}
                </div>
              )}
            </div>
          )
        })}
      </div>
    </section>
  )
}
