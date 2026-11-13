import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { ready, replaceSource } from "./helpers.ts";

test("GADT row equations run in the WASM compiler and recover after an invalid edit", async ({ page }) => {
  const source = "// 🦔\n" + readFileSync(new URL("../../tests/run/codegen/gadt_rows.fl", import.meta.url), "utf8");
  await page.goto("/flick-lang/play/");
  await ready(page);
  await replaceSource(page, source);
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText(/42\s+107\s+9\s+17\s+closed/, { timeout: 60_000 });

  await replaceSource(page, source.replace("Cmd r e a ->{r; e} a", "Cmd r e a -> a"));
  await expect(page.locator(".fp-status")).toContainText("Errors", { timeout: 30_000 });
  await expect(page.locator(".fp-problems-panel")).toContainText("FLICK-E001");
  await expect(page.locator(".cm-lintRange-error").first()).toHaveText("Db.get key");

  await replaceSource(page, source);
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText(/42\s+107\s+9\s+17\s+closed/, { timeout: 60_000 });
});
