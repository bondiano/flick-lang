// A defect as inert text: building it never runs code of the value.
import { constructorName } from "../tags.ts";

const LIMIT = 200;

const text = (s: string): string => (s.length > LIMIT ? `${s.slice(0, LIMIT)}…` : s);

const own = (value: object, key: string): string | undefined => {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  return descriptor && "value" in descriptor && typeof descriptor.value === "string" ? text(descriptor.value) : undefined;
};

/** The message of an error, a primitive, or the constructor of a tagged value. */
export const describeDefect = (defect: unknown): string => {
  switch (typeof defect) {
    case "string":
      return text(defect);
    case "number":
    case "boolean":
    case "bigint":
    case "undefined":
      return String(defect);
    case "symbol":
      return "<symbol>";
    case "function":
      return "<function>";
    default:
  }
  if (typeof defect !== "object" || defect === null) return "null";
  if (defect instanceof Error) return own(defect, "message") ?? "<error>";
  const name = own(defect, "_tag");
  return name === undefined ? "<object>" : constructorName(name);
};
