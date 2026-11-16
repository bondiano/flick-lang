// `Runner.Node`: runs `main` on Node.js, with signals, host services, and the exit code.
import { asMain, runMain } from "../entry.ts";
import { provideNodeServices } from "./host.ts";

export const run = (main: unknown): Promise<void> => runMain(asMain(main), { provide: provideNodeServices });
