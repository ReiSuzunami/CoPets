import test from "node:test";
import assert from "node:assert/strict";

import { renderMarkdown } from "../ui/lib/markdown.js";

test("speech bubble markdown supports compact GFM content", () => {
  const html = renderMarkdown("**Bold** and `code`\n\n- first\n- second\n\n~~done~~");
  assert.match(html, /<strong>Bold<\/strong>/);
  assert.match(html, /<code>code<\/code>/);
  assert.match(html, /<ul>/);
  assert.match(html, /<del>done<\/del>/);
});

test("speech bubble markdown does not execute raw HTML or unsafe links", () => {
  const html = renderMarkdown('<script>alert("x")</script> [unsafe](javascript:alert(1))');
  assert.doesNotMatch(html, /<script|href=/);
  assert.match(html, /&lt;script&gt;/);
});
