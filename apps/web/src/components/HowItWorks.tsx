const STEPS = [
  {
    n: "01",
    title: "Grant",
    body: "Alice signs a mandate: FinanceAgent may transfer up to $1,000, to AWS only. It's published on Arkiv from Alice's own wallet.",
  },
  {
    n: "02",
    title: "Delegate, only narrower",
    body: "FinanceAgent hands InvoiceAgent a smaller slice: up to $300. Trying to give more than it holds is rejected.",
  },
  {
    n: "03",
    title: "Check, then revoke",
    body: "Before acting, a tool asks Mandate. ALLOW or DENY, with the reason. Revoke a mandate and everything beneath it stops, instantly.",
  },
]

export function HowItWorks() {
  return (
    <section className="how" aria-label="How Mandate works">
      {STEPS.map((s) => (
        <div className="how-step" key={s.n}>
          <span className="how-n">{s.n}</span>
          <h3>{s.title}</h3>
          <p>{s.body}</p>
        </div>
      ))}
    </section>
  )
}
