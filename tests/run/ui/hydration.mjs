// What a registry would hand a client: its serializable atoms, by key.
import * as Effect from "@flick-lang/effect-ts/effect/Effect";
import * as Hydration from "@flick-lang/effect-ts/effect/reactivity/Hydration";
import { RegistryService, nativeRegistry } from "@flick-lang/effect-ts/effect/reactivity/Atom";

export const dehydrated = () =>
  Effect.map(Effect.service(RegistryService), (record) =>
    Hydration.dehydrate(nativeRegistry(record))
      .map(({ key, value }) => `${key} = ${JSON.stringify(value)}`)
      .sort()
      .join("\n"),
  );
