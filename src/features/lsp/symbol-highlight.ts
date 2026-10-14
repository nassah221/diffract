/**
 * The ⌘-hover affordance: the symbol under the pointer gets an underline and a tint, the way an editor shows
 * that a click will follow it.
 *
 * Uses the CSS Custom Highlight API, which styles a `Range` without touching the rendered spans — the diff's
 * lines are split into per-token spans, so wrapping the symbol in an element is not an option.
 */

const NAME = 'diffity-lsp-symbol';

export function setSymbolHighlight(range: Range | null): void {
  // Typed as present, but an older webview would not have it and a missing affordance is not worth a crash.
  if (!('highlights' in CSS)) {
    return;
  }
  if (!range) {
    CSS.highlights.delete(NAME);
    return;
  }
  CSS.highlights.set(NAME, new Highlight(range));
}
