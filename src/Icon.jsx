// The UI's icons: SVGs bundled from src/assets/icons, never fetched.
const SVGS = import.meta.glob("./assets/icons/*.svg", { query: "?raw", import: "default", eager: true });

/** The markup of icon `name` (src/assets/icons/<name>.svg). */
export const iconSvg = (name) => SVGS[`./assets/icons/${name}.svg`]?.trim();

/** Icon `name`, decorative: the control around it carries the words. */
export default function Icon({ name, className = "icon" }) {
  return <span className={className} data-icon={name} aria-hidden="true" dangerouslySetInnerHTML={{ __html: iconSvg(name) }} />;
}
