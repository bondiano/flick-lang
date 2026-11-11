import { uiRunWith } from "@flick-lang/effect-ts/ui/react";
import { withActionSpans } from "@flick-lang/effect-ts/ui/tracing";
export const run = (...args) => withActionSpans(true)(uiRunWith(...args));
