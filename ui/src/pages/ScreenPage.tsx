import { ArrowLeft, Check, Copy, Grid3X3, MonitorUp, Plus, Redo2, Save, Trash2, Undo2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Modal, useToast } from "../components/ui";
import { api, errText, type ElementType, type EngineStatus, type Theme, type ThemeInfo } from "../lib/api";
import { useConfig, usePreview } from "../lib/hooks";
import { blankTheme, makeElement, newId, normalizeTheme, TYPE_LABEL } from "../lib/theme-utils";
import { Canvas } from "./editor/Canvas";
import { Inspector } from "./editor/Inspector";
import { Layers } from "./editor/Layers";
import { useEditor } from "./editor/useEditor";

const ADD_TYPES: ElementType[] = ["text", "bar", "gauge", "rect", "ellipse", "line", "image"];

export function ScreenPage({ status }: { status: EngineStatus }) {
  const [editing, setEditing] = useState<{ id: string | null; theme: Theme } | null>(null);
  if (editing) {
    return <ThemeEditor initialId={editing.id} initial={editing.theme} status={status} onExit={() => setEditing(null)} />;
  }
  return <ThemeGallery status={status} onEdit={(id, theme) => setEditing({ id, theme })} />;
}

// ── Gallery ─────────────────────────────────────────────────────────────────

function ThemeGallery({ status, onEdit }: { status: EngineStatus; onEdit: (id: string | null, theme: Theme) => void }) {
  const toast = useToast();
  const [themes, setThemes] = useState<ThemeInfo[]>([]);
  const [config] = useConfig();
  const [confirmDelete, setConfirmDelete] = useState<ThemeInfo | null>(null);
  const [thumbs, setThumbs] = useState<Record<string, string>>({});

  const refresh = useCallback(
    () =>
      api
        .themes()
        .then((list) => {
          setThemes(list);
          list.forEach((t) => api.thumbnail(t.id, 320).then((d) => setThumbs((m) => ({ ...m, [t.id]: d }))).catch(() => {}));
        })
        .catch((e) => toast.error(errText(e))),
    [toast],
  );
  useEffect(() => {
    refresh();
  }, [refresh]);

  const active = config?.screen.active_theme ?? null;
  const screenW = status.screen?.width ?? 720;
  const screenH = status.screen?.height ?? 720;

  const edit = async (t: ThemeInfo) => {
    try {
      if (t.bundled) {
        // Bundled themes are read-only: edit a copy in "My themes".
        const id = await api.duplicateTheme(t.id, t.name);
        onEdit(id, normalizeTheme(await api.loadTheme(id)));
      } else {
        onEdit(t.id, normalizeTheme(await api.loadTheme(t.id)));
      }
    } catch (e) {
      toast.error(errText(e));
    }
  };

  const Card = ({ t }: { t: ThemeInfo }) => (
    <div className={`theme-card ${t.id === active ? "selected" : ""}`}>
      <div className="tc-thumb" style={{ aspectRatio: `${t.width} / ${t.height}` }}>
        {thumbs[t.id] ? <img src={thumbs[t.id]} alt="" /> : <span className="dim small">…</span>}
      </div>
      <div className="tc-name">
        {t.name}
        {t.id === active && <span className="badge accent">on screen</span>}
      </div>
      <div className="tc-meta">
        {t.width}×{t.height} · {t.elements} elements{t.bundled ? " · bundled" : ""}
      </div>
      <div className="row-wrap" style={{ marginTop: 4 }}>
        <button className="btn primary sm" disabled={t.id === active} onClick={() => api.applyTheme(t.id).then(() => (toast.success(`${t.name} is on the screen`), refresh())).catch((e) => toast.error(errText(e)))}>
          <MonitorUp size={13} /> Apply
        </button>
        <button className="btn sm" onClick={() => edit(t)}>
          {t.bundled ? "Edit a copy" : "Edit"}
        </button>
        <button className="btn ghost icon sm" aria-label="Duplicate" onClick={() => api.duplicateTheme(t.id).then(refresh).catch((e) => toast.error(errText(e)))}>
          <Copy size={13} />
        </button>
        {!t.bundled && (
          <button className="btn ghost icon sm danger" aria-label="Delete" onClick={() => setConfirmDelete(t)}>
            <Trash2 size={13} />
          </button>
        )}
      </div>
    </div>
  );

  const bundled = themes.filter((t) => t.bundled);
  const mine = themes.filter((t) => !t.bundled);

  return (
    <div className="page stack">
      <div className="page-head">
        <div>
          <div className="eyebrow">// screen</div>
          <h1 className="page-title">Themes</h1>
          <p className="page-sub">
            What the {status.screen ? `${status.screen.width}×${status.screen.height}` : ""} screen shows. Edit live: every change is sent to the screen as you make it.
          </p>
        </div>
        <button className="btn primary" onClick={() => onEdit(null, blankTheme(screenW, screenH))}>
          <Plus size={15} /> New theme
        </button>
      </div>

      <div>
        <div className="eyebrow" style={{ marginBottom: 10 }}>
          // my themes
        </div>
        {mine.length === 0 ? (
          <div className="empty">
            <h3>No themes of your own yet</h3>
            <span>Start from a bundled theme (“Edit a copy”) or create a new one.</span>
          </div>
        ) : (
          <div className="grid grid-auto">
            {mine.map((t) => (
              <Card key={t.id} t={t} />
            ))}
          </div>
        )}
      </div>
      <div>
        <div className="eyebrow" style={{ marginBottom: 10 }}>
          // bundled
        </div>
        <div className="grid grid-auto">
          {bundled.map((t) => (
            <Card key={t.id} t={t} />
          ))}
        </div>
      </div>

      {confirmDelete && (
        <Modal
          title={`Delete “${confirmDelete.name}”?`}
          onClose={() => setConfirmDelete(null)}
          footer={
            <>
              <button className="btn ghost" onClick={() => setConfirmDelete(null)}>
                Cancel
              </button>
              <button
                className="btn danger"
                onClick={() =>
                  api
                    .deleteTheme(confirmDelete.id)
                    .then(() => {
                      toast.success("Deleted");
                      setConfirmDelete(null);
                      refresh();
                    })
                    .catch((e) => toast.error(errText(e)))
                }
              >
                Delete
              </button>
            </>
          }
        >
          <p className="small muted">The file is removed from your themes folder. This cannot be undone.</p>
        </Modal>
      )}
    </div>
  );
}

// ── Editor ──────────────────────────────────────────────────────────────────

function ThemeEditor({ initialId, initial, status, onExit }: { initialId: string | null; initial: Theme; status: EngineStatus; onExit: () => void }) {
  const toast = useToast();
  const editor = useEditor(initial);
  const { theme, selected, select, patch, undo, redo, canUndo, canRedo } = editor;
  const [themeId, setThemeId] = useState<string | null>(initialId);
  const [fonts, setFonts] = useState<string[]>([]);
  const [grid, setGrid] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [config] = useConfig();
  const frame = usePreview(true);
  const lastSent = useRef<string>("");

  useEffect(() => {
    api.fonts().then(setFonts).catch(() => {});
  }, []);

  // Live push to the engine (debounced); the screen and the preview follow.
  useEffect(() => {
    const t = setTimeout(() => {
      const json = JSON.stringify(theme);
      if (json === lastSent.current) return;
      lastSent.current = json;
      api.previewTheme(theme).catch(() => {});
    }, 90);
    return () => clearTimeout(t);
  }, [theme]);

  // Leaving the editor: put the active theme back on the screen.
  useEffect(() => {
    return () => {
      const active = config?.screen.active_theme;
      if (active) api.applyTheme(active).catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = useCallback(async (): Promise<string | null> => {
    try {
      let id = themeId;
      if (id) await api.saveTheme(id, theme);
      else {
        id = await api.createTheme(theme);
        setThemeId(id);
      }
      editor.markClean();
      toast.success("Saved");
      return id;
    } catch (e) {
      toast.error(errText(e));
      return null;
    }
  }, [themeId, theme, editor, toast]);

  const saveAndApply = async () => {
    const id = await save();
    if (!id) return;
    try {
      await api.applyTheme(id);
      toast.success("On the screen");
    } catch (e) {
      toast.error(errText(e));
    }
  };

  // Keyboard shortcuts (ignored while typing in a field).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT")) return;
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "z" && !e.shiftKey) (e.preventDefault(), undo());
      else if (mod && (e.key.toLowerCase() === "y" || (e.key.toLowerCase() === "z" && e.shiftKey))) (e.preventDefault(), redo());
      else if (mod && e.key.toLowerCase() === "s") (e.preventDefault(), save());
      else if (mod && e.key.toLowerCase() === "d" && selected) {
        e.preventDefault();
        patch((th) => {
          const i = th.elements.findIndex((x) => x.id === selected.id);
          const copy = structuredClone(th.elements[i]);
          copy.id = newId(copy.type);
          copy.x += 16;
          copy.y += 16;
          th.elements.splice(i + 1, 0, copy);
          select(copy.id);
        });
      } else if ((e.key === "Delete" || e.key === "Backspace") && selected) {
        e.preventDefault();
        patch((th) => {
          th.elements = th.elements.filter((x) => x.id !== selected.id);
        });
        select(null);
      } else if (e.key === "Escape") select(null);
      else if (e.key.startsWith("Arrow") && selected && !selected.locked) {
        e.preventDefault();
        const d = e.shiftKey ? 10 : 1;
        const dx = e.key === "ArrowLeft" ? -d : e.key === "ArrowRight" ? d : 0;
        const dy = e.key === "ArrowUp" ? -d : e.key === "ArrowDown" ? d : 0;
        editor.updateElement(selected.id, (x) => {
          x.x += dx;
          x.y += dy;
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo, save, selected, patch, select, editor]);

  const add = (type: ElementType) => {
    const el = makeElement(type, theme.width, theme.height);
    patch((t) => {
      t.elements.push(el);
    });
    select(el.id);
    setAddOpen(false);
  };

  const sizeMismatch = status.screen && (status.screen.width !== theme.width || status.screen.height !== theme.height);

  return (
    <div className="editor">
      <aside className="editor-side">
        <div className="editor-side-head">
          <button className="btn ghost sm" onClick={onExit}>
            <ArrowLeft size={14} /> Themes
          </button>
          {editor.dirty && <span className="badge accent">unsaved</span>}
        </div>
        <div className="editor-side-body">
          <div className="eyebrow">// layers</div>
          <Layers editor={editor} />
          <div className="spacer" />
          <div className="hint">
            <span className="kbd">Ctrl+Z</span> undo · <span className="kbd">Del</span> remove · <span className="kbd">←↑↓→</span> nudge · <span className="kbd">Shift</span> ×10 · <span className="kbd">Ctrl+D</span> duplicate
          </div>
        </div>
      </aside>

      <section className="editor-center">
        <div className="editor-toolbar">
          <div style={{ position: "relative" }}>
            <button className="btn primary" onClick={() => setAddOpen((o) => !o)} aria-expanded={addOpen} aria-haspopup="menu">
              <Plus size={15} /> Add
            </button>
            {addOpen && (
              <div className="menu" role="menu" style={{ top: "calc(100% + 6px)", left: 0 }} onMouseLeave={() => setAddOpen(false)}>
                {ADD_TYPES.map((t) => (
                  <button key={t} role="menuitem" onClick={() => add(t)}>
                    {TYPE_LABEL[t]}
                  </button>
                ))}
              </div>
            )}
          </div>
          <span className="sep" />
          <button className="btn ghost icon" aria-label="Undo" disabled={!canUndo} onClick={undo}>
            <Undo2 size={16} />
          </button>
          <button className="btn ghost icon" aria-label="Redo" disabled={!canRedo} onClick={redo}>
            <Redo2 size={16} />
          </button>
          <button className={`btn ghost icon ${grid ? "active" : ""}`} aria-label="Toggle grid snapping" aria-pressed={grid} onClick={() => setGrid((g) => !g)} style={grid ? { color: "var(--accent)" } : undefined}>
            <Grid3X3 size={16} />
          </button>
          <span className="spacer" />
          <button className="btn" onClick={save} disabled={!editor.dirty && !!themeId} title="Save (Ctrl+S)">
            <Save size={15} /> Save
          </button>
          <button className="btn primary" onClick={saveAndApply} title="Save and make this the theme on the screen">
            <Check size={15} /> Apply
          </button>
        </div>
        {sizeMismatch && (
          <div className="banner info" style={{ borderRadius: 0, borderLeft: 0, borderRight: 0 }}>
            This theme is {theme.width}×{theme.height}; your screen is {status.screen!.width}×{status.screen!.height}. It is scaled to fit — set the theme's screen size (deselect everything) for a pixel-exact layout.
          </div>
        )}
        <Canvas editor={editor} frame={frame} grid={grid} snap={grid ? 8 : 1} />
        <div className="canvas-status">
          <span>{status.connected ? `live on ${status.screen?.name}` : "no screen — preview only"}</span>
          <span>render {status.render_ms.toFixed(1)} ms</span>
          {selected && (
            <span>
              {selected.type} @ {Math.round(selected.x)}, {Math.round(selected.y)}
            </span>
          )}
          {status.media_errors.length > 0 && <span style={{ color: "var(--red)" }}>missing media: {status.media_errors.map(([p]) => p).join(", ")}</span>}
        </div>
      </section>

      <aside className="editor-side is-right">
        <div className="editor-side-head">
          <span className="eyebrow">{selected ? `// ${selected.type}` : "// theme"}</span>
          {selected && (
            <button className="btn ghost sm" onClick={() => select(null)}>
              Done
            </button>
          )}
        </div>
        <div className="editor-side-body">
          <Inspector editor={editor} fonts={fonts} />
        </div>
      </aside>
    </div>
  );
}
