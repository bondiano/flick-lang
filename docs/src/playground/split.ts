// DOM measurement, pointer capture and persistence for Flick's panel sizing policy.
import { clamp_share, initial_share, key_share, maximum, minimum } from "./Split.fl";

const storageKey = (stacked: boolean): string => stacked ? "flick-playground-split-stacked" : "flick-playground-split";

const stored = (stacked: boolean): number => {
  try {
    const value = Number(localStorage.getItem(storageKey(stacked)));
    return Number.isFinite(value) && value >= minimum && value <= maximum ? value : initial_share;
  } catch {
    return initial_share;
  }
};

/** Binds the splitter of `root`; the returned function unbinds it. */
export const bindSplit = (root: HTMLElement): (() => void) => {
  const panes = root.querySelector<HTMLElement>(".fp-panes");
  const split = root.querySelector<HTMLElement>(".fp-split");
  if (panes === null || split === null) return () => {};
  let stacked = false;
  let pointer: number | undefined;
  const set = (percent: number, save: boolean): void => {
    const share = clamp_share(Math.round(percent));
    panes.style.setProperty("--fp-split", `${share}%`);
    split.setAttribute("aria-valuenow", String(share));
    if (!save) return;
    try {
      localStorage.setItem(storageKey(stacked), String(share));
    } catch {
      // a share that is not remembered is still applied
    }
  };
  const now = (): number => Number(split.getAttribute("aria-valuenow"));
  const orientation = (): void => {
    // CSS owns the responsive layout; read its rendered axis, including container queries.
    const next = getComputedStyle(panes).gridTemplateColumns.split(" ").length === 1;
    if (next === stacked && split.hasAttribute("aria-orientation")) return;
    stacked = next;
    split.setAttribute("aria-orientation", stacked ? "horizontal" : "vertical");
    set(stored(stacked), false);
  };
  const move = (event: PointerEvent): void => {
    if (event.pointerId !== pointer) return;
    const box = panes.getBoundingClientRect();
    const size = stacked ? box.height : box.width;
    if (size > 0) set(((stacked ? event.clientY - box.top : event.clientX - box.left) / size) * 100, true);
  };
  const down = (event: PointerEvent): void => {
    if (event.button !== 0 || !event.isPrimary || pointer !== undefined) return;
    event.preventDefault();
    orientation();
    split.focus();
    pointer = event.pointerId;
    split.setPointerCapture(event.pointerId);
    panes.classList.add("fp-resizing");
    split.addEventListener("pointermove", move);
  };
  const up = (event: PointerEvent): void => {
    if (pointer !== event.pointerId) return;
    pointer = undefined;
    if (split.hasPointerCapture(event.pointerId)) split.releasePointerCapture(event.pointerId);
    panes.classList.remove("fp-resizing");
    split.removeEventListener("pointermove", move);
  };
  const keys = (event: KeyboardEvent): void => {
    const result = key_share(stacked)(event.key)(now());
    if (!result.handled) return;
    event.preventDefault();
    set(result.share, true);
  };
  split.setAttribute("aria-valuemin", String(minimum));
  split.setAttribute("aria-valuemax", String(maximum));
  orientation();
  const observer = new ResizeObserver(orientation);
  observer.observe(panes);
  split.addEventListener("pointerdown", down);
  split.addEventListener("pointerup", up);
  split.addEventListener("pointercancel", up);
  split.addEventListener("lostpointercapture", up);
  split.addEventListener("keydown", keys);
  return () => {
    observer.disconnect();
    if (pointer !== undefined && split.hasPointerCapture(pointer)) split.releasePointerCapture(pointer);
    panes.classList.remove("fp-resizing");
    split.removeEventListener("pointerdown", down);
    split.removeEventListener("pointerup", up);
    split.removeEventListener("pointercancel", up);
    split.removeEventListener("lostpointercapture", up);
    split.removeEventListener("pointermove", move);
    split.removeEventListener("keydown", keys);
  };
};
