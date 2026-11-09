// Effect's `Metric` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: options
// with `Option` fields, and states as Flick records.
import { type Duration, Effect, Metric, Option } from "effect";

import { tag } from "@flick-lang/runtime/tags";

export * from "effect/Metric";

type Attributes = ReadonlyArray<[string, string]>;

interface Common {
  readonly description: Option.Option<string>;
  readonly attributes: Attributes;
}

const common = (o: Common) => ({ description: Option.getOrUndefined(o.description), attributes: o.attributes });

/** `Metric.counter`: numbers only. */
export const flickCounter = (name: string, o: Common & { readonly incremental: boolean }) =>
  Metric.counter(name, { ...common(o), incremental: o.incremental });

/** `Metric.gauge`: numbers only. */
export const flickGauge = (name: string, o: Common) => Metric.gauge(name, common(o));

export const flickFrequency = (name: string, o: Common & { readonly preregisteredWords: ReadonlyArray<string> }) =>
  Metric.frequency(name, { ...common(o), preregisteredWords: o.preregisteredWords });

export const flickHistogram = (name: string, o: Common & { readonly boundaries: ReadonlyArray<number> }) =>
  Metric.histogram(name, { ...common(o), boundaries: o.boundaries });

type SummaryOptions = Common & {
  readonly maxAge: Duration.Duration;
  readonly maxSize: number;
  readonly quantiles: ReadonlyArray<number>;
};

const summaryOptions = (o: SummaryOptions) => ({
  ...common(o),
  maxAge: o.maxAge,
  maxSize: o.maxSize,
  quantiles: o.quantiles,
});

export const flickSummary = (name: string, o: SummaryOptions) => Metric.summary(name, summaryOptions(o));

export const flickSummaryWithTimestamp = (name: string, o: SummaryOptions) =>
  Metric.summaryWithTimestamp(name, summaryOptions(o));

export const flickTimer = (name: string, o: Common & { readonly boundaries: Option.Option<ReadonlyArray<number>> }) =>
  Metric.timer(name, Option.match(o.boundaries, { onNone: () => common(o), onSome: (boundaries) => ({ ...common(o), boundaries }) }));

/** The type of a metric with its state. */
type Kinded = Metric.Metric.Snapshot extends infer S ? (S extends Metric.Metric.Snapshot ? Pick<S, "type" | "state"> : never) : never;

/** A state of Effect as the Flick record of its kind. */
const state = (s: Kinded): unknown => {
  switch (s.type) {
    case "Counter":
      return { count: Number(s.state.count), incremental: s.state.incremental };
    case "Gauge":
      return { value: Number(s.state.value) };
    case "Frequency":
      return { occurrences: [...s.state.occurrences] };
    case "Histogram": {
      const h = s.state;
      return { buckets: h.buckets, count: h.count, min: h.min, max: h.max, sum: h.sum };
    }
    default: {
      const q = s.state;
      const quantiles = q.quantiles.map(([quantile, v]) => [quantile, Option.fromUndefinedOr(v)] as const);
      return { quantiles, count: q.count, min: q.min, max: q.max, sum: q.sum };
    }
  }
};

/** `Metric.value`. */
export const flickValue = <I, S>(metric: Metric.Metric<I, S>): Effect.Effect<unknown> =>
  Effect.map(Metric.value(metric), (s) =>
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a metric of a type has the state of that type
    state({ type: metric.type, state: s } as Kinded),
  );

const STATES: Readonly<Record<Metric.Metric.Type, string>> = {
  Counter: tag("Metric.State.CounterValue"),
  Gauge: tag("Metric.State.GaugeValue"),
  Frequency: tag("Metric.State.FrequencyValue"),
  Histogram: tag("Metric.State.HistogramValue"),
  Summary: tag("Metric.State.SummaryValue"),
};

/** `Metric.snapshot`: `Metric.Snapshot` records. */
export const flickSnapshot = () =>
  Effect.map(Metric.snapshot, (all) =>
    all.map((s) => ({
      id: s.id,
      description: Option.fromUndefinedOr(s.description),
      attributes: Object.entries(s.attributes ?? {}),
      state: { _tag: STATES[s.type], _0: state(s) },
    })),
  );

export const flickDump = (): Effect.Effect<string> => Metric.dump;

/** `Metric.with_current_attributes`. */
export const flickWithCurrentAttributes =
  (attributes: Attributes) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, Metric.CurrentMetricAttributes, Object.fromEntries(attributes));
