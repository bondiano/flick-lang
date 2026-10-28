// How often a page's task ran in this process: the browser check reads it
// on the client, where a hydrated page runs none.
export const mark = () => {
  globalThis.flickTaskRuns = (globalThis.flickTaskRuns ?? 0) + 1;
};
