// The UI's icons: SVGs bundled from src/assets/icons, never fetched.
import { iconChar } from "./symbols.js";

const SVGS = import.meta.glob("./assets/icons/*.svg", { query: "?raw", import: "default", eager: true });

/** The markup of icon `name` (src/assets/icons/<name>.svg). */
export const iconSvg = (name) => SVGS[`./assets/icons/${name}.svg`]?.trim();

/** Icon `name`, decorative: the control around it carries the words. */
export default function Icon({ name, className = "icon" }) {
  return <span className={className} data-icon={name} aria-hidden="true" dangerouslySetInnerHTML={{ __html: iconSvg(name) }} />;
}

/**
 * Profile icon `name`, any bundled icon name (src/symbols.js),
 * drawn from the bundled font at the size of an Icon; decorative.
 */
export function SymbolIcon({ name, className = "icon" }) {
  return (
    <span className={`${className} symbol`} data-icon={name} aria-hidden="true">
      <span className="symbol-glyph" data-glyph={iconChar(name)} />
    </span>
  );
}
