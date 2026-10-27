// Sets a variable of the process's environment for the trace run test.
export const set = (name, value) => {
  process.env[name] = value;
  return undefined;
};
