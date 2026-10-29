/** A request: its status and body, or `refused` when nothing listens. */
export const call = async (method, url, body) => {
  try {
    const init = body === "" ? { method } : { method, headers: { "content-type": "application/json" }, body };
    const r = await fetch(url, init);
    return `${r.status} ${await r.text()}`;
  } catch {
    return "refused";
  }
};
