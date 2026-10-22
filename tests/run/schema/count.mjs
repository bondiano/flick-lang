// The default of `quota`: counts its calls in the cell it is given.
export const count = (cell) => {
  cell.value += 1;
  return 100 + cell.value;
};
