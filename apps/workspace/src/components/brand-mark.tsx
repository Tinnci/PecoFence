// In-app mark: the stepped 工 of icons/workspace.svg as a single-colour glyph.
// The plated Art Deco icon belongs to Windows surfaces (taskbar, Start, title bar).
export function BrandMark({ size = 26 }: { size?: number }) {
  return (
    <svg
      viewBox="232 236 560 560"
      width={size}
      height={size}
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M412 236h200v40h120v80H548v300h224v80H652v36h-70v24H442v-24h-70v-36H252v-80h224V356H292v-80h120z" />
    </svg>
  );
}
