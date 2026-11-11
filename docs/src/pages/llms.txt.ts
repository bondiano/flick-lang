// `llms.txt`: what Flick is and a link to each English page, in the order of the sidebar.
import type { APIRoute } from "astro";
import { getCollection } from "astro:content";
import { sidebar } from "../../astro.sidebar.mjs";
import { index, type Page } from "../llms/text.ts";

export const GET: APIRoute = async ({ site }) => {
  const pages = new Map<string, Page>(
    (await getCollection("docs"))
      .filter((entry) => !entry.id.startsWith("ru/"))
      .map((entry) => [entry.id, { slug: entry.id, title: entry.data.title, description: entry.data.description }]),
  );
  const url = (path: string): string => new URL(`${import.meta.env.BASE_URL.replace(/\/$/, "")}/${path}`, site).href;
  return new Response(index(sidebar, pages, url), { headers: { "Content-Type": "text/plain; charset=utf-8" } });
};
