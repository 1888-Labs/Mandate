import { useEffect, useRef, useState } from "react"

import { API_URL } from "../api"
import type { LiveEvent } from "../types"

export type LiveStatus = "off" | "connecting" | "live" | "reconnecting"

/**
 * Arkiv entity events, pushed from the adapter's WebSocket subscription and
 * relayed by mandate-server as Server-Sent Events. No polling.
 */
export function useLiveEvents(enabled: boolean, onEvent: (e: LiveEvent) => void) {
  const [status, setStatus] = useState<LiveStatus>(enabled ? "connecting" : "off")
  const [events, setEvents] = useState<(LiveEvent & { receivedAt: Date })[]>([])
  const handler = useRef(onEvent)
  handler.current = onEvent

  useEffect(() => {
    if (!enabled) {
      setStatus("off")
      return
    }
    const source = new EventSource(`${API_URL}/v1/events`)
    source.addEventListener("ready", () => setStatus("live"))
    source.addEventListener("arkiv", (msg) => {
      let event: LiveEvent
      try {
        event = JSON.parse((msg as MessageEvent<string>).data) as LiveEvent
      } catch {
        return
      }
      setStatus("live")
      setEvents((list) => [{ ...event, receivedAt: new Date() }, ...list].slice(0, 25))
      handler.current(event)
    })
    source.onerror = () => setStatus("reconnecting")
    return () => source.close()
  }, [enabled])

  return { status, events }
}
