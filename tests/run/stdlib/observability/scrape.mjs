// What a Prometheus scrape of a route gets: its content type and body.
export const scrape = async (url) => {
  const response = await fetch(url);
  return `${response.headers.get("content-type")}\n${await response.text()}`;
};
