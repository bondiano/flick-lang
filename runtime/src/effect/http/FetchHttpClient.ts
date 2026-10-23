// Effect's `FetchHttpClient` for the bindings of the standard library.
import * as FetchHttpClient from "effect/http/FetchHttpClient";

import { serviceOf } from "./HttpClient.ts";

export * from "effect/http/FetchHttpClient";

/** `Http.FetchClient.layer`. */
export const flickLayer = serviceOf(FetchHttpClient.layer);
