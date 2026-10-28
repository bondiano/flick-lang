// How many times the guarded function's body ran, for the auth run test.
let runs = 0;

export const ran = () => {
  runs += 1;
  return undefined;
};

export const runs_ = () => runs;
