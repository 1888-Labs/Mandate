'use client'

import { useConfig } from 'vocs'

/** 1888 Labs sign-off at the foot of the landing page, in the brand kit's logo. */
export function BuiltBy() {
  const { basePath } = useConfig()
  const base = basePath.endsWith('/') ? basePath : `${basePath}/`
  return (
    <div className="mx-builtby">
      <a href="https://github.com/1888-Labs" aria-label="1888 Labs on GitHub">
        <img className="mx-logo-light" alt="1888 Labs" height={28} src={`${base}1888-labs-on-light.svg`} />
        <img className="mx-logo-dark" alt="1888 Labs" height={28} src={`${base}1888-labs-on-dark.svg`} />
      </a>
      <span>Research. Build. Verify.</span>
    </div>
  )
}
