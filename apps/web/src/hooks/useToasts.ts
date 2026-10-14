import { useCallback, useState } from "react"

export type Toast = { id: number; tone: "ok" | "bad" | "info"; title: string; detail?: string; href?: string }

/** Short-lived notifications for actions that land on Arkiv. */
export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([])
  const push = useCallback((t: Omit<Toast, "id">) => {
    const id = Date.now() + Math.random()
    setToasts((list) => [...list.slice(-3), { ...t, id }])
    setTimeout(() => setToasts((list) => list.filter((x) => x.id !== id)), 6000)
  }, [])
  const dismiss = useCallback((id: number) => setToasts((list) => list.filter((x) => x.id !== id)), [])
  return { toasts, push, dismiss }
}
