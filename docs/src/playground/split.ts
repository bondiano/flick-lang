// The splitter between the editor and the result. Flick draws it; the pointer and the keys move it here.
const key = "flick-playground-split";
const min = 20;
const max = 80;

const stored = (): number => {
  try {
    const value = Number(localStorage.getItem(key));
    return value >= min && value <= max ? value : 55;
  } catch {
    return 55;
  }
};

/** Binds the splitter of `root`; the returned function unbinds it. */
export const bindSplit = (root: HTMLElement): (() => void) => {
  const panes = root.querySelector<HTMLElement>(".fp-panes");
  const split = root.querySelector<HTMLElement>(".fp-split");
  if (panes === null || split === null) return () => {};
  const set = (percent: number, save: boolean): void => {
    const share = Math.round(Math.min(max, Math.max(min, percent)));
    panes.style.setProperty("--fp-split", `${share}%`);
    split.setAttribute("aria-valuenow", String(share));
    if (!save) return;
    try {
      localStorage.setItem(key, String(share));
    } catch {
      // a share that is not remembered is still applied
    }
  };
  const now = (): number => Number(split.getAttribute("aria-valuenow"));
  const move = (event: PointerEvent): void => {
    const box = panes.getBoundingClientRect();
    set(((event.clientX - box.left) / box.width) * 100, true);
  };
  const down = (event: PointerEvent): void => {
    split.setPointerCapture(event.pointerId);
    split.addEventListener("pointermove", move);
  };
  const up = (event: PointerEvent): void => {
    if (split.hasPointerCapture(event.pointerId)) split.releasePointerCapture(event.pointerId);
    split.removeEventListener("pointermove", move);
  };
  const keys = (event: KeyboardEvent): void => {
    const steps: Record<string, number> = { ArrowLeft: -5, ArrowRight: 5, Home: min - now(), End: max - now() };
    const step = steps[event.key];
    if (step === undefined) return;
    event.preventDefault();
    set(now() + step, true);
  };
  split.setAttribute("aria-orientation", "vertical");
  split.setAttribute("aria-valuemin", String(min));
  split.setAttribute("aria-valuemax", String(max));
  set(stored(), false);
  split.addEventListener("pointerdown", down);
  split.addEventListener("pointerup", up);
  split.addEventListener("pointercancel", up);
  split.addEventListener("keydown", keys);
  return () => {
    split.removeEventListener("pointerdown", down);
    split.removeEventListener("pointerup", up);
    split.removeEventListener("pointercancel", up);
    split.removeEventListener("pointermove", move);
    split.removeEventListener("keydown", keys);
  };
};
