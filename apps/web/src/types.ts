// Shapes returned by the Mandate HTTP API (see docs/api.md).

export type Hex = `0x${string}`

export type Resource = { kind: string; id: string }

export type Constraints = {
  maxAmount: number | null
  allowedRecipients: string[] | null
}

export type Mandate = {
  id: Hex
  issuer: Hex
  subject: Hex
  action: string
  resource: Resource
  constraints: Constraints
  parent: Hex | null
  delegable: boolean
  issuedAt: number
  expiresAt: number
  nonce: number
  signature: Hex
}

/**
 * Where a document lives on Arkiv: its immutable `$creator`, and its `$owner`
 * (the burn address once nobody may change or delete it).
 */
export type ArkivRef = { entityKey: Hex | null; creator: Hex | null; owner?: Hex | null }

/** A mandate with its current status: `ACTIVE` or a deny code. */
export type MandateStatus = {
  mandate: Mandate
  status: string
  arkiv: ArkivRef | null
}

export type Allow = {
  decision: "ALLOW"
  mandateId: Hex
  rootPrincipal: Hex
  delegationPath: Hex[]
  chain: Hex[]
  effectiveConstraints: Constraints
}

export type Deny = {
  decision: "DENY"
  reason: string
  message: string
  mandateId?: Hex
  delegationPath: Hex[]
  requestedAmount?: number | null
  maximumAmount?: number
  requestedRecipient?: string | null
  allowedRecipients?: string[]
  revokedMandateId?: Hex
  revocationId?: Hex
  expiredMandateId?: Hex
}

export type Decision = Allow | Deny

export type Principal = { name: string; address: Hex }

export type DemoInfo = {
  resource: Resource
  principals: Principal[]
  sandboxAgents: Principal[]
  backend: string
  liveEvents: boolean
  explorer: string
  sandbox: { maxAmount: number; lifetimeSeconds: number }
}

export type SandboxCreated = {
  agent: Principal
  mandate: Mandate
}

/** A live Arkiv entity event, relayed by the adapter. */
export type LiveEvent = {
  kind: "mandate" | "revocation"
  action: "created" | "deleted"
  entityKey: Hex
  creator: Hex
  mandateId: Hex | null
  txHash: Hex
  blockNumber: string
}

export type Revocation = {
  id: Hex
  mandateId: Hex
  issuer: Hex
  revokedAt: number
  reason: string | null
  signature: Hex
}

/** `GET /v1/mandates/{id}/chain` */
export type Chain = {
  mandateId: Hex
  valid: boolean
  links: Mandate[]
  arkiv: Record<string, ArkivRef>
  revocations: { revocation: Revocation; arkiv: ArkivRef | null }[]
  delegationPath: Hex[]
  denial?: Deny
}

