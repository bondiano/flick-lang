import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Option } from "effect";

import { ReadService, atomState } from "../src/index.ts";
import { eventExhaust, uiButton, uiComponent, uiDiv, uiField, uiFieldArea, uiFragment, uiHtml, uiInput, uiRender, uiText, uiView } from "../src/ui/react/index.ts";
import { uiToHtml } from "../src/ui/html.ts";

const handler = eventExhaust(() => Effect.void);
const Widget = () => null;

test("text and attribute values are escaped, booleans written as React writes them", () => {
  const ui = uiDiv({ class_name: 'a"b' }, [
    uiText("<script>&"),
    uiButton({ disabled: true, on_click: handler }, [uiText("ok")]),
    uiButton({ disabled: false, on_click: handler }, [uiText("no")]),
    uiInput({ value: "x>y", placeholder: "", on_input: handler }),
    uiHtml("<b>kept</b>"),
  ]);
  assert.equal(
    uiToHtml(ui),
    '<div class="a&quot;b">&lt;script&gt;&amp;<button type="button" disabled>ok</button><button type="button">no</button>' +
      '<input value="x&gt;y" placeholder=""><div><b>kept</b></div></div>',
  );
});

test("views and fields read atoms from a registry of the call, and a text area's value is its content", () => {
  const name = atomState("<Ann>");
  const view = uiView((greeting) =>
    Effect.map(Effect.flatMap(Effect.service(ReadService), (read) => read.get(name)), (n) => uiText(`${String(greeting)} ${String(n)}`)),
  );
  const ui = uiFragment([
    uiRender(view, "Hi"),
    uiField({ bind: name, placeholder: "Name", on_input: Option.none() }),
    uiFieldArea({ bind: name, placeholder: "", on_input: Option.none() }),
  ]);
  const expected = 'Hi &lt;Ann&gt;<input placeholder="Name" value="&lt;Ann&gt;"><textarea placeholder="">&lt;Ann&gt;</textarea>';
  assert.equal(uiToHtml(ui), expected);
  assert.equal(uiToHtml(ui), expected);
});

test("a foreign component is a defect naming it", () => {
  assert.throws(() => uiToHtml(uiComponent(Widget, {})), /the foreign component `Widget` renders only with React/);
});
