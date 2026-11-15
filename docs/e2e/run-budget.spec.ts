import { expect, test } from "@playwright/test";
import { ready, replaceSource } from "./helpers.ts";

test("compiler preparation does not consume the program's execution budget", async ({ page }) => {
  await page.addInitScript(() => {
    const post = Worker.prototype.postMessage;
    Object.defineProperty(Worker.prototype, "postMessage", {
      value: function (this: Worker, message: unknown, ...options: Array<unknown>): void {
        const send = (): void => { Reflect.apply(post, this, [message, ...options]); };
        if (typeof message === "object" && message !== null && "action" in message && message.action === "build") {
          setTimeout(send, 11_000);
        } else send();
      },
    });
  });
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText("Hello from Flick", { timeout: 30_000 });
  await expect(page.locator(".fp-exit")).toHaveText("Exit code 0 · debug");
});

test("the ten-second execution limit still terminates a non-yielding worker", async ({ page }) => {
  await page.addInitScript(() => {
    const executions = new WeakSet<Worker>();
    let terminated = 0;
    const post = Worker.prototype.postMessage;
    const terminate = Worker.prototype.terminate;
    Object.defineProperty(window, "executionWorkersTerminated", { get: () => terminated });
    Object.defineProperty(Worker.prototype, "postMessage", {
      value: function (this: Worker, message: unknown, ...options: Array<unknown>): void {
        if (typeof message === "object" && message !== null && "files" in message && "entry" in message) executions.add(this);
        Reflect.apply(post, this, [message, ...options]);
      },
    });
    Object.defineProperty(Worker.prototype, "terminate", {
      value: function (this: Worker): void {
        if (executions.has(this)) terminated += 1;
        Reflect.apply(terminate, this, []);
      },
    });
  });
  await page.goto("/flick-lang/play/");
  await ready(page);
  await replaceSource(page, `module Main exposing (main)
main : Unit ->{} Unit
let main () =
  let mut value = 0
  while true do value = value + 1
`);
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-exit")).toHaveText("Stopped after 10 seconds · debug", { timeout: 30_000 });
  await expect(page.getByRole("button", { name: "Run", exact: true })).toBeEnabled();
  expect(await page.evaluate(() => Reflect.get(window, "executionWorkersTerminated"))).toBe(1);
});
