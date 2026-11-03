// `vite` serves the client and builds it with `flick build`; in development
// the API goes to the Go server (`flick run dev` starts both).
import { flickVite } from "@flick-lang/astro/vite";
import { defineConfig } from "vite";

const api = `http://127.0.0.1:${process.env.API_PORT ?? "8000"}`;

export default defineConfig({
  plugins: [flickVite()],
  server: { proxy: { "/api": api } },
});
