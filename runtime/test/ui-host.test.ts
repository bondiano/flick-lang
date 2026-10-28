import assert from "node:assert/strict";
import { test } from "node:test";

import { Option } from "effect";
import { renderToStaticMarkup } from "react-dom/server";

import { tag } from "../src/tags.ts";
import { uiDiv, uiHeadingAt, uiRenderStatic, uiText, uiWithHost } from "../src/ui/react/index.ts";

const none = {
  id: Option.none(),
  class_name: Option.none(),
  role: Option.none(),
  aria_label: Option.none(),
  aria_describedby: Option.none(),
  aria_hidden: Option.none(),
  aria_invalid: Option.none(),
  aria_pressed: Option.none(),
  aria_live: Option.none(),
  tab_index: Option.none(),
  data: [],
  auto_capitalize: Option.none(),
  auto_correct: Option.none(),
  wrap: Option.none(),
};

const markup = (node: Parameters<typeof uiRenderStatic>[0]): string => renderToStaticMarkup(uiRenderStatic(node));

test("host attributes are typed fields written as their DOM names", () => {
  const node = uiWithHost(
    {
      ...none,
      id: Option.some("status"),
      role: Option.some("status"),
      aria_live: Option.some({ _tag: tag("Ui.Live.Polite") }),
      aria_hidden: Option.some(false),
      tab_index: Option.some(-1),
      data: [["step", "2"]],
    },
    uiDiv({ class_name: "box" }, [uiText("ok")]),
  );
  assert.equal(
    markup(node),
    '<div class="box" id="status" role="status" aria-hidden="false" aria-live="polite" tabindex="-1" data-step="2">ok</div>',
  );
});

test("a `data` name that is not one, or a node that is no element, is refused", () => {
  assert.throws(() => uiWithHost({ ...none, data: [["Bad Name", "x"]] }, uiDiv({ class_name: "" }, [])), /data-Bad Name/);
  assert.throws(() => uiWithHost(none, uiText("x")), /host attributes go on an element/);
});

test("a heading has its level", () => {
  assert.equal(markup(uiHeadingAt({ _tag: tag("Ui.Level.H3") }, [uiText("Title")])), "<h3>Title</h3>");
});
