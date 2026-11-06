export type Command = {
  id: string;
  label: string;
  /** Other words that find it: a model's code, English for a Chinese label. */
  words?: string;
  /** Found by typing only, so an empty palette stays short. */
  typed?: boolean;
  run: () => void;
};

const MOST = 8;

/** Commands for what was typed: names that start with it first, then any match. */
export function matchCommands(commands: Command[], text: string) {
  const words = text.trim().toLowerCase();
  if (!words) return commands.filter((c) => !c.typed);
  const scored = commands.flatMap((command) => {
    const label = command.label.toLowerCase();
    const found = `${label} ${command.words ?? ""}`.toLowerCase();
    if (!found.includes(words)) return [];
    return [{ command, first: label.startsWith(words) ? 0 : 1 }];
  });
  return scored
    .sort((a, b) => a.first - b.first)
    .slice(0, MOST)
    .map((s) => s.command);
}

/** The next row, wrapping at both ends. */
export function moveActive(active: number, step: number, count: number) {
  return count ? (active + step + count) % count : 0;
}
