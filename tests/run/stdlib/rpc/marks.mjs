// Marks the handlers of the local run test leave once they finish.
const marks = [];
export const mark = (name) => {
  marks.push(name);
  return undefined;
};
export const marked = () => marks.join(", ");

export const sleep = (millis) => new Promise((resolve) => setTimeout(() => resolve(undefined), millis));
