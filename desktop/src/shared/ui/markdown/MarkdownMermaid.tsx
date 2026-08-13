import * as React from "react";

import { useEscapeKey } from "@/shared/hooks/useEscapeKey";

/**
 * Mermaid fences in the desktop markdown pipeline.
 *
 * Renders fenced `mermaid` blocks as a diagram with lightbox pan/zoom.
 * Flowchart-TD SVG subset plus a code-fence fallback — never a broken box.
 */
export function MarkdownMermaid({ source }: { source: string }) {
  const [svg, setSvg] = React.useState<string | null>(null);
  const [failed, setFailed] = React.useState(false);
  const [open, setOpen] = React.useState(false);
  const [zoom, setZoom] = React.useState(1);
  const [pan, setPan] = React.useState({ x: 0, y: 0 });
  const drag = React.useRef<{ x: number; y: number } | null>(null);
  const closeLightbox = React.useCallback(() => setOpen(false), []);
  useEscapeKey(closeLightbox, open);

  React.useEffect(() => {
    const fallback = renderFlowchartSvg(source);
    if (fallback) {
      setFailed(false);
      setSvg(fallback);
    } else {
      setSvg(null);
      setFailed(true);
    }
  }, [source]);

  if (failed) {
    return (
      <pre
        className="overflow-auto rounded-md border border-border bg-muted/40 p-3 font-mono text-sm"
        data-testid="wiki-mermaid-fallback"
      >
        <code>{source}</code>
      </pre>
    );
  }

  if (!svg) {
    return (
      <div
        className="rounded-md border border-dashed border-border p-4 text-2xs text-muted-foreground"
        data-testid="wiki-mermaid-pending"
      >
        Rendering diagram…
      </div>
    );
  }

  return (
    <>
      <button
        className="language-mermaid block w-full overflow-auto rounded-md border border-border bg-background p-2 text-left"
        data-testid="wiki-mermaid"
        onClick={() => {
          setOpen(true);
          setZoom(1);
          setPan({ x: 0, y: 0 });
        }}
        type="button"
      >
        <div
          className="pointer-events-none mx-auto max-w-full"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: mermaid SVG is generated locally from fenced source
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      </button>
      {open ? (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
          data-testid="wiki-mermaid-lightbox"
          onClick={() => setOpen(false)}
          onKeyDown={(event) => {
            if (event.key === "Escape") setOpen(false);
          }}
          role="dialog"
          aria-modal="true"
          aria-label="Diagram"
          tabIndex={-1}
        >
          <div
            className="max-h-[90vh] max-w-[90vw] cursor-grab overflow-hidden rounded-lg bg-background p-4"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => event.stopPropagation()}
            role="document"
            onPointerDown={(event) => {
              drag.current = {
                x: event.clientX - pan.x,
                y: event.clientY - pan.y,
              };
            }}
            onPointerMove={(event) => {
              if (!drag.current) return;
              setPan({
                x: event.clientX - drag.current.x,
                y: event.clientY - drag.current.y,
              });
            }}
            onPointerUp={() => {
              drag.current = null;
            }}
            onWheel={(event) => {
              event.preventDefault();
              setZoom((current) =>
                Math.min(
                  4,
                  Math.max(0.5, current + (event.deltaY < 0 ? 0.15 : -0.15)),
                ),
              );
            }}
          >
            <div
              style={{
                transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
                transformOrigin: "center center",
              }}
              // biome-ignore lint/security/noDangerouslySetInnerHtml: mermaid SVG is generated locally from fenced source
              dangerouslySetInnerHTML={{ __html: svg }}
            />
          </div>
        </div>
      ) : null}
    </>
  );
}

function renderFlowchartSvg(source: string): string | null {
  const lines = source
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines[0]?.toLowerCase().startsWith("flowchart")) return null;
  const nodes = new Map<string, string>();
  const edges: Array<{ from: string; to: string }> = [];
  const nodeRe = /([A-Za-z][\w-]*)(?:\[([^\]]+)\])?/g;
  for (const line of lines.slice(1)) {
    const arrow = line.split("-->");
    if (arrow.length < 2) continue;
    const left = [...arrow[0].matchAll(nodeRe)];
    const right = [...arrow.slice(1).join("-->").matchAll(nodeRe)];
    const from = left[0];
    const to = right[0];
    if (!from || !to) continue;
    nodes.set(from[1], from[2] ?? from[1]);
    nodes.set(to[1], to[2] ?? to[1]);
    edges.push({ from: from[1], to: to[1] });
  }
  if (nodes.size === 0) return null;
  const ids = [...nodes.keys()];
  const width = Math.max(240, ids.length * 140);
  const height = 160;
  const boxes = ids
    .map((id, index) => {
      const x = 20 + index * 140;
      const label = escapeXml(nodes.get(id) ?? id);
      return `<g><rect x="${x}" y="48" width="120" height="48" rx="8" fill="#f4f4f5" stroke="#a1a1aa"/><text x="${x + 60}" y="76" text-anchor="middle" font-size="12" fill="#18181b">${label}</text></g>`;
    })
    .join("");
  const linesSvg = edges
    .map((edge) => {
      const from = ids.indexOf(edge.from);
      const to = ids.indexOf(edge.to);
      if (from < 0 || to < 0) return "";
      const x1 = 20 + from * 140 + 120;
      const x2 = 20 + to * 140;
      return `<line x1="${x1}" y1="72" x2="${x2}" y2="72" stroke="#71717a" marker-end="url(#arrow)"/>`;
    })
    .join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto"><path d="M0,0 L0,6 L8,3 z" fill="#71717a"/></marker></defs>${linesSvg}${boxes}</svg>`;
}

function escapeXml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
