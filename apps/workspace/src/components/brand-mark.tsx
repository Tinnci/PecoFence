// In-app mark: the 工 of icons/workspace-small.svg without its tile — beam and base in the
// text colour, the upright one green and one flagged block.
// The paper tile belongs to Windows surfaces (taskbar, Start, title bar).
export function BrandMark({ size = 26 }: { size?: number }) {
  return (
    <svg
      viewBox="96 152 832 720"
      width={size}
      height={(size * 720) / 832}
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      <rect x="176" y="152" width="672" height="168" rx="24" />
      <rect x="96" y="704" width="832" height="168" rx="24" />
      <rect x="368" y="344" width="288" height="160" rx="20" fill="#3f7a3c" />
      <rect x="368" y="520" width="288" height="160" rx="20" fill="#b4532a" />
    </svg>
  );
}
