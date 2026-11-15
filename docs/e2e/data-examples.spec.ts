import { readFileSync } from "node:fs";
import { defineNetworkFixture, type NetworkFixture } from "@msw/playwright";
import { type BrowserContext, expect, type Page, test as base } from "@playwright/test";
import { http, HttpResponse, type JsonBodyType } from "msw";
import { ready } from "./helpers.ts";

const mockPokeApi = (context: BrowserContext): { network: NetworkFixture; unexpected: Array<string> } => {
  const unexpected: Array<string> = [];
  const network = defineNetworkFixture({
    context,
    skipAssetRequests: false,
    handlers: [http.all("https://pokeapi.co/*", ({ request }) => {
      unexpected.push(request.url);
      return HttpResponse.error();
    })],
  });
  return { network, unexpected };
};

const test = base.extend<{ network: NetworkFixture }>({
  network: [async ({ context }, use) => {
    const { network, unexpected } = mockPokeApi(context);
    await network.enable();
    try {
      await use(network);
      expect(unexpected, "Every PokéAPI request must have a local MSW handler").toEqual([]);
    } finally {
      await network.disable();
    }
  }, { auto: true }],
});

test.use({ serviceWorkers: "block" });

const pokemon = "https://pokeapi.co/api/v2/pokemon/:name";
const examples = new URL("../src/playground/examples/", import.meta.url);
const replies = JSON.parse(readFileSync(new URL("pokeapi.fetch.json", examples), "utf8")) as Record<string, JsonBodyType>;
const output = (id: string): string => readFileSync(new URL(`${id}.stdout`, examples), "utf8").trim();

const choose = async (page: Page, name: string): Promise<void> => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name, exact: true }).click();
  await expect(page.locator(".fp-status")).toContainText("Type-checked");
};

base("unmocked PokéAPI asset URLs cannot reach the network", async ({ context, page }) => {
  const url = "https://pokeapi.co/api/v2/pokemon/pikachu.json";
  const fallbacks: Array<string> = [];
  await context.route("https://pokeapi.co/**", async (route) => {
    fallbacks.push(route.request().url());
    await route.abort();
  });
  const { network, unexpected } = mockPokeApi(context);
  await network.enable();
  try {
    await page.goto("/flick-lang/");
    const blocked = await page.evaluate(async (url) => {
      try {
        await fetch(url);
        return false;
      } catch {
        return true;
      }
    }, url);
    expect(blocked).toBe(true);
    expect(unexpected).toEqual([url]);
    expect(fallbacks).toEqual([]);
  } finally {
    await network.disable();
  }
});

for (const profile of ["Debug", "Release"]) {
  test(`PokéAPI requests run explicitly and serialize decoded records (${profile})`, async ({ page, network }) => {
    const requests: Array<string> = [];
    const methods: Array<string> = [];
    network.use(http.get(pokemon, ({ request }) => {
      requests.push(request.url);
      methods.push(request.method);
      return Object.hasOwn(replies, request.url)
        ? HttpResponse.json(replies[request.url])
        : new HttpResponse(null, { status: 404 });
    }));
    await choose(page, "PokéAPI: HTTP and JSON");
    expect(requests).toEqual([]);
    await page.getByRole("button", { name: profile, exact: true }).click();
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await expect(page.locator(".fp-output")).toHaveText(output("pokeapi"), { timeout: 60_000 });
    expect(requests.sort()).toEqual(Object.keys(replies).sort());
    expect(methods).toEqual(Object.keys(replies).map(() => "GET"));
  });
}

for (const failure of ["schema", "json", "status", "network"] as const) {
  test(`PokéAPI explains a ${failure} failure`, async ({ page, network }) => {
    network.use(http.get(pokemon, () => {
      if (failure === "network") return HttpResponse.error();
      if (failure === "status") return new HttpResponse("Not Found", { status: 404 });
      if (failure === "json") return HttpResponse.text("not json", { headers: { "Content-Type": "application/json" } });
      return HttpResponse.json({ id: 25, name: "pikachu", height: 4, weight: 60, types: [{ slot: 1, type: { name: 42 } }] });
    }));
    await choose(page, "PokéAPI: HTTP and JSON");
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await expect(page.locator(".fp-output")).toContainText(failure === "schema" ? "Invalid Pokemon data" : "Request failed", { timeout: 60_000 });
    await expect(page.getByRole("button", { name: "Run", exact: true })).toBeEnabled();
  });
}

test("a pending PokéAPI request can be stopped", async ({ page, network }) => {
  const { promise, resolve } = Promise.withResolvers<void>();
  const response = Promise.withResolvers<void>();
  network.use(http.get(pokemon, async () => {
    resolve();
    // Leave the response pending until the worker is terminated by Stop.
    await response.promise;
    return HttpResponse.error();
  }));
  try {
    await choose(page, "PokéAPI: HTTP and JSON");
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await promise;
    await page.getByRole("button", { name: "Stop", exact: true }).click();
    await expect(page.getByRole("button", { name: "Run", exact: true })).toBeEnabled();
    await expect(page.locator(".fp-exit")).toHaveText("Stopped · debug");
    await expect(page.locator(".fp-output")).not.toContainText("Serialized:");
  } finally {
    response.resolve();
  }
});

for (const [id, name] of [
  ["json-validation", "Validate and serialize JSON"],
  ["sales-report", "Group and sort orders"],
  ["state-machine", "An order state machine"],
] as const) {
  test(`${name} runs in the browser`, async ({ page }) => {
    await choose(page, name);
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await expect(page.locator(".fp-output")).toHaveText(output(id), { timeout: 60_000 });
  });
}
