// Direct GETs for measuring a server without fetch's Request/Response and
// Web Stream overhead. Redirects remain responses, not extra requests.
import { Agent as HttpAgent, get as httpGet } from "node:http";
import { Agent as HttpsAgent, get as httpsGet } from "node:https";
import { brotliDecompressSync, gunzipSync, inflateSync } from "node:zlib";

const decoder = new TextDecoder();
const decompress = { gzip: gunzipSync, "x-gzip": gunzipSync, deflate: inflateSync, br: brotliDecompressSync };
const textOf = (bytes, encoding) => {
  const codings = encoding?.toLowerCase().split(",").map((s) => s.trim()) ?? [];
  if (codings.every((coding) => Object.hasOwn(decompress, coding))) {
    for (const coding of codings.reverse()) bytes = decompress[coding](bytes);
  }
  return decoder.decode(bytes);
};

export class HttpClient {
  #agents = { "http:": new HttpAgent({ keepAlive: true }), "https:": new HttpsAgent({ keepAlive: true }) };

  async get(url, { signal } = {}) {
    const target = new URL(url);
    const agent = this.#agents[target.protocol];
    if (!agent) throw new TypeError(`unsupported load protocol: ${target.protocol}`);
    return new Promise((resolve, reject) => {
      const fail = (error) => reject(signal?.aborted ? signal.reason : error);
      const request = (target.protocol === "https:" ? httpsGet : httpGet)(target, {
        agent,
        signal,
        // Match fetch's GET headers, so the server receives the same work.
        headers: {
          host: target.host,
          connection: "keep-alive",
          accept: "*/*",
          "accept-language": "*",
          "sec-fetch-mode": "cors",
          "user-agent": "node",
          "accept-encoding": target.protocol === "https:" ? "br, gzip, deflate" : "gzip, deflate",
        },
      }, (response) => {
        const chunks = [];
        response.on("data", (chunk) => chunks.push(chunk));
        response.on("error", fail);
        response.on("end", () => {
          try {
            resolve({ status: response.statusCode, body: textOf(Buffer.concat(chunks), response.headers["content-encoding"]) });
          } catch (error) {
            fail(error);
          }
        });
      });
      request.on("error", fail);
    });
  }

  close() {
    for (const agent of Object.values(this.#agents)) agent.destroy();
  }
}
