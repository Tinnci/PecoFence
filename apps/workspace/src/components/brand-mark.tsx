import mark from "../../icons/workspace.svg";

// icons/workspace.svg is the single source of every app icon size.
export function BrandMark({ size = 38 }: { size?: number }) {
  return <img src={mark} width={size} height={size} alt="" draggable={false} />;
}
