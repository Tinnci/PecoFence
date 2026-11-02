import { useTranslation } from "react-i18next";
import { t } from "../../i18n";
import { cn } from "cn";
import { Loader2Icon } from "lucide-react";

function Spinner({ className, ...props }: React.ComponentProps<"svg">) {
  useTranslation();
  return (
    <Loader2Icon
      data-slot="spinner"
      role="status"
      aria-label={t("controls.loading")}
      className={cn("size-4 animate-spin", className)}
      {...props}
    />
  );
}

export { Spinner };
