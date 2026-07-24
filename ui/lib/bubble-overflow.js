export function insertOverflowEllipsis(value, offset) {
  if (!value) return value;
  const numericOffset = Number.isFinite(offset) ? Math.trunc(offset) : value.length;
  const index = Math.max(0, Math.min(numericOffset, value.length));
  if (value[index] === "…") return value.slice(0, index + 1);
  const prefix = value.slice(0, index);
  return prefix.endsWith("…") ? prefix : `${prefix}…`;
}

function normalizeTextOffset(value, offset) {
  if (offset <= 0 || offset >= value.length) return offset;
  const previous = value.charCodeAt(offset - 1);
  const next = value.charCodeAt(offset);
  return previous >= 0xd800 && previous <= 0xdbff && next >= 0xdc00 && next <= 0xdfff
    ? offset - 1
    : offset;
}

function previousTextOffset(value, offset) {
  const next = Math.max(0, offset - 1);
  return normalizeTextOffset(value, next);
}

function rangeEndRect(document, textNode, offset) {
  if (offset <= 0) return null;
  const range = document.createRange();
  range.setStart(textNode, 0);
  range.setEnd(textNode, offset);
  const rects = range.getClientRects();
  return rects.length ? rects[rects.length - 1] : null;
}

function lastVisibleTextPosition(node) {
  const document = node.ownerDocument;
  const nodeFilter = document.defaultView?.NodeFilter;
  if (!nodeFilter || node.clientHeight <= 0) return null;
  const viewportBottom = node.getBoundingClientRect().top + node.clientHeight - 1;
  const walker = document.createTreeWalker(node, nodeFilter.SHOW_TEXT);
  let candidate = null;
  let textNode;
  while ((textNode = walker.nextNode())) {
    if (!textNode.data.trim()) continue;
    let low = 0;
    let high = textNode.length;
    while (low < high) {
      const middle = Math.ceil((low + high) / 2);
      const rect = rangeEndRect(document, textNode, middle);
      if (rect && rect.bottom <= viewportBottom) low = middle;
      else high = middle - 1;
    }
    low = normalizeTextOffset(textNode.data, low);
    if (low > 0) candidate = { textNode, offset: low, viewportBottom };
    if (low < textNode.length) break;
  }
  return candidate;
}

export function markVisibleOverflow(node) {
  const originalMarkup = node.innerHTML;
  const position = lastVisibleTextPosition(node);
  if (!position) return false;
  const { textNode, viewportBottom } = position;
  const document = node.ownerDocument;
  const originalText = textNode.data;
  let offset = position.offset;
  const removal = document.createRange();
  removal.setStart(textNode, offset);
  removal.setEnd(node, node.childNodes.length);
  removal.deleteContents();
  while (offset > 0 && /\s/u.test(originalText[offset - 1])) {
    offset = previousTextOffset(originalText, offset);
  }
  while (offset > 0) {
    textNode.data = insertOverflowEllipsis(originalText, offset);
    const markerOffset = textNode.data.length - 1;
    const marker = document.createRange();
    marker.setStart(textNode, markerOffset);
    marker.setEnd(textNode, markerOffset + 1);
    if (marker.getBoundingClientRect().bottom <= viewportBottom) break;
    offset = previousTextOffset(originalText, offset);
  }
  if (offset <= 0) {
    node.innerHTML = originalMarkup;
    return false;
  }
  return true;
}
