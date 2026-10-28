// `flick dev` starts Vite for the page and the server entry beside it; the
// page's calls of `server fn` go to that server.
import { flickVite } from "@flick-lang/astro/vite";
import { defineConfig } from "vite";

const server = `http://127.0.0.1:${process.env.PORT ?? "3000"}`;

export default defineConfig({
  plugins: [flickVite()],
  server: { proxy: { "/_flick": server } },
});
