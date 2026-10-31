import { label } from "./model";
export function Status({ state }: { state: string }) {
  return (
    <span className={`status status-${state}`}>
      <span aria-hidden="true" className="status-dot" />
      {label(state)}
    </span>
  );
}
