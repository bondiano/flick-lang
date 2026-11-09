import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Option } from "effect";

import { ReadService, atomState, syncOperation } from "../src/index.ts";
import {
  eventExhaust,
  uiButton,
  uiComponent,
  uiDiv,
  uiField,
  uiFieldArea,
  uiFieldset,
  uiFooter,
  uiForm,
  uiFragment,
  uiHr,
  uiHtml,
  uiIcon,
  uiImage,
  uiInput,
  uiNav,
  uiRender,
  uiSubmit,
  uiText,
  uiView,
} from "../src/ui/react/index.ts";
import { uiToHtml } from "../src/ui/html.ts";

/** `Atom.get` as a Flick render calls it: a Sync operation of `Atom.Read`. */
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the operation takes the atom
const get = syncOperation(ReadService, "get", 1) as (atom: unknown) => unknown;

const handler = eventExhaust(() => Effect.void);
const Widget = () => null;

test("the elements of a page: navigation, images, icons, forms and their fields", () => {
  const ui = uiNav({ class_name: "navbar" }, [
    uiImage({ src: "/a.svg", alt: "ada", class_name: "user-pic" }),
    uiIcon({ class_name: "ion-heart" }),
    uiHr,
    uiForm({ on_submit: handler }, [
      uiFieldset({ class_name: "form-group" }, [uiInput({ value: "", placeholder: "Password", on_input: handler })]),
      uiSubmit({ disabled: true }, [uiText("Sign in")]),
    ]),
    uiFooter({ class_name: "footer" }, []),
  ]);
  assert.equal(
    uiToHtml(ui),
    '<nav class="navbar"><img src="/a.svg" alt="ada" class="user-pic"><i class="ion-heart"></i><hr>' +
      '<form><fieldset class="form-group"><input value="" placeholder="Password"></fieldset><button type="submit" disabled>Sign in</button></form>' +
      '<footer class="footer"></footer></nav>',
  );
});

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
    uiText(`${String(greeting)} ${String(get(name))}`),
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
