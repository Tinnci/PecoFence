import { Badge } from "@/components/ui/badge";
import { label } from "./model";
export function Status({ state }: { state: string }) {
  return (
    <Badge variant="outline" className={`status status-${state}`}>
      <span aria-hidden="true" className="status-dot" />
      {label(state)}
    </Badge>
  );
}
