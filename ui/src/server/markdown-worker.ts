// Runs one Markdown parse off the server's thread; see parseMarkdownBounded.
import { parseMarkdown } from "./markdown.ts";

declare const self: Worker;

self.onmessage = (event: MessageEvent<string>) => {
  self.postMessage(parseMarkdown(event.data));
};
