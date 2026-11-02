import { ChevronDown, ChevronUp, Copy, Eye, EyeOff, Lock, LockOpen, Trash2 } from "lucide-react";

import { elementLabel, newId } from "../../lib/theme-utils";
import type { Editor } from "./useEditor";

/** Element list, top-most first. */
export function Layers({ editor }: { editor: Editor }) {
  const { theme, selectedId, select, patch } = editor;
  const items = [...theme.elements].reverse();

  const move = (id: string, dir: 1 | -1) =>
    patch((t) => {
      const i = t.elements.findIndex((e) => e.id === id);
      const j = i + dir;
      if (i < 0 || j < 0 || j >= t.elements.length) return;
      [t.elements[i], t.elements[j]] = [t.elements[j], t.elements[i]];
    });

  if (items.length === 0) return <p className="hint">No elements yet — use “Add” in the toolbar.</p>;

  return (
    <div className="col" style={{ gap: 2 }}>
      {items.map((el) => (
        <div key={el.id} className={`layer ${el.id === selectedId ? "selected" : ""}`} onClick={() => select(el.id)} role="button" tabIndex={0} onKeyDown={(e) => e.key === "Enter" && select(el.id)}>
          <span className="layer-type">{el.type}</span>
          <span className="layer-name" style={{ opacity: el.visible ? 1 : 0.5 }}>
            {elementLabel(el)}
          </span>
          <button className="mini" aria-label={el.visible ? "Hide" : "Show"} onClick={(e) => (e.stopPropagation(), editor.updateElement(el.id, (x) => (x.visible = !x.visible)))}>
            {el.visible ? <Eye size={13} /> : <EyeOff size={13} />}
          </button>
          <button className="mini" aria-label={el.locked ? "Unlock" : "Lock"} onClick={(e) => (e.stopPropagation(), editor.updateElement(el.id, (x) => (x.locked = !x.locked)))}>
            {el.locked ? <Lock size={13} /> : <LockOpen size={13} />}
          </button>
          {el.id === selectedId && (
            <>
              <button className="mini" aria-label="Bring forward" onClick={(e) => (e.stopPropagation(), move(el.id, 1))}>
                <ChevronUp size={13} />
              </button>
              <button className="mini" aria-label="Send backward" onClick={(e) => (e.stopPropagation(), move(el.id, -1))}>
                <ChevronDown size={13} />
              </button>
              <button
                className="mini"
                aria-label="Duplicate"
                onClick={(e) => {
                  e.stopPropagation();
                  patch((t) => {
                    const i = t.elements.findIndex((x) => x.id === el.id);
                    const copy = structuredClone(t.elements[i]);
                    copy.id = newId(copy.type);
                    copy.x += 16;
                    copy.y += 16;
                    t.elements.splice(i + 1, 0, copy);
                    select(copy.id);
                  });
                }}
              >
                <Copy size={13} />
              </button>
              <button
                className="mini"
                aria-label="Delete"
                onClick={(e) => {
                  e.stopPropagation();
                  patch((t) => {
                    t.elements = t.elements.filter((x) => x.id !== el.id);
                  });
                  select(null);
                }}
              >
                <Trash2 size={13} />
              </button>
            </>
          )}
        </div>
      ))}
    </div>
  );
}
