import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FilePlus2, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import { Modal, useToast } from "../../components/ui";
import { api, errText, type MediaInfo } from "../../lib/api";

/** Pick a file from the media folder (or import a new one). */
export function MediaPicker({ value, onPick, onClose }: { value: string; onPick: (name: string) => void; onClose: () => void }) {
  const toast = useToast();
  const [items, setItems] = useState<MediaInfo[]>([]);
  const refresh = () => api.media().then(setItems).catch((e) => toast.error(errText(e)));
  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const importFile = async () => {
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "Images & video", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif", "mp4", "mkv", "webm", "mov", "avi", "m4v"] }],
      });
      if (!picked) return;
      const name = await api.importMedia(picked as string);
      toast.success(`Imported ${name}`);
      await refresh();
      onPick(name);
    } catch (e) {
      toast.error(errText(e));
    }
  };

  return (
    <Modal
      title="Media"
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            Close
          </button>
          <button className="btn primary" onClick={importFile}>
            <FilePlus2 size={15} /> Import file…
          </button>
        </>
      }
    >
      <p className="small muted">Files live in your media folder and are referenced by name, so themes stay portable. Video needs ffmpeg.</p>
      {items.length === 0 ? (
        <div className="empty">
          <h3>No media yet</h3>
          <span>Import a PNG, JPEG, WebP, GIF or a video.</span>
        </div>
      ) : (
        <div className="media-grid">
          {items.map((m) => (
            <div key={m.name} style={{ position: "relative" }}>
              <button className={`media-item ${m.name === value ? "selected" : ""}`} title={m.name} onClick={() => onPick(m.name)}>
                {m.kind === "video" ? (
                  <div style={{ height: "100%", display: "grid", placeItems: "center", fontSize: 10, color: "var(--text-dim)", padding: 6, overflowWrap: "anywhere" }}>{m.name}</div>
                ) : (
                  <img src={convertFileSrc(m.path)} alt={m.name} />
                )}
                <span className="media-kind">{m.kind}</span>
              </button>
              <button
                className="btn ghost icon sm"
                style={{ position: "absolute", top: 2, right: 2, background: "rgba(0,0,0,.6)" }}
                aria-label={`Delete ${m.name}`}
                onClick={() => api.deleteMedia(m.name).then(refresh).catch((e) => toast.error(errText(e)))}
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
    </Modal>
  );
}
