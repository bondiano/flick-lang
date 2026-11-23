import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { clearDraft, restoreSource, saveDraft } from "../src/playground/drafts.ts";

const original = "main : Unit ->{Host.Console} Unit";
const key = "flick.lesson.tour.loops.draft";

const page = (lesson = "tour/loops", source = original): JSDOM => {
  const dom = new JSDOM(
    `<div id="flick-playground" data-lesson="${lesson}"></div>` +
    `<script id="flick-playground-examples" type="application/json">${JSON.stringify([{ source }])}</script>`,
    { url: "https://example.com/tour/loops/" },
  );
  Object.assign(globalThis, { document: dom.window.document, localStorage: dom.window.localStorage, location: dom.window.location, history: dom.window.history });
  return dom;
};

test("a legacy lesson draft cannot replace updated lesson code", async () => {
  const dom = page();
  try {
    localStorage.setItem(key, "main : Unit ->{} Unit");
    assert.deepEqual(await restoreSource(), { source: "", status: "" });
    assert.equal(localStorage.getItem(key), "main : Unit ->{} Unit", "do not delete the reader's old code");
  } finally { dom.window.close(); }
});

test("lesson drafts restore only against the same original, across locales", async () => {
  const dom = page();
  try {
    saveDraft("edited");
    const saved = localStorage.getItem(key);
    assert.deepEqual(await restoreSource(), { source: "edited", status: "" });
    document.getElementById("flick-playground")!.dataset.locale = "ru";
    assert.equal((await restoreSource()).source, "edited");
    document.getElementById("flick-playground-examples")!.textContent = JSON.stringify([{ source: "updated" }]);
    assert.deepEqual(await restoreSource(), { source: "", status: "" });
    assert.equal(localStorage.getItem(key), saved);
    saveDraft("new edit");
    assert.equal((await restoreSource()).source, "new edit");
    clearDraft();
    assert.equal(localStorage.getItem(key), null);
  } finally { dom.window.close(); }
});

test("the gallery retains its existing plain-text drafts", async () => {
  const dom = page("");
  try {
    localStorage.setItem("flick.playground.draft.v1", "legacy gallery");
    assert.equal((await restoreSource()).source, "legacy gallery");
    saveDraft("gallery edit");
    assert.equal((await restoreSource()).source, "gallery edit");
    clearDraft();
    assert.equal((await restoreSource()).source, "");
  } finally { dom.window.close(); }
});

test("malformed lesson persistence does not replace the source", async () => {
  const dom = page();
  try {
    for (const value of ["null", "{}", '{"original":1,"source":"bad"}', '{"original":"' + original + '","source":1}']) {
      localStorage.setItem(key, value);
      assert.equal((await restoreSource()).source, "");
    }
  } finally { dom.window.close(); }
});
