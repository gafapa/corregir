/** Convert browser UTF-16 positions into validated backend UTF-8 byte ranges. */
export function findManualSpans(text: string, manual: string): [number, number][] {
  const encoder = new TextEncoder();
  const spans: [number, number][] = [];
  for (const fragment of manual.split("\n").map((value) => value.trim()).filter(Boolean)) {
    let cursor = 0;
    let position = text.indexOf(fragment, cursor);
    if (position === -1) throw new Error(`The redaction fragment was not found: ${fragment}`);
    while (position !== -1) {
      spans.push([encoder.encode(text.slice(0, position)).length, encoder.encode(text.slice(0, position + fragment.length)).length]);
      cursor = position + fragment.length;
      position = text.indexOf(fragment, cursor);
    }
  }
  return spans;
}
