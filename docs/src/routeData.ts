import { defineRouteMiddleware } from "@astrojs/starlight/route-data";

const plain = (text: string): string => text.replaceAll("`", "");

// A title may mark code with backticks. A browser tab and a link preview cannot show code.
export const onRequest = defineRouteMiddleware(({ locals }) => {
  const route = locals.starlightRoute;
  route.head = route.head.map((entry) => {
    if (entry.tag === "title" && entry.content !== undefined) return { ...entry, content: plain(entry.content) };
    if (entry.tag === "meta" && entry.attrs?.property === "og:title" && typeof entry.attrs.content === "string") {
      return { ...entry, attrs: { ...entry.attrs, content: plain(entry.attrs.content) } };
    }
    return entry;
  });
  for (const link of [route.pagination.prev, route.pagination.next]) {
    if (link !== undefined) link.label = plain(link.label);
  }
});
