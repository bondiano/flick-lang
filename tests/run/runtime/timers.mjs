/** A promise resolved after `millis`. */
export const sleep = (millis) => new Promise((resolve) => setTimeout(resolve, millis));
