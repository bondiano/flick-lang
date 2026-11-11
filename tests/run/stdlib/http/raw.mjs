// Requests the client of HttpApi would never send.
export const post = async (url, body) => {
  const r = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body });
  return `${r.status} ${await r.text()}`;
};
export const get = async (url) => {
  const r = await fetch(url);
  return `${r.status} ${r.headers.get("content-type")} ${(await r.text()).length > 0}`;
};
export const text = async (url) => (await fetch(url)).text();
/** Starts a request without waiting for it; `wait` gives its outcome. */
export const start = (url) => fetch(url).then(async (r) => `${r.status} ${await r.text()}`).catch((e) => `failed: ${e.message}`);
export const wait = (pending) => pending;
export const head = async (url) => String((await fetch(url, { method: "HEAD" })).status);
