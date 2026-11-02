import { useCallback, useMemo, useRef, useState } from "react";

import type { Element, Theme } from "../../lib/api";
import { clone } from "../../lib/theme-utils";

export interface EditorState {
  theme: Theme;
  selectedId: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
}

/**
 * Theme editing state with undo/redo. `patch` changes the theme and records
 * history; `live` changes it without recording (used during drags), and
 * `commit` records the current state afterwards.
 */
export function useEditor(initial: Theme) {
  const [theme, setThemeState] = useState<Theme>(() => clone(initial));
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const undoRef = useRef<Theme[]>([]);
  const redoRef = useRef<Theme[]>([]);
  const liveBase = useRef<Theme | null>(null);
  const [, bump] = useState(0);

  const reset = useCallback((t: Theme) => {
    setThemeState(clone(t));
    setSelectedId(null);
    setDirty(false);
    undoRef.current = [];
    redoRef.current = [];
    liveBase.current = null;
    bump((x) => x + 1);
  }, []);

  const patch = useCallback((fn: (t: Theme) => void) => {
    setThemeState((cur) => {
      undoRef.current.push(clone(cur));
      if (undoRef.current.length > 100) undoRef.current.shift();
      redoRef.current = [];
      const next = clone(cur);
      fn(next);
      return next;
    });
    setDirty(true);
    bump((x) => x + 1);
  }, []);

  const live = useCallback((fn: (t: Theme) => void) => {
    setThemeState((cur) => {
      if (!liveBase.current) liveBase.current = clone(cur);
      const next = clone(cur);
      fn(next);
      return next;
    });
    setDirty(true);
  }, []);

  const commit = useCallback(() => {
    if (!liveBase.current) return;
    undoRef.current.push(liveBase.current);
    if (undoRef.current.length > 100) undoRef.current.shift();
    redoRef.current = [];
    liveBase.current = null;
    bump((x) => x + 1);
  }, []);

  const undo = useCallback(() => {
    setThemeState((cur) => {
      const prev = undoRef.current.pop();
      if (!prev) return cur;
      redoRef.current.push(clone(cur));
      return prev;
    });
    setDirty(true);
    bump((x) => x + 1);
  }, []);

  const redo = useCallback(() => {
    setThemeState((cur) => {
      const next = redoRef.current.pop();
      if (!next) return cur;
      undoRef.current.push(clone(cur));
      return next;
    });
    setDirty(true);
    bump((x) => x + 1);
  }, []);

  const updateElement = useCallback(
    (id: string, fn: (el: Element) => void, mode: "patch" | "live" = "patch") => {
      const apply = (t: Theme) => {
        const el = t.elements.find((e) => e.id === id);
        if (el) fn(el);
      };
      if (mode === "live") live(apply);
      else patch(apply);
    },
    [live, patch],
  );

  const selected = useMemo(() => theme.elements.find((e) => e.id === selectedId) ?? null, [theme, selectedId]);

  return {
    theme,
    selected,
    selectedId,
    select: setSelectedId,
    dirty,
    markClean: () => setDirty(false),
    reset,
    patch,
    live,
    commit,
    undo,
    redo,
    canUndo: undoRef.current.length > 0,
    canRedo: redoRef.current.length > 0,
    updateElement,
  };
}

export type Editor = ReturnType<typeof useEditor>;
