// Each top-level section of the documentation is a line of its own colour, in the order of the
// sidebar; the sidebar and the title of a page draw it.
export const lineColours = ["violet", "amber", "cobalt", "green", "coral"] as const;

export const lineOf = (index: number): string => `--line: var(--flick-line-${lineColours[index % lineColours.length]}-rail, var(--flick-line-${lineColours[index % lineColours.length]}))`;

interface Entry { isCurrent?: boolean; entries?: Entry[] }

export const holdsCurrent = (entry: Entry): boolean => entry.isCurrent === true || (entry.entries ?? []).some(holdsCurrent);
