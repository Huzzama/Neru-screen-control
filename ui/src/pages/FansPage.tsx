import { Fan } from "lucide-react";

import { Empty } from "../components/ui";

export function FansPage() {
  return (
    <div className="page stack">
      <div className="page-head">
        <div>
          <div className="eyebrow">// cooling</div>
          <h1 className="page-title">Fans</h1>
          <p className="page-sub">Fan and pump curves driven by CPU, GPU or coolant temperature.</p>
        </div>
      </div>
      <Empty icon={<Fan size={36} strokeWidth={1.25} />} title="Coming in a later phase">
        Fan speed control needs write access to the motherboard's PWM channels (hwmon). This phase lands after RGB.
      </Empty>
    </div>
  );
}
