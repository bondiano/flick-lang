// Writes runtime-go/flickrt/datetime_intl.go: what Effect's DateTime asks
// Intl, as Node's ICU (the JS target's) answers it, so that both targets
// name zones and format date-times alike:
// - the zone names of Go's time zone database that V8 accepts, and the
//   name it resolves each to;
// - for a fixed set of common locales, the patterns of
//   `Intl.DateTimeFormat`'s styles, with their month, weekday and day
//   period names and the names of UTC and of offsets.
// Run from the repository root: `node tools/go-intl/gen.mjs`.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const out = "runtime-go/flickrt/datetime_intl.go";

// the entry names of Go's zoneinfo.zip, which `time/tzdata` embeds
const goZones = () => {
  const root = execFileSync("go", ["env", "GOROOT"], { encoding: "utf8" }).trim();
  const zip = readFileSync(`${root}/lib/time/zoneinfo.zip`);
  let end = zip.length - 22;
  while (zip.readUInt32LE(end) !== 0x06054b50) end--;
  const names = [];
  for (let i = 0, at = zip.readUInt32LE(end + 16); i < zip.readUInt16LE(end + 10); i++) {
    const n = zip.readUInt16LE(at + 28);
    names.push(zip.toString("utf8", at + 46, at + 46 + n));
    at += 46 + n + zip.readUInt16LE(at + 30) + zip.readUInt16LE(at + 32);
  }
  return names;
};

const resolveZone = (name) => {
  try {
    return new Intl.DateTimeFormat("en-US", { timeZone: name }).resolvedOptions().timeZone;
  } catch {
    return undefined;
  }
};

const zoneNamesResolved = goZones().flatMap((name) => {
  const resolved = resolveZone(name);
  if (resolved === undefined) return [];
  if (resolveZone(resolved) !== resolved) throw new Error(`${name} resolves to ${resolved}, which resolves on`);
  return [[name, resolved]];
});
const zones = zoneNamesResolved.map(([name, resolved]) => (resolved === name ? name : `${name}>${resolved}`)).join(";");
const zoneIds = [...new Set(zoneNamesResolved.map(([, resolved]) => resolved))].filter((id) => id !== "UTC").sort();

// the languages and regions ICU replaces when it canonicalizes a tag
const LETTERS = "abcdefghijklmnopqrstuvwxyz";
const codes2 = Array.from(LETTERS).flatMap((a) => Array.from(LETTERS, (b) => a + b));
const canonical = (tag) => {
  try {
    return Intl.getCanonicalLocales(tag)[0];
  } catch {
    return tag;
  }
};
const languageAliases = [...codes2, ...codes2.flatMap((c) => Array.from(LETTERS, (d) => c + d))]
  .filter((c) => canonical(c) !== c)
  .map((c) => `${c}>${canonical(c)}`)
  .join(";");
const regionAliases = [...codes2.map((c) => c.toUpperCase()), ...Array.from({ length: 1000 }, (_, i) => String(i).padStart(3, "0"))]
  .filter((c) => canonical(`und-${c}`) !== `und-${c}`)
  .map((c) => `${c}>${canonical(`und-${c}`).slice(4)}`)
  .join(";");

// Common locales on the Gregorian calendar with Latin digits. The tables
// have their languages: every locale of them ICU has data for (most the
// same as another), which V8 resolves a tag to by dropping subtags from
// its end. A tag of another language falls back to the default locale.
const LOCALES = [
  "en", "en-US", "en-GB", "en-AU", "en-CA", "en-IE", "en-IN", "en-NZ", "en-ZA",
  "de", "de-AT", "de-CH", "fr", "fr-BE", "fr-CA", "fr-CH", "it", "it-CH",
  "es", "es-419", "es-AR", "es-MX", "es-US", "pt", "pt-PT", "nl", "nl-BE",
  "sv", "da", "nb", "fi", "is", "pl", "cs", "sk", "hu", "ro", "bg", "hr", "sl",
  "el", "ru", "uk", "tr", "he", "ja", "ko", "zh", "zh-HK", "zh-TW", "id", "vi",
];
const SCRIPTS = ["Latn", "Cyrl", "Grek", "Hebr", "Arab", "Hans", "Hant", "Jpan", "Kore", "Deva"];
const REGIONS = [...codes2.map((c) => c.toUpperCase()), ...Array.from({ length: 1000 }, (_, i) => String(i).padStart(3, "0"))];
const available = (tag) => new Intl.DateTimeFormat(tag).resolvedOptions().locale === tag;
const locales = [...new Set(LOCALES.map((l) => l.split("-")[0]))]
  .flatMap((l) => [l, ...SCRIPTS.map((s) => `${l}-${s}`).filter(available)])
  .flatMap((base) => [base, ...REGIONS.map((r) => `${base}-${r}`).filter(available)]);
for (const l of LOCALES) if (!locales.includes(l)) throw new Error(`${l} is no locale of ICU's`);

const STYLES = [undefined, "full", "long", "medium", "short"];
const SEP = "|";
const ITEM = "~";

// V8's `format`, unlike `formatToParts`, gives U+202F as a plain space
const plain = (s) => s.replaceAll(" ", " ");

// an ICU pattern literal: quoted when it holds letters or a quote
const literal = (s) => {
  const text = plain(s);
  return /[A-Za-z']/.test(text) ? `'${text.replaceAll("'", "''")}'` : text;
};

const at = (month, day, hour) => Date.UTC(2024, month, day, hour, 4, 5, 6);
const PROBE = at(0, 2, 3); // Tuesday 2024-01-02 03:04:05.006

const HOURS = { h12: "h", h23: "H", h11: "K", h24: "k" };

// the parts of a formatter's output other than literals, by type
const part = (format, date, type) => plain(format.formatToParts(date).find((p) => p.type === type)?.value ?? "");

/** The pattern of a style and the name lists its text fields use. */
const patternOf = (locale, dateStyle, timeStyle) => {
  const options = dateStyle || timeStyle ? { dateStyle, timeStyle } : {};
  const format = new Intl.DateTimeFormat(locale, { ...options, timeZone: "UTC" });
  const resolved = format.resolvedOptions();
  if (resolved.calendar !== "gregory" || resolved.numberingSystem !== "latn") {
    throw new Error(`${locale}: ${resolved.calendar} calendar, ${resolved.numberingSystem} digits`);
  }
  const names = {};
  const list = (key, dates, type) => {
    names[key] = dates.map((d) => part(format, d, type));
    return key;
  };
  const tokens = format.formatToParts(PROBE).map(({ type, value }) => {
    const numeric = /^\d+$/.test(value);
    switch (type) {
      case "literal":
        return literal(value);
      case "year":
        return value.length === 2 ? "yy" : "y";
      case "month": {
        if (numeric) return value.length === 2 ? "MM" : "M";
        const months = Array.from({ length: 12 }, (_, m) => at(m, 2, 3));
        const short = new Intl.DateTimeFormat(locale, { month: "long", day: "numeric", timeZone: "UTC" });
        return list(part(short, PROBE, "month") === value ? "MMMM" : "MMM", months, "month");
      }
      case "day":
        return value.length === 2 ? "dd" : "d";
      case "weekday": {
        const days = Array.from({ length: 7 }, (_, d) => at(0, 7 + d, 3));
        const long = new Intl.DateTimeFormat(locale, { weekday: "long", day: "numeric", timeZone: "UTC" });
        return list(part(long, PROBE, "weekday") === value ? "EEEE" : "EEE", days, "weekday");
      }
      case "hour":
        return HOURS[resolved.hourCycle].repeat(value.length);
      case "minute":
        return value.length === 2 ? "mm" : "m";
      case "second":
        return value.length === 2 ? "ss" : "s";
      case "dayPeriod":
        return list("a", Array.from({ length: 24 }, (_, h) => at(0, 2, h)), "dayPeriod");
      case "timeZoneName":
        return timeStyle === "full" ? "zzzz" : "z";
      default:
        throw new Error(`${locale} ${dateStyle}/${timeStyle}: a ${type} part`);
    }
  });
  return { pattern: tokens.join(""), names };
};

// The zone names: of UTC, of offset 0, and templates of other offsets,
// where `{H}` is the hours and `{M}` the minutes, both of two digits.
const zoneName = (locale, timeZone, timeStyle) =>
  part(new Intl.DateTimeFormat(locale, { timeStyle, timeZone }), PROBE, "timeZoneName");
const template = (name) => {
  let n = 0;
  return name.replace(/\d+/g, (d) => (n++ === 0 ? (d.length === 2 ? "{HH}" : "{H}") : "{MM}"));
};
const zoneNames = (locale) => [
  zoneName(locale, "UTC", "long"),
  zoneName(locale, "UTC", "full"),
  zoneName(locale, "+00:00", "long"),
  zoneName(locale, "+00:00", "full"),
  ...["+03:00", "+05:30", "-03:00", "-05:30"].map((o) => template(zoneName(locale, o, "long"))),
  ...["+05:30", "-05:30"].map((o) => template(zoneName(locale, o, "full"))),
];

// render is the Go side's formatting, to check the tables against Intl.
const pad = (n, width) => String(n).padStart(width, "0");
const zoneText = (zone, offset, full) => {
  if (offset === "UTC") return zone[full ? 1 : 0];
  if (offset === 0) return zone[full ? 3 : 2];
  const abs = Math.abs(offset), h = Math.floor(abs / 60), m = abs % 60;
  const t = full ? zone[offset > 0 ? 8 : 9] : zone[4 + (offset < 0 ? 2 : 0) + (m === 0 ? 0 : 1)];
  return t.replace("{HH}", pad(h, 2)).replace("{H}", String(h)).replace("{MM}", pad(m, 2));
};
const render = (pattern, names, zone, date, offset) => {
  const d = new Date(date + (offset === "UTC" ? 0 : offset * 60000));
  const fields = {
    y: () => String(d.getUTCFullYear()),
    yy: () => pad(d.getUTCFullYear() % 100, 2),
    M: () => String(d.getUTCMonth() + 1),
    MM: () => pad(d.getUTCMonth() + 1, 2),
    MMM: () => names.MMM[d.getUTCMonth()],
    MMMM: () => names.MMMM[d.getUTCMonth()],
    d: () => String(d.getUTCDate()),
    dd: () => pad(d.getUTCDate(), 2),
    EEE: () => names.EEE[d.getUTCDay()],
    EEEE: () => names.EEEE[d.getUTCDay()],
    a: () => names.a[d.getUTCHours()],
    h: () => String(d.getUTCHours() % 12 || 12),
    hh: () => pad(d.getUTCHours() % 12 || 12, 2),
    H: () => String(d.getUTCHours()),
    HH: () => pad(d.getUTCHours(), 2),
    K: () => String(d.getUTCHours() % 12),
    KK: () => pad(d.getUTCHours() % 12, 2),
    k: () => String(d.getUTCHours() || 24),
    kk: () => pad(d.getUTCHours() || 24, 2),
    m: () => String(d.getUTCMinutes()),
    mm: () => pad(d.getUTCMinutes(), 2),
    s: () => String(d.getUTCSeconds()),
    ss: () => pad(d.getUTCSeconds(), 2),
    z: () => zoneText(zone, offset, false),
    zzzz: () => zoneText(zone, offset, true),
  };
  return pattern.replace(/'((?:[^']|'')*)'|([A-Za-z])\2*/g, (token, quoted) =>
    quoted !== undefined ? quoted.replaceAll("''", "'") : fields[token](),
  );
};

// dates over every month, weekday and hour, and two-digit years
const CHECKED = [
  ...Array.from({ length: 12 }, (_, m) => at(m, 1 + m * 2, m * 2)),
  ...Array.from({ length: 24 }, (_, h) => at(5, 1 + (h % 7), h)),
  Date.UTC(2005, 6, 4, 23, 59, 59),
  Date.UTC(1999, 11, 31, 12, 0, 0),
];
const OFFSETS = [["UTC", "UTC"], [0, "+00:00"], [60, "+01:00"], [-210, "-03:30"], [345, "+05:45"], [-600, "-10:00"]];

const tableOf = (locale) => {
  const names = {};
  const patterns = STYLES.flatMap((ds) =>
    STYLES.map((ts) => {
      const p = patternOf(locale, ds, ts);
      for (const [key, list] of Object.entries(p.names)) {
        if (names[key] !== undefined && names[key].join(ITEM) !== list.join(ITEM)) {
          throw new Error(`${locale}: two lists of ${key}`);
        }
        names[key] = list;
      }
      return { ...p, ds, ts };
    }),
  );
  const zone = zoneNames(locale);
  for (const { pattern, ds, ts } of patterns) {
    for (const date of CHECKED) {
      for (const [offset, timeZone] of ts === "full" || ts === "long" ? OFFSETS : OFFSETS.slice(0, 1)) {
        const options = ds || ts ? { dateStyle: ds, timeStyle: ts } : {};
        const want = new Intl.DateTimeFormat(locale, { ...options, timeZone }).format(date);
        const got = render(pattern, names, zone, date, offset);
        if (got !== want) throw new Error(`${locale} ${ds}/${ts} ${timeZone} ${pattern}: ${JSON.stringify(got)} ${[...got].map((c) => c.charCodeAt(0).toString(16))}, Intl ${JSON.stringify(want)} ${[...want].map((c) => c.charCodeAt(0).toString(16))}`);
      }
    }
  }
  const fields = [
    patterns.map((p) => p.pattern).join(ITEM),
    ...["MMM", "MMMM", "EEE", "EEEE", "a"].map((key) => (names[key] ?? []).join(ITEM)),
    zone.join(ITEM),
  ];
  for (const f of fields) if (f.includes(SEP) || f.includes("\n") || f.includes('"') || f.includes("\\")) throw new Error(`${locale}: ${f}`);
  return fields.join(SEP);
};

const seen = new Map();
const lines = locales.map((locale) => {
  const table = tableOf(locale);
  const same = seen.get(table);
  if (same !== undefined) return `${locale}=@${same}`;
  seen.set(table, locale);
  return `${locale}=${table}`;
});

// The names of named zones, which ICU takes from their metazone at the
// date. Each zone is named at two probes, in winter and in summer, and the
// Go side names a date by the probe of its offset; a name the offset would
// give anyway is left out. Zones named alike in every locale share a group.
const PROBES = [Date.UTC(2024, 0, 15, 12), Date.UTC(2024, 6, 15, 12)];
const offsetAt = (timeZone, date) => {
  const name = part(new Intl.DateTimeFormat("en", { timeZoneName: "longOffset", timeZone }), date, "timeZoneName");
  const m = /GMT([+-−])(\d\d):(\d\d)/.exec(name);
  return m === null ? 0 : (m[1] === "+" ? 1 : -1) * (Number(m[2]) * 60 + Number(m[3]));
};
const zoneOffsets = new Map(zoneIds.map((id) => [id, PROBES.map((d) => offsetAt(id, d))]));
const tablesOf = new Map(lines.map((line) => line.split("=")).map(([l, t]) => [l, t]));
const zoneTemplates = (locale) => {
  let t = tablesOf.get(locale);
  if (t.startsWith("@")) t = tablesOf.get(t.slice(1));
  return t.split(SEP)[6].split(ITEM);
};
// names[locale][style][zone]: an entry `winter` or `winter~summer`
const zoneNamesOf = (locale) => {
  const templates = zoneTemplates(locale);
  return [["long", false], ["full", true]].map(([timeStyle, full]) =>
    zoneIds.map((timeZone) => {
      const format = new Intl.DateTimeFormat(locale, { timeStyle, timeZone });
      const [w, s] = PROBES.map((d, i) => {
        const name = part(format, d, "timeZoneName");
        return name === zoneText(templates, zoneOffsets.get(timeZone)[i], full) ? "" : name;
      });
      for (const n of [w, s]) if (/[|~\n`]/.test(n)) throw new Error(`${locale} ${timeZone}: ${n}`);
      return w === s ? w : `${w}${ITEM}${s}`;
    }),
  );
};
const zoneNameTable = locales.map((locale) => [locale, zoneNamesOf(locale)]);
const signatures = new Map();
zoneIds.forEach((id, z) => {
  const signature = zoneNameTable.map(([, styles]) => styles.map((names) => names[z]).join(SEP)).join("\n");
  if (/^[|\n]*$/.test(signature)) return;
  signatures.set(signature, [...(signatures.get(signature) ?? []), z]);
});
const groups = [...signatures.values()];
const zoneLines = (style) => {
  const seenNames = new Map();
  return zoneNameTable.map(([locale, styles]) => {
    const line = groups.map((zs) => styles[style][zs[0]]).join(SEP);
    const same = seenNames.get(line);
    if (same !== undefined) return `${locale}=@${same}`;
    seenNames.set(line, locale);
    return `${locale}=${line}`;
  });
};
const zoneGroups = groups.map((zs) => zs.map((z) => zoneIds[z]).join(",")).join(";");
const shortZoneLines = zoneLines(0), longZoneLines = zoneLines(1);

// check the Go side's naming at other dates against Intl
const nameAt = (locale, style, timeZone, date) => {
  const g = groups.findIndex((zs) => zs.some((z) => zoneIds[z] === timeZone));
  const [, styles] = zoneNameTable.find(([l]) => l === locale);
  const offset = offsetAt(timeZone, date);
  const [w, s = w] = g < 0 ? [""] : styles[style][groups[g][0]].split(ITEM);
  const probe = zoneOffsets.get(timeZone).indexOf(offset);
  return (probe === 0 ? w : probe === 1 ? s : "") || zoneText(zoneTemplates(locale), offset, style === 1);
};
let misnamed = 0;
for (const locale of ["en", "en-US", "en-GB", "de", "fr", "ja", "zh-TW", "es-MX"]) {
  for (const timeZone of zoneIds) {
    for (const date of [Date.UTC(2023, 1, 1), Date.UTC(2025, 7, 1)]) {
      ["long", "full"].forEach((timeStyle, style) => {
        const want = part(new Intl.DateTimeFormat(locale, { timeStyle, timeZone }), date, "timeZoneName");
        if (nameAt(locale, style, timeZone, date) !== want && misnamed++ < 10) {
          console.log(`misnamed: ${locale} ${timeZone} ${new Date(date).toISOString()} ${timeStyle}: ${nameAt(locale, style, timeZone, date)}, Intl ${want}`);
        }
      });
    }
  }
}
console.log(`${misnamed} zone names at other dates differ from Intl's`);

const go = `// Code generated by tools/go-intl/gen.mjs; DO NOT EDIT.

package flickrt

// As Node ${process.versions.node} (ICU ${process.versions.icu}, tz ${process.versions.tz}) answers.
const (
	// languageAliasData and regionAliasData are the subtags ICU replaces: \`from>to\`.
	languageAliasData = "${languageAliases}"
	regionAliasData   = "${regionAliases}"
	// zoneData is the zones V8 accepts: \`name\`, or \`name>resolved\` for one it resolves to another.
	zoneData = "${zones}"
	// intlData is a line per locale: \`tag=@other\` for the data of another, or \`tag=\` and, apart by
	// "${SEP}": the patterns of the date styles none, full, long, medium and short, each with the time
	// styles in that order; the names of MMM, MMMM, EEE and EEEE, and of the day period of each hour;
	// the names of UTC, short and long, of offset 0, then templates of the short names of whole and
	// other positive offsets, of negative ones, and of the long names of positive and negative ones.
	intlData = \`${lines.join("\n")}\`
	// zoneGroupData is the named zones with names of their own, by group: the zones of a group,
	// apart by ",", are named alike in every locale.
	zoneGroupData = "${zoneGroups}"
	// shortZoneData and longZoneData are a line per locale, as intlData: the short and long name
	// of each group, apart by "${SEP}", at a winter and a summer date: \`winter${ITEM}summer\` or, for
	// the same, \`name\`; empty where the offset's name is the zone's.
	shortZoneData = \`${shortZoneLines.join("\n")}\`
	longZoneData = \`${longZoneLines.join("\n")}\`
)
`;
writeFileSync(out, go);
console.log(`${out}: ${go.length} bytes, ${locales.length} locales, ${seen.size} tables, ${groups.length} zone groups`);
