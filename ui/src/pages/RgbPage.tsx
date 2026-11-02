import { Lightbulb } from "lucide-react";

import { Empty } from "../components/ui";

export function RgbPage() {
  return (
    <div className="page stack">
      <div className="page-head">
        <div>
          <div className="eyebrow">// lighting</div>
          <h1 className="page-title">RGB</h1>
          <p className="page-sub">Motherboard, fans, pump, RAM and GPU lighting through OpenRGB.</p>
        </div>
      </div>
      <Empty icon={<Lightbulb size={36} strokeWidth={1.25} />} title="Coming in the next phase">
        Neru will talk to the OpenRGB SDK server (port 6742): devices and zones, colours, hardware modes, direct effects and
        temperature-reactive lighting.
      </Empty>
    </div>
  );
}
