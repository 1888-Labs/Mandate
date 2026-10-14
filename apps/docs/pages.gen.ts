// deno-fmt-ignore-file
// biome-ignore format: generated types do not need formatting
// prettier-ignore
import type { PathsForPages } from 'waku/router'

// prettier-ignore
type Page =
  | { path: '/arkiv/friction'; render: 'static' }
  | { path: '/arkiv/schema'; render: 'static' }
  | { path: '/docs/api'; render: 'static' }
  | { path: '/docs/architecture'; render: 'static' }
  | { path: '/docs/cli'; render: 'static' }
  | { path: '/docs/development'; render: 'static' }
  | { path: '/docs/inspiration'; render: 'static' }
  | { path: '/docs/introduction'; render: 'static' }
  | { path: '/docs/library'; render: 'static' }
  | { path: '/docs/protocol'; render: 'static' }
  | { path: '/docs/quickstart'; render: 'static' }
  | { path: '/docs/security'; render: 'static' }
  | { path: '/'; render: 'static' }

// prettier-ignore
declare module 'waku/router' {
  interface RouteConfig {
    paths: PathsForPages<Page>
  }
  interface CreatePagesConfig {
    pages: Page
  }
}
