// `llms-full.txt`: the whole tour as one text, each lesson with its program.
import type { APIRoute } from "astro";
import { getCollection } from "astro:content";
import { lessons } from "../../tour/load.ts";
import { tour, type Page } from "../llms/text.ts";

export const GET: APIRoute = async () => {
  const pages = new Map<string, Page>(
    (await getCollection("docs"))
      .filter((entry) => entry.id.startsWith("tour/"))
      .map((entry) => [entry.id, { slug: entry.id, title: entry.data.title, body: entry.body ?? "" }]),
  );
  return new Response(tour(lessons(), pages), { headers: { "Content-Type": "text/plain; charset=utf-8" } });
};
