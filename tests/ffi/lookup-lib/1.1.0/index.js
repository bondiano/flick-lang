const names = new Map([["1", "Ada"]]);
let counter = 0;

export const lookup = async (id, region) => {
  if (id === "boom") throw new TypeError("lookup failed");
  return names.get(id);
};
export const nextId = () => ++counter;
export const toDate = (value) => new Date(value);
export const each = (items, visit) => items.forEach((item) => visit(item));
