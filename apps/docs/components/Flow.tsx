'use client'

import { useConfig } from 'vocs'

/** The two-Sprite flowchart, in the light or dark variant to match the theme. */
export function Flow() {
  const { basePath } = useConfig()
  const base = basePath.endsWith('/') ? basePath : `${basePath}/`
  const alt =
    'The agent Sprite sends signed calls to the Mandate Warden Sprite, which verifies, classifies, authorizes and logs each one, forwards allowed calls to the GitHub and Ollama connectors, returns denials with a reason, and checkpoints or restores the agent through the Sprites API.'
  return (
    <div className="mx-flow">
      <img className="mx-logo-light" alt={alt} src={`${base}flow-light.svg`} />
      <img className="mx-logo-dark" alt={alt} src={`${base}flow-dark.svg`} />
    </div>
  )
}
