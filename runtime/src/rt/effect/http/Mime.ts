// `Http.Mime`: media types by file extension and back, over the table of
// the `mime` package that Effect's `Mime` reads, with its lookups.
import { type Option, none, some } from "../../../core/data.ts";
import { mimeTypes } from "../../mime_types.ts";

interface Tables {
  readonly extensionToType: Map<string, string>;
  readonly typeToExtension: Map<string, string>;
  readonly typeToExtensions: Map<string, ReadonlyArray<string>>;
}

let tables: Tables | undefined;

// built on the first lookup, so a program that never asks pays nothing
const getTables = (): Tables => {
  if (tables !== undefined) return tables;
  const built: Tables = { extensionToType: new Map(), typeToExtension: new Map(), typeToExtensions: new Map() };
  for (const line of mimeTypes.split("\n")) {
    const [type = "", ...extensions] = line.split(" ");
    const all = new Set<string>();
    extensions.forEach((entry, index) => {
      // a starred extension belongs to another type when looked up by extension
      const extension = entry.startsWith("*") ? entry.slice(1) : entry;
      all.add(extension);
      if (index === 0) built.typeToExtension.set(type, extension);
      if (extension === entry) built.extensionToType.set(extension, type);
    });
    built.typeToExtensions.set(type, [...all].toSorted());
  }
  tables = built;
  return built;
};

const found = <A>(value: A | undefined): Option<A> => (value === undefined ? none : some(value));

/** The media type without parameters, as the tables key it. */
const essence = (type: string): string => (type.split(";")[0] ?? "").trim().toLowerCase();

/** `Http.Mime.get_type`: a path with a directory needs a dot in its last part. */
export const getType = (path: string): Option<string> => {
  const last = path.replace(/^.*[/\\]/s, "").toLowerCase();
  const extension = last.replace(/^.*\./s, "").toLowerCase();
  const hasPath = last.length < path.length;
  const hasDot = extension.length < last.length - 1;
  if (!hasDot && hasPath) return none;
  return found(getTables().extensionToType.get(extension));
};

export const getExtension = (type: string): Option<string> => found(getTables().typeToExtension.get(essence(type)));

export const flickGetAllExtensions = (type: string): Option<ReadonlyArray<string>> =>
  found(getTables().typeToExtensions.get(essence(type)));
