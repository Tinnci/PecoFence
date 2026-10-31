// Same geometry as icons/workspace.svg, the source of every app icon size.
export function BrandMark({ size = 38 }: { size?: number }) {
  return (
    <svg
      viewBox="0 0 1024 1024"
      width={size}
      height={size}
      aria-hidden="true"
      focusable="false"
    >
      <rect x="48" y="48" width="928" height="928" rx="216" fill="#4d6a55" />
      <rect x="252" y="252" width="520" height="112" rx="40" fill="#fffaf0" />
      <rect x="456" y="300" width="112" height="420" fill="#fffaf0" />
      <rect x="180" y="660" width="664" height="112" rx="40" fill="#fffaf0" />
      <circle cx="704" cy="512" r="72" fill="#f0a73a" />
    </svg>
  );
}
