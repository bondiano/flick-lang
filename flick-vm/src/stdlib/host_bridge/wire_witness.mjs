// Well-known Flick constructor tag registry, UTF-16 helpers, and Codec/Schema/Route
// witness decoders shared between `wire.mjs` and `wire_encode.mjs`.

export const tagHashToFull = new Map();
const fullToTagHash = new Map();
export const tupleArrays = new WeakSet();
export const opaqueObjects = new WeakSet();

export function registerTag(fullTag, tagFn) {
  if (fullTag.includes("#")) return fullTag;
  let hashed = fullToTagHash.get(fullTag);
  if (hashed === undefined) {
    hashed = tagFn(fullTag);
    fullToTagHash.set(fullTag, hashed);
    tagHashToFull.set(hashed, fullTag);
  }
  return hashed;
}

export function initWellKnownTags(tagFn) {
  for (const full of [
    "Arbitrary.CheckResult.Exhausted",
    "Arbitrary.CheckResult.Falsified",
    "Arbitrary.CheckResult.Passed",
    "Arbitrary.CheckResult.ReplayMismatch",
    "Arbitrary.PropertyFailure.PropertyError",
    "Arbitrary.PropertyFailure.ReturnedFalse",
    "Arbitrary.SampleError.SampleError",
    "Atom.Async.Failed",
    "Atom.Async.Loaded",
    "Atom.Async.Loading",
    "BigDecimal.DecimalError.DecimalError",
    "Cause.NoSuchElementError.NoSuchElementError",
    "ChildProcess.ProcessError.ProcessError",
    "Cli.CliError.CliError",
    "Cli.Prompt.Action.Beep",
    "Cli.Prompt.Action.NextFrame",
    "Cli.Prompt.Action.Submit",
    "Cli.Prompt.ProcessInput.Event",
    "Cli.Prompt.ProcessInput.Input",
    "Codec.ExportError.ExportError",
    "Config.ConfigError.ConfigError",
    "ConfigProvider.Node.ValueNode",
    "ConfigProvider.SourceError.SourceError",
    "Cron.CronParseError.CronParseError",
    "Crypto.CryptoError.CryptoError",
    "DateTime.DateTimeError.DateTimeError",
    "Duration.DurationError.DurationError",
    "Effect.Collections.ValidationError.ValidationError",
    "ExecutionPlan.Event.AttemptFailure",
    "ExecutionPlan.Event.AttemptStart",
    "ExecutionPlan.Event.AttemptSuccess",
    "File.FileError.IoError",
    "File.FileError.NotFound",
    "File.FileError.PermissionDenied",
    "Graph.Types.GraphError.GraphError",
    "Http.Body.BodyErrorReason.InvalidJson",
    "Http.Body.BodyErrorReason.InvalidSchema",
    "Http.Body.HttpBodyError.HttpBodyError",
    "Http.ClientError.ClientErrorReason.DecodeError",
    "Http.ClientError.ClientErrorReason.EmptyBodyError",
    "Http.ClientError.ClientErrorReason.EncodeError",
    "Http.ClientError.ClientErrorReason.InvalidUrlError",
    "Http.ClientError.ClientErrorReason.RequestError",
    "Http.ClientError.ClientErrorReason.StatusCodeError",
    "Http.ClientError.ClientErrorReason.TransportError",
    "Http.ClientError.HttpClientError.HttpClientError",
    "Http.Cookies.CookiesError.CookiesError",
    "Http.Cookies.CookiesErrorReason.InvalidCookiePath",
    "Http.Etag.Etag.Strong",
    "Http.Etag.Etag.Weak",
    "Http.Headers.Redaction.Plain",
    "Http.Headers.Redaction.Secret",
    "Http.HttpError.HttpError",
    "Http.Method.Method.DELETE",
    "Http.Method.Method.GET",
    "Http.Method.Method.HEAD",
    "Http.Method.Method.OPTIONS",
    "Http.Method.Method.PATCH",
    "Http.Method.Method.POST",
    "Http.Method.Method.PUT",
    "Http.Method.Method.QUERY",
    "Http.Method.Method.TRACE",
    "Http.Multipart.MultipartError.MultipartError",
    "Http.Multipart.MultipartErrorReason.BodyTooLarge",
    "Http.Multipart.MultipartErrorReason.FieldTooLarge",
    "Http.Multipart.MultipartErrorReason.FileTooLarge",
    "Http.Multipart.MultipartErrorReason.InternalError",
    "Http.Multipart.MultipartErrorReason.Parse",
    "Http.Multipart.MultipartErrorReason.TooManyParts",
    "Http.Multipart.Part.Field",
    "Http.Multipart.Part.File",
    "Http.Server.Address.TcpAddress",
    "Http.Server.Address.UnixAddress",
    "Http.ServerError.HttpServerError.HttpServerError",
    "Http.ServerError.ServeError.ServeError",
    "Http.ServerError.ServerErrorReason.InternalError",
    "Http.ServerError.ServerErrorReason.RequestParseError",
    "Http.ServerError.ServerErrorReason.ResponseError",
    "Http.ServerError.ServerErrorReason.RouteNotFound",
    "Http.Url.UrlError.UrlError",
    "HttpApi.ClientError.ClientError",
    "HttpApi.HttpError.HttpError",
    "HttpApi.ServeError.ServeError",
    "Int.ArithmeticError.ArithmeticError",
    "Js.SnapshotError.SnapshotError",
    "JsonPatch.Operation.Add",
    "JsonPatch.Operation.Remove",
    "JsonPatch.Operation.Replace",
    "JsonPatch.PatchError.PatchError",
    "Logger.PrettyMode.Auto",
    "Logger.PrettyMode.Browser",
    "Logger.PrettyMode.Tty",
    "Loop.Step.Continue",
    "Metric.State.CounterValue",
    "Metric.State.FrequencyValue",
    "Metric.State.GaugeValue",
    "Metric.State.HistogramValue",
    "Metric.State.SummaryValue",
    "PlatformError.PlatformError.PlatformError",
    "PlatformError.Reason.Bad",
    "PlatformError.Reason.System",
    "Pool.TimeToLiveStrategy.Creation",
    "Prelude.AppError.AppError",
    "Prelude.Ordering.EQ",
    "Prelude.Ordering.GT",
    "Prelude.Ordering.LT",
    "Prelude.SchemaError.SchemaError",
    "Prelude.TimeoutError.TimeoutError",
    "Queue.Strategy.Dropping",
    "Queue.Strategy.Sliding",
    "Queue.Strategy.Suspend",
    "RcMap.ExceededCapacity.ExceededCapacity",
    "RegExp.RegExpError.RegExpError",
    "Rpc.ClientError.ClientError",
    "Rpc.ServeError.ServeError",
    "Sql.DbError.DbError",
    "Sql.DecodeError.DecodeError",
    "Sql.EncodeError.EncodeError",
    "Stream.Platform.WatchEvent.Create",
    "Stream.Platform.WatchEvent.Remove",
    "Stream.Platform.WatchEvent.Update",
    "Terminal.QuitError.QuitError",
    "Test.Assertion.Failed",
    "Test.Assertion.Pass",
    "Test.AssertionFailed.AssertionFailed",
    "Tracer.SpanStatus.Ended",
    "Tracer.SpanStatus.Started",
    "Tx.RetryReason.Conflict",
    "Tx.RetryReason.Retry",
    "Ui.Detached.Detached",
    "Ui.Level.H1",
    "Ui.Level.H2",
    "Ui.Level.H3",
    "Ui.Level.H4",
    "Ui.Level.H5",
    "Ui.Level.H6",
    "Ui.Live.Assertive",
    "Ui.Live.Off",
    "Ui.Live.Polite",
  ]) {
    registerTag(full, tagFn);
  }
}

export function fromUtf16Units(units) {
  let out = "";
  const CHUNK = 4096;
  for (let i = 0; i < units.length; i += CHUNK) {
    out += String.fromCharCode(...units.slice(i, i + CHUNK));
  }
  return out;
}

export function hasLoneSurrogate(str) {
  for (let i = 0; i < str.length; i++) {
    const c = str.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = i + 1 < str.length ? str.charCodeAt(i + 1) : 0;
      if (next >= 0xdc00 && next <= 0xdfff) {
        i++;
      } else {
        return true;
      }
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      return true;
    }
  }
  return false;
}

export function toUtf16Units(str) {
  const units = new Array(str.length);
  for (let i = 0; i < str.length; i++) {
    units[i] = str.charCodeAt(i);
  }
  return units;
}

function fullTagOf(obj) {
  if (typeof obj === "object" && obj !== null && typeof obj._tag === "string") {
    return tagHashToFull.get(obj._tag) ?? obj._tag;
  }
  return undefined;
}

function prefixSchemaErr(res, path) {
  if (!path || !res || res._tag !== "Failure" || !res.failure || !Array.isArray(res.failure._0)) {
    return res;
  }
  return {
    ...res,
    failure: {
      ...res.failure,
      _0: res.failure._0.map((issue) => ({
        ...issue,
        path: issue.path === "(root)" || !issue.path ? path : `${path}.${issue.path}`,
      })),
    },
  };
}

function metaToWitness(meta, ctx) {
  const sm = ctx.schemaMod;
  const mt = fullTagOf(meta);
  if (!mt) return undefined;
  switch (mt) {
    case "Codec.$Meta.$String": return sm.schemaString;
    case "Codec.$Meta.$Number": return sm.schemaNumber;
    case "Codec.$Meta.$Int": return sm.schemaInt;
    case "Codec.$Meta.$BigInt": return sm.schemaBigint;
    case "Codec.$Meta.$Bool": return sm.schemaBool;
    case "Codec.$Meta.$Unit": return sm.schemaUnit;
    case "Codec.$Meta.$Never": return sm.schemaNever;
    case "Codec.$Meta.$Unknown":
    case "Codec.$Meta.$Json": return sm.schemaUnknown;
    case "Codec.$Meta.$List": {
      const item = metaToWitness(meta._0, ctx);
      return item ? sm.schemaList(item) : undefined;
    }
    case "Codec.$Meta.$Option": {
      const item = metaToWitness(meta._0, ctx);
      return item ? sm.schemaOption(item) : undefined;
    }
    case "Codec.$Meta.$Result": {
      const e = metaToWitness(meta._0, ctx);
      const o = metaToWitness(meta._1, ctx);
      return e && o ? sm.schemaResult(e, o) : undefined;
    }
    case "Codec.$Meta.$Tuple": {
      const items = [];
      for (const m of meta._0) {
        const w = metaToWitness(m, ctx);
        if (!w) return undefined;
        items.push(w);
      }
      return sm.schemaTupleOf(items);
    }
    case "Codec.$Meta.$Record": {
      const fields = [];
      for (const [k, m] of meta._0) {
        const w = metaToWitness(m, ctx);
        if (!w) return undefined;
        fields.push([k, w]);
      }
      return sm.schemaRecordOf(fields);
    }
    case "Codec.$Meta.$Adt": {
      const cases = [];
      for (const entry of meta._0) {
        const ft = entry[0];
        const ms = entry.length === 3 ? entry[2] : entry[1];
        const fs = [];
        if (Array.isArray(ms)) {
          for (const m of ms) {
            const w = metaToWitness(m, ctx);
            if (!w) return undefined;
            fs.push(w);
          }
        }
        cases.push({ tag: registerTag(ft, ctx.tagFn), fields: fs });
      }
      return sm.schemaAdtOf("adt", cases);
    }
    case "Codec.$Meta.$Enum":
      return sm.schemaLiterals(sm.schemaUnknown, undefined, meta._0);
    case "Codec.$Meta.$Literal":
      return sm.schemaLiterals(sm.schemaUnknown, undefined, [meta._0]);
    case "Codec.$Meta.$Checked":
      return metaToWitness(meta._0, ctx);
    case "Codec.$Meta.$Lazy":
      return sm.schemaLazyOf(meta._0, () => meta._1(undefined));
    default:
      return undefined;
  }
}

export function decodeCodecWitness(fields, ctx) {
  const sm = ctx.schemaMod;
  const [dec, enc, _acc, meta, extra] = fields;
  const decodeAt = (input, path) => prefixSchemaErr(dec(input), path);
  const encodeAt = (input, path) => prefixSchemaErr(enc(input), path);
  const extraTag = fullTagOf(extra);
  const metaTag = fullTagOf(meta);
  if (extraTag === "Codec.$Extra.$Block") {
    const name = extra._0;
    const rawFields = extra._1;
    const blockFields = rawFields.map(([label, sp]) => {
      const entry = { wire: sp._0, codec: sp._1, optional: Boolean(sp._2) };
      const om = sp._3;
      if (om && om._tag === "Some" && om.value) {
        const omt = fullTagOf(om.value);
        if (omt && omt.includes("Effect")) entry.onMissingEffect = om.value._0;
        else if (typeof om.value === "function") entry.onMissing = om.value;
        else if (typeof om.value._0 === "function") entry.onMissing = om.value._0;
      }
      return [label, entry];
    });
    const ignoreExtra = metaTag === "Codec.$Meta.$Block" ? Boolean(meta._1) : false;
    const w = sm.witness(`block:${name}`, decodeAt, encodeAt);
    return sm.withBlock(w, blockFields, ignoreExtra);
  }
  let desc;
  if (extraTag === "Codec.$Extra.$TransformAs") {
    const target = extra._0;
    const valueAt =
      target && typeof target.value === "function"
        ? (input, path) => target.value(input, path)
        : undefined;
    return sm.witness(target?.kind ?? "transform-as", decodeAt, encodeAt, valueAt);
  }
  if (extraTag === "Codec.$Extra.$ChunkCodec") {
    desc = { kind: "chunk", item: extra._0 };
  } else if (metaTag === "Codec.$Meta.$Stage") {
    const from = metaToWitness(meta._0, ctx);
    desc = from ? { kind: "transform", from } : undefined;
  } else if (metaTag === "Codec.$Meta.$Refined") {
    const base = metaToWitness(meta._2, ctx);
    const known = meta._1 && meta._1._tag === "Some" ? meta._1.value : undefined;
    desc = base ? { kind: "refined", name: meta._0, known, base } : undefined;
  } else {
    const structural = metaToWitness(meta, ctx);
    if (structural) desc = sm.schemaDescription(structural);
  }
  const w = sm.witness(desc?.kind ?? "codec", decodeAt, encodeAt, undefined, desc);
  if (metaTag === "Codec.$Meta.$Refined" && meta._1 && meta._1._tag === "Some") {
    w._flickKnown = meta._1.value;
  }
  return w;
}

function descToStructuralWitness(desc, label, ctx) {
  const sm = ctx.schemaMod;
  if (!desc || !sm) return undefined;
  switch (desc.kind) {
    case "string": return sm.schemaString;
    case "number": return sm.schemaNumber;
    case "int": return sm.schemaInt;
    case "bigint": return sm.schemaBigint;
    case "bool": return sm.schemaBool;
    case "unit": return sm.schemaUnit;
    case "never": return sm.schemaNever;
    case "unknown": return sm.schemaUnknown;
    case "list": return sm.schemaList(desc.item);
    case "option": return sm.schemaOption(desc.item);
    case "result": return sm.schemaResult(desc.error, desc.value);
    case "tuple": return sm.schemaTupleOf(desc.items);
    case "record": return sm.schemaRecordOf(desc.fields);
    case "adt": return sm.schemaAdtOf(label, desc.cases);
    case "literals": return sm.schemaLiterals(sm.schemaUnknown, undefined, desc.values);
    case "lazy": return sm.schemaLazyOf(desc.name, desc.get);
    default: return undefined;
  }
}

export function decodeSchemaWitness(fields, ctx) {
  const sm = ctx.schemaMod;
  const [val, codec, kind] = fields;
  const fallbackValueAt = (input, path) => prefixSchemaErr(val(input), path);
  const kt = fullTagOf(kind);
  let desc;
  let label = codec?.kind ?? "schema";
  switch (kt) {
    case "Schema.$Kind.$String": desc = { kind: "string" }; label = "string"; break;
    case "Schema.$Kind.$Number": desc = { kind: "number" }; label = "number"; break;
    case "Schema.$Kind.$Int": desc = { kind: "int" }; label = "int"; break;
    case "Schema.$Kind.$BigInt": desc = { kind: "bigint" }; label = "bigint"; break;
    case "Schema.$Kind.$Bool": desc = { kind: "bool" }; label = "bool"; break;
    case "Schema.$Kind.$Unit": desc = { kind: "unit" }; label = "unit"; break;
    case "Schema.$Kind.$Never": desc = { kind: "never" }; label = "never"; break;
    case "Schema.$Kind.$Unknown": desc = { kind: "unknown" }; label = "unknown"; break;
    case "Schema.$Kind.$Duration": desc = { kind: "duration-millis" }; label = "duration"; break;
    case "Schema.$Kind.$List": desc = { kind: "list", item: kind._0 }; label = "list"; break;
    case "Schema.$Kind.$Chunk": desc = { kind: "chunk", item: kind._0 }; label = "chunk"; break;
    case "Schema.$Kind.$Option": desc = { kind: "option", item: kind._0 }; label = "option"; break;
    case "Schema.$Kind.$Result": desc = { kind: "result", error: kind._0, value: kind._1 }; label = "result"; break;
    case "Schema.$Kind.$Tuple": desc = { kind: "tuple", items: kind._0 }; label = "tuple"; break;
    case "Schema.$Kind.$Record": desc = { kind: "record", fields: kind._0 }; label = "record"; break;
    case "Schema.$Kind.$Adt":
      label = kind._0;
      desc = {
        kind: "adt",
        cases: kind._1.map(([ft, _sn, fs]) => ({
          tag: registerTag(ft, ctx.tagFn),
          fields: fs,
        })),
      };
      break;
    case "Schema.$Kind.$Lazy":
      label = kind._0;
      desc = { kind: "lazy", name: kind._0, get: () => kind._1(undefined) };
      break;
    case "Schema.$Kind.$Enum":
      label = "literals";
      desc = { kind: "literals", values: kind._0 };
      break;
    case "Schema.$Kind.$Refined":
      label = kind._0;
      desc = { kind: "refined", name: kind._0, known: codec?._flickKnown, base: kind._1 };
      break;
    default:
      desc = codec ? sm.schemaDescription(codec) : undefined;
      break;
  }
  const structural = descToStructuralWitness(desc, label, ctx);
  const valueAt = structural ? structural.value : fallbackValueAt;
  return sm.witness(label, codec.decodeAt, codec.encodeAt, valueAt, desc);
}

export function decodeRouteUrl(fields, ctx) {
  const rm = ctx.routeMod;
  const [schema, rawRoutes] = fields;
  const routes = rawRoutes.map((r) => {
    const rtag = registerTag(r._0, ctx.tagFn);
    const rpath = r._1;
    const rsegs = r._2.map((seg) => {
      const st = fullTagOf(seg);
      if (st === "Route.Segment.$Words") {
        return rm.routeWords(seg._0.map(([w, t]) => [w, registerTag(t, ctx.tagFn)]));
      }
      if (st === "Route.Segment.$Nested") {
        return rm.routeNestedOf(seg._0);
      }
      if (st === "Route.Segment.$Schema") {
        return rm.routeSegment(seg._0);
      }
      return seg;
    });
    const rq =
      r._3 && r._3._tag === "Some"
        ? r._3.value.map(([lbl, sch, opt]) => rm.routeQuery(lbl, sch, Boolean(opt)))
        : null;
    const ropt = Boolean(r._4);
    return [rtag, rpath, rsegs, rq, ropt];
  });
  return rm.routeCodec(schema, routes);
}
