import { micromark } from "micromark";
import { gfm, gfmHtml } from "micromark-extension-gfm";

const markdownOptions = {
  extensions: [gfm()],
  htmlExtensions: [gfmHtml()],
};

export function renderMarkdown(value) {
  const source = typeof value === "string" ? value : "";
  return micromark(source, markdownOptions);
}
