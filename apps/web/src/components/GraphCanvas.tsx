import "@xyflow/react/dist/style.css"

import {
  Background,
  BackgroundVariant,
  BaseEdge,
  type Edge,
  type EdgeProps,
  EdgeLabelRenderer,
  getBezierPath,
  Handle,
  type Node,
  type NodeProps,
  Position,
  ReactFlow,
} from "@xyflow/react"
import { useMemo } from "react"

import { colorFor, limits, short } from "../format"
import { layout, NODE_WIDTH, pairKey, pathPairs } from "../graph/layout"
import type { Hex, MandateStatus } from "../types"

type PrincipalData = { name: string; address: Hex; root: boolean; onPath: boolean }
type MandateData = {
  entry: MandateStatus
  status: "active" | "revoked" | "expired" | "denied"
  onPath: boolean
  cut: boolean
  selected: boolean
  onSelect: (entry: MandateStatus) => void
}
type PrincipalNode = Node<PrincipalData, "principal">
type MandateEdge = Edge<MandateData, "mandate">

function statusClass(status: string): MandateData["status"] {
  if (status === "ACTIVE") return "active"
  if (status === "REVOKED" || status === "ANCESTOR_REVOKED") return "revoked"
  if (status === "EXPIRED") return "expired"
  return "denied"
}

function Principal({ data }: NodeProps<PrincipalNode>) {
  return (
    <div className={`principal ${data.onPath ? "on-path" : ""}`} style={{ width: NODE_WIDTH }}>
      <Handle type="target" position={Position.Top} />
      <span className="avatar" style={{ background: colorFor(data.name) }}>
        {data.name.slice(0, 1).toUpperCase()}
      </span>
      <span className="who">
        <b>{data.name}</b>
        <span className="mono small">{data.root ? "trust anchor" : short(data.address)}</span>
      </span>
      <Handle type="source" position={Position.Bottom} />
    </div>
  )
}

function Mandate(props: EdgeProps<MandateEdge>) {
  const { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data } = props
  const [path] = getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition })
  // Just above the agent it points to: siblings sit a column apart, so labels never collide.
  const labelX = targetX
  const labelY = targetY - 30
  if (!data) return null
  const m = data.entry.mandate
  const cls = [data.status, data.onPath ? "on-path" : "", data.cut ? "cut" : "", data.selected ? "selected" : ""].join(" ")
  return (
    <>
      <BaseEdge id={id} path={path} className={`mandate-edge ${cls}`} />
      <EdgeLabelRenderer>
        <button
          className={`edge-label nodrag nopan ${cls}`}
          style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
          onClick={() => data.onSelect(data.entry)}
          title="Inspect this mandate"
        >
          <span className={`edge-dot ${data.status}`} aria-hidden="true" />
          <span className="edge-limits">{limits(m.constraints)}</span>
          {data.status !== "active" && <span className={`status ${data.status}`}>{data.cut ? "✂ " : ""}{data.entry.status}</span>}
        </button>
      </EdgeLabelRenderer>
    </>
  )
}

const nodeTypes = { principal: Principal }
const edgeTypes = { mandate: Mandate }

type Props = {
  edges: MandateStatus[]
  nameOf: (address: string) => string
  highlight: Hex[]
  cutMandate: Hex | null
  selected: Hex | null
  onSelect: (entry: MandateStatus) => void
}

export function GraphCanvas({ edges, nameOf, highlight, cutMandate, selected, onSelect }: Props) {
  const onPath = useMemo(() => pathPairs(highlight), [highlight])
  const pathNodes = useMemo(() => new Set(highlight.map((a) => a.toLowerCase())), [highlight])

  const { nodes, flowEdges } = useMemo(() => {
    const placed = layout(edges)
    const subjects = new Set(edges.map((e) => e.mandate.subject))
    const nodes: PrincipalNode[] = placed.map((p) => ({
      id: p.address,
      type: "principal",
      position: { x: p.x, y: p.y },
      data: { name: nameOf(p.address), address: p.address, root: !subjects.has(p.address), onPath: pathNodes.has(p.address.toLowerCase()) },
    }))
    const flowEdges: MandateEdge[] = edges.map((e) => ({
      id: e.mandate.id,
      type: "mandate",
      source: e.mandate.issuer,
      target: e.mandate.subject,
      data: {
        entry: e,
        status: statusClass(e.status),
        onPath: onPath.has(pairKey(e.mandate.issuer, e.mandate.subject)),
        cut: e.mandate.id === cutMandate,
        selected: e.mandate.id === selected,
        onSelect,
      },
    }))
    return { nodes, flowEdges }
  }, [edges, nameOf, onPath, pathNodes, cutMandate, selected, onSelect])

  // Re-fit the view when the set of principals changes.
  const shape = nodes.map((n) => n.id).join(",")
  // On phones, stay readable and let people drag, rather than shrinking to fit.
  const narrow = typeof window !== "undefined" && window.matchMedia("(max-width: 720px)").matches

  return (
    <div className="graph-canvas">
      <ReactFlow
        key={shape}
        nodes={nodes}
        edges={flowEdges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        fitViewOptions={{ padding: 0.12, maxZoom: 1.05, minZoom: narrow ? 0.62 : 0.35 }}
        minZoom={0.35}
        maxZoom={1.4}
        nodesConnectable={false}
        elementsSelectable={false}
        zoomOnScroll={false}
        preventScrolling={false}
        colorMode="system"
      >
        <Background variant={BackgroundVariant.Dots} gap={18} size={1} />
      </ReactFlow>
      {narrow && <span className="drag-hint">drag to explore</span>}
    </div>
  )
}
