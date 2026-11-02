import type { ReactNode } from "react";
import { LazyMotion, MotionConfig } from "motion/react";
import NumberFlow from "@number-flow/react";

// Motion for elements that enter, leave or move. `m` renders at once and gains its features
// (domMax, for layout animations) from a chunk loaded after first paint. Reduced motion
// follows the Windows setting.
const features = () => import("./motion-features").then((r) => r.default);
export function MotionRoot({ children }: { children: ReactNode }) {
  return (
    <LazyMotion features={features} strict>
      <MotionConfig
        reducedMotion="user"
        transition={{ duration: 0.28, ease: [0.2, 0.8, 0.2, 1] }}
      >
        {children}
      </MotionConfig>
    </LazyMotion>
  );
}

/** A figure whose digits roll to a new value; NumberFlow honours reduced motion itself. */
export function Num({
  value,
  suffix,
  prefix,
  digits = 1,
  sign = false,
}: {
  value: number;
  suffix?: string;
  prefix?: string;
  digits?: number;
  sign?: boolean;
}) {
  return (
    <NumberFlow
      value={value}
      prefix={prefix}
      suffix={suffix}
      format={{
        maximumFractionDigits: digits,
        useGrouping: false,
        signDisplay: sign ? "exceptZero" : "auto",
      }}
    />
  );
}
