// A client of the streamed page in the network stream run test: it reads
// the response chunk by chunk and lets the page's task end only once the
// shell has arrived.
let release;
const gate = new Promise((resolve) => (release = resolve));

/** The user's name, once the client has seen the shell. */
export const later = () => gate.then(() => "ada");

/** What the client saw of the page at `url`: the shell first, the rest after the task ended. */
export const visit = async (url) => {
  const response = await fetch(url);
  const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
  let shell = "";
  while (!shell.includes("waiting")) {
    // oxlint-disable-next-line eslint/no-await-in-loop -- chunks are read one after another
    const { done, value } = await reader.read();
    if (done) break;
    shell += value;
  }
  release();
  let rest = "";
  for (;;) {
    // oxlint-disable-next-line eslint/no-await-in-loop -- chunks are read one after another
    const { done, value } = await reader.read();
    if (done) break;
    rest += value;
  }
  return [
    `type: ${response.headers.get("content-type")}`,
    `shell before the task: ${shell.includes("waiting") && !shell.includes("hello")}`,
    `state after it: ${rest.includes("data-flick-state") && rest.includes('"ada"')}`,
    `markup after it: ${rest.includes("hello ada")}`,
  ].join("\n");
};
