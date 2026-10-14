import { DOCS_URL } from "../brand"

type Props = { mandates: number; revoked: number; network: string }

export function Hero({ mandates, revoked, network }: Props) {
  return (
    <section className="hero" id="top">
      <div className="glow" aria-hidden="true" />
      <div className="hero-inner">
        <span className="eyebrow">
          <span className="eyebrow-dot" /> Live on Arkiv · {network}
        </span>
        <h1>
          The prompt is not the security boundary.
          <br />
          <span className="gradient-text">Mandate is.</span>
        </h1>
        <p className="lede">
          Give an AI agent signed, narrowly scoped authority. Let it delegate only a smaller slice. Revoke it any time.
          Every action gets an explained <b>ALLOW</b> or <b>DENY</b>, decided live from Arkiv.
        </p>
        <div className="cta">
          <a className="button primary" href="#check">
            Try a check
          </a>
          <a className="button" href="#sandbox">
            Revoke something live
          </a>
          <a className="button ghost" href={DOCS_URL} target="_blank" rel="noopener">
            Read the docs →
          </a>
        </div>
        <dl className="stats">
          <div>
            <dt>Mandates on Arkiv</dt>
            <dd>{mandates || "–"}</dd>
          </div>
          <div>
            <dt>Revoked</dt>
            <dd>{revoked}</dd>
          </div>
          <div>
            <dt>Decided</dt>
            <dd>live</dd>
          </div>
        </dl>
      </div>
    </section>
  )
}
