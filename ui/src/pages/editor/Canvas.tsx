import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { Element, PreviewFrame } from "../../lib/api";
import { bbox, type BBox } from "../../lib/theme-utils";
import type { Editor } from "./useEditor";

interface DragState {
  kind: "move" | "resize";
  id: string;
  startX: number;
  startY: number;
  el: Element;
  box: BBox;
}

export function Canvas({ editor, frame, grid, snap }: { editor: Editor; frame: PreviewFrame | null; grid: boolean; snap: number }) {
  const { theme, selectedId, select, updateElement, commit } = editor;
  const wrapRef = useRef<HTMLDivElement>(null);
  const [wrapSize, setWrapSize] = useState({ w: 800, h: 600 });
  const dragRef = useRef<DragState | null>(null);
  const overlayRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      const r = e.contentRect;
      setWrapSize({ w: r.width, h: r.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const scale = Math.max(0.05, Math.min((wrapSize.w - 48) / theme.width, (wrapSize.h - 48) / theme.height));
  const stageW = theme.width * scale;
  const stageH = theme.height * scale;

  // Pointer → theme coordinates.
  const toTheme = useCallback(
    (e: { clientX: number; clientY: number }) => {
      const svg = overlayRef.current;
      if (!svg) return { x: 0, y: 0 };
      const r = svg.getBoundingClientRect();
      return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
    },
    [scale],
  );

  const snapTo = (v: number) => (snap > 1 ? Math.round(v / snap) * snap : Math.round(v));

  const onPointerDown = (e: React.PointerEvent, el: Element, kind: DragState["kind"]) => {
    if (el.locked && kind === "move") {
      select(el.id);
      return;
    }
    e.stopPropagation();
    (e.currentTarget as SVGElement).setPointerCapture(e.pointerId);
    const p = toTheme(e);
    select(el.id);
    dragRef.current = { kind, id: el.id, startX: p.x, startY: p.y, el: structuredClone(el), box: bbox(el) };
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    const p = toTheme(e);
    const dx = p.x - d.startX;
    const dy = p.y - d.startY;
    if (d.kind === "move") {
      updateElement(
        d.id,
        (el) => {
          el.x = snapTo(d.el.x + dx);
          el.y = snapTo(d.el.y + dy);
        },
        "live",
      );
      return;
    }
    // resize from the bottom-right handle
    updateElement(
      d.id,
      (el) => {
        const s = d.el;
        switch (el.type) {
          case "bar":
          case "rect":
          case "image":
            if (s.type === el.type) {
              el.w = Math.max(4, snapTo(s.w + dx));
              el.h = Math.max(4, snapTo(s.h + dy));
            }
            break;
          case "line":
            if (s.type === "line") {
              el.w = snapTo(s.w + dx);
              el.h = snapTo(s.h + dy);
            }
            break;
          case "ellipse":
            if (s.type === "ellipse") {
              el.rx = Math.max(2, snapTo(s.rx + dx / 2));
              el.ry = Math.max(2, snapTo(s.ry + dy / 2));
            }
            break;
          case "gauge":
            if (s.type === "gauge") {
              el.r = Math.max(4, Math.round(s.r + Math.max(dx, dy) / 2));
            }
            break;
          case "text":
            if (s.type === "text") {
              const f = (d.box.h + dy) / d.box.h;
              el.size = Math.max(6, Math.round(s.size * Math.max(0.1, f)));
            }
            break;
        }
      },
      "live",
    );
  };

  const onPointerUp = () => {
    if (dragRef.current) {
      dragRef.current = null;
      commit();
    }
  };

  const boxes = useMemo(() => theme.elements.map((el) => ({ el, box: bbox(el) })), [theme]);
  const selBox = boxes.find((b) => b.el.id === selectedId);
  const handleSize = 10 / scale;

  return (
    <div ref={wrapRef} className={`canvas-wrap ${grid ? "grid-on" : ""}`} onPointerDown={() => select(null)}>
      <div className="canvas-stage" style={{ width: stageW, height: stageH }}>
        {frame ? (
          <img src={frame.data} alt="" draggable={false} />
        ) : (
          <div style={{ width: "100%", height: "100%", display: "grid", placeItems: "center", color: "var(--text-muted)", fontSize: 12 }}>waiting for preview…</div>
        )}
        <svg
          ref={overlayRef}
          className="canvas-overlay"
          viewBox={`0 0 ${theme.width} ${theme.height}`}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
        >
          {boxes.map(({ el, box }) =>
            el.visible ? (
              <rect
                key={el.id}
                className={`hit ${el.locked ? "locked" : ""}`}
                x={box.x}
                y={box.y}
                width={box.w}
                height={box.h}
                transform={el.rotation ? `rotate(${el.rotation} ${el.x} ${el.y})` : undefined}
                onPointerDown={(e) => onPointerDown(e, el, "move")}
              />
            ) : null,
          )}
          {selBox && (
            <g transform={selBox.el.rotation ? `rotate(${selBox.el.rotation} ${selBox.el.x} ${selBox.el.y})` : undefined}>
              <rect className="sel" x={selBox.box.x} y={selBox.box.y} width={selBox.box.w} height={selBox.box.h} />
              {!selBox.el.locked && (
                <rect
                  className="handle"
                  x={selBox.box.x + selBox.box.w - handleSize / 2}
                  y={selBox.box.y + selBox.box.h - handleSize / 2}
                  width={handleSize}
                  height={handleSize}
                  onPointerDown={(e) => onPointerDown(e, selBox.el, "resize")}
                />
              )}
            </g>
          )}
        </svg>
      </div>
    </div>
  );
}
