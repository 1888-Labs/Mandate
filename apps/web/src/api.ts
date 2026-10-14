import type { Decision, DemoInfo, Hex, MandateStatus, Resource, SandboxCreated } from "./types"

export class ApiError extends Error {}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { headers: { "content-type": "application/json" }, ...init })
  const body = (await res.json().catch(() => ({}))) as Record<string, unknown>
  // A denial is a successful answer, not an error.
  if (!res.ok && !("decision" in body)) {
    throw new ApiError(typeof body.message === "string" ? body.message : `${res.status} ${res.statusText}`)
  }
  return body as T
}

export const api = {
  demo: () => request<DemoInfo>("/v1/demo"),

  agentMandates: (agent: Hex) =>
    request<{ mandates: MandateStatus[] }>(`/v1/agents/${agent}/mandates?active=true`).then((r) => r.mandates),

  authorize: (body: { subject: Hex; action: string; resource: Resource; amount: number; recipient: string }) =>
    request<Decision>("/v1/authorize", { method: "POST", body: JSON.stringify(body) }),

  createSandbox: () => request<SandboxCreated>("/v1/demo/sandbox", { method: "POST" }),

  revokeSandbox: (mandateId: Hex) => request<unknown>(`/v1/demo/sandbox/${mandateId}/revoke`, { method: "POST" }),
}
