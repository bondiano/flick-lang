import { readFileSync } from "node:fs";
import { expect, type Page, test } from "@playwright/test";
import { ready } from "./helpers.ts";

const examples = new URL("../src/playground/examples/", import.meta.url);
const replies = JSON.parse(readFileSync(new URL("pokeapi.fetch.json", examples), "utf8")) as Record<string, unknown>;
const output = (id: string): string => readFileSync(new URL(`${id}.stdout`, examples), "utf8").trim();

const choose = async (page: Page, name: string): Promise<void> => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name, exact: true }).click();
  await expect(page.locator(".fp-status")).toContainText("Type-checked");
};

for (const profile of ["Debug", "Release"]) {
  test(`PokéAPI requests run explicitly and serialize decoded records (${profile})`, async ({ page }) => {
    const requests: Array<string> = [];
    await page.route("https://pokeapi.co/api/v2/pokemon/**", async (route) => {
      const url = route.request().url();
      requests.push(url);
      expect(route.request().method()).toBe("GET");
      expect(Object.hasOwn(replies, url)).toBe(true);
      await route.fulfill({ json: replies[url] });
    });
    await choose(page, "PokéAPI: HTTP and JSON");
    expect(requests).toEqual([]);
    await page.getByRole("button", { name: profile, exact: true }).click();
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await expect(page.locator(".fp-output")).toHaveText(output("pokeapi"), { timeout: 60_000 });
    expect(requests.sort()).toEqual(Object.keys(replies).sort());
  });
}

for (const failure of ["schema", "json", "status", "network"] as const) {
  test(`PokéAPI explains a ${failure} failure`, async ({ page }) => {
    await page.route("https://pokeapi.co/api/v2/pokemon/**", async (route) => {
      if (failure === "network") await route.abort("failed");
      else if (failure === "status") await route.fulfill({ status: 404, body: "Not Found" });
      else if (failure === "json") await route.fulfill({ contentType: "application/json", body: "not json" });
      else await route.fulfill({ json: { id: 25, name: "pikachu", height: 4, weight: 60, types: [{ slot: 1, type: { name: 42 } }] } });
    });
    await choose(page, "PokéAPI: HTTP and JSON");
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await expect(page.locator(".fp-output")).toContainText(failure === "schema" ? "Invalid Pokemon data" : "Request failed", { timeout: 60_000 });
    await expect(page.getByRole("button", { name: "Run", exact: true })).toBeEnabled();
  });
}

test("a pending PokéAPI request can be stopped", async ({ page }) => {
  const { promise, resolve } = Promise.withResolvers<void>();
  const response = Promise.withResolvers<void>();
  await page.route("https://pokeapi.co/api/v2/pokemon/**", async (route) => {
    resolve();
    // Leave the response pending until the worker is terminated by Stop.
    await response.promise;
    await route.abort("timedout").catch(() => {});
  });
  await choose(page, "PokéAPI: HTTP and JSON");
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await promise;
  try {
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
