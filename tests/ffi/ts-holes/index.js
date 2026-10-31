export class Point {
  constructor(x) {
    this.x = x;
  }
  get length() {
    return Math.abs(this.x);
  }
  set label(value) {
    this._label = value;
  }
}
export const makePoint = (x) => new Point(x);
export const register = (ctor) => void ctor;
export const isPoint = (value) => value instanceof Point;
export const first = (xs) => xs[0];
export const scores = () => ({});
export const retry = (options) => options.retries;
export const logOptions = () => ({ verbose: true });
