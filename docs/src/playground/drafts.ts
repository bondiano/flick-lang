/** Browser persistence and URL encoding for a single-file playground. */
const key = (): string => document.getElementById("flick-playground")?.dataset.lesson === "true"
  ? "flick.lesson.signatures.01.draft"
  : "flick.playground.draft.v1";
const maxSourceBytes = 64 * 1024;

const bytes = async (stream: ReadableStream<Uint8Array>): Promise<Uint8Array> => {
  const reader = stream.getReader();
  const parts: Array<Uint8Array> = [];
  let length = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > maxSourceBytes) throw new Error("Shared code is too large");
      parts.push(value);
    }
  } finally { reader.releaseLock(); }
  const result = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) { result.set(part, offset); offset += part.length; }
  return result;
};
const base64url = (value: Uint8Array): string =>
  btoa(Array.from(value, (byte) => String.fromCharCode(byte)).join("")).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
const unbase64url = (text: string): Uint8Array => {
  if (!/^[A-Za-z0-9_-]+$/.test(text)) throw new Error("Invalid shared code");
  const binary = atob(text.replaceAll("-", "+").replaceAll("_", "/"));
  return Uint8Array.from(binary, (char) => char.charCodeAt(0));
};
const stored = (): string => {
  try { return localStorage.getItem(key()) ?? ""; } catch { return ""; }
};

export const restoreSource = async (): Promise<{ source: string; status: string }> => {
  const value = new URLSearchParams(location.hash.slice(1)).get("code");
  if (value === null) return { source: stored(), status: "" };
  try {
    if (value.length > maxSourceBytes) throw new Error("Shared code is too large");
    const encoded = unbase64url(value);
    const buffer = new ArrayBuffer(encoded.length);
    new Uint8Array(buffer).set(encoded);
    const data = await bytes(new Blob([buffer]).stream().pipeThrough(new DecompressionStream("gzip")));
    if (data.length > maxSourceBytes) throw new Error("Shared code is too large");
    return { source: new TextDecoder("utf-8", { fatal: true }).decode(data), status: "shared" };
  } catch {
    return { source: stored(), status: "invalid-link" };
  }
};

export const saveDraft = (source: string): void => {
  try { localStorage.setItem(key(), source); } catch { /* Editing still works without storage. */ }
};
export const clearDraft = (): void => {
  try { localStorage.removeItem(key()); } catch { /* A disabled store is already empty. */ }
  if (location.hash.startsWith("#code=")) history.replaceState(null, "", location.pathname + location.search);
};

export const shareSource = async (source: string): Promise<string> => {
  const input = new TextEncoder().encode(source);
  if (input.length > maxSourceBytes) return "too-large";
  try {
    const compressed = await bytes(new Blob([input]).stream().pipeThrough(new CompressionStream("gzip")));
    const url = new URL(location.href);
    url.hash = new URLSearchParams({ code: base64url(compressed) }).toString();
    history.replaceState(null, "", url);
    await navigator.clipboard.writeText(url.href);
    return "copied";
  } catch {
    return location.hash.startsWith("#code=") ? "address-bar" : "unavailable";
  }
};

export const downloadSource = (source: string): void => {
  const url = URL.createObjectURL(new Blob([source], { type: "text/plain;charset=utf-8" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = "Main.fl";
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
};
