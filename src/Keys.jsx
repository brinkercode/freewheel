import { useEffect, useState } from "react";
import { getConfig } from "./settingsApi.js";

// Every Speed Editor key: its binding name and printed label, as
// printed_label in freewheel-core status.rs (Keys.test.jsx checks they match).
export const KEYS = [
  ["SmartInsrt", "SMART INSRT"], ["Appnd", "APPND"], ["RiplOwr", "RIPL O/WR"], ["CloseUp", "CLOSE UP"],
  ["PlaceOnTop", "PLACE ON TOP"], ["SrcOwr", "SRC O/WR"], ["In", "IN"], ["Out", "OUT"],
  ["TrimIn", "TRIM IN"], ["TrimOut", "TRIM OUT"], ["Roll", "ROLL"], ["SlipSrc", "SLIP SRC"],
  ["SlipDest", "SLIP DEST"], ["TransDur", "TRANS DUR"], ["Cut", "CUT"], ["Dis", "DIS"],
  ["SmthCut", "SMTH CUT"], ["Source", "SOURCE"], ["Timeline", "TIMELINE"], ["Shtl", "SHTL"],
  ["Jog", "JOG"], ["Scrl", "SCRL"], ["Esc", "ESC"], ["SyncBin", "SYNC BIN"],
  ["AudioLevel", "AUDIO LEVEL"], ["FullView", "FULL VIEW"], ["Trans", "TRANS"], ["Split", "SPLIT"],
  ["Snap", "SNAP"], ["RiplDel", "RIPL DEL"], ["Cam1", "CAM 1"], ["Cam2", "CAM 2"],
  ["Cam3", "CAM 3"], ["Cam4", "CAM 4"], ["Cam5", "CAM 5"], ["Cam6", "CAM 6"],
  ["Cam7", "CAM 7"], ["Cam8", "CAM 8"], ["Cam9", "CAM 9"], ["LiveOwr", "LIVE O/WR"],
  ["VideoOnly", "VIDEO ONLY"], ["AudioOnly", "AUDIO ONLY"], ["StopPlay", "STOP/PLAY"],
];

// The Keys tab: each key of the base layer, 'Not set' until it is bound.
export default function Keys({ initial = null }) {
  const [config, setConfig] = useState(initial);
  useEffect(() => {
    if (initial || !globalThis.window?.__TAURI__) return;
    getConfig().then(setConfig, (error) => console.error("freewheel: loading the config failed:", String(error)));
  }, [initial]);
  const base = config?.layers.find((l) => l.id === config.base_layer);
  return (
    <section id="keys">
      {KEYS.map(([key, label]) => (
        <div key={key} className="key">
          <strong>{label}</strong> <span>{base?.bindings[key] ? "Set" : "Not set"}</span>
        </div>
      ))}
    </section>
  );
}
