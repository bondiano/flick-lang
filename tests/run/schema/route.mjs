// Whether every text of a value is well-formed Unicode, which a URL can carry.
const wellFormed = (v) =>
  typeof v === "string" ? v.isWellFormed() : typeof v === "object" && v !== null ? Object.values(v).every(wellFormed) : true;

export const encodable = (value) => wellFormed(value);
