/**
 * ⌘-click a Go symbol in the diff to see where it is declared, and go there.
 *
 * The click position is resolved to a `(path, side, line, character)` by `lib/symbol-position`, the line's text
 * is taken from the diff itself (so the backend can refuse a position the diff no longer matches), and the
 * answer becomes links: a target in the repo opens in the Files tab at that line, one outside the repo (the
 * module cache, `$GOROOT`) opens in the editor.
 */

import { useCallback, useState } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { ContextMenu, MenuSeparator } from '../../components/ui/popover';
import * as tauri from '../../lib/tauri';
import { errorMessage, fetchDefinition } from '../../lib/api';
import { getFilePath } from '../../lib/diff-utils';
import type { DiffFile } from '../../lib/diff-parser';
import { symbolAtPoint } from '../../lib/symbol-position';
import type { LspLocation } from '../../lib/types';

/** The line as the diff renders it, or null when the position is not in the diff (a collapsed or omitted file). */
export function lineTextAt(files: DiffFile[], path: string, side: 'old' | 'new', line: number): string | null {
  const file = files.find((candidate) => getFilePath(candidate) === path);
  for (const hunk of file?.hunks ?? []) {
    for (const diffLine of hunk.lines) {
      const number = side === 'old' ? diffLine.oldLineNumber : diffLine.newLineNumber;
      if (number === line) {
        return diffLine.content;
      }
    }
  }
  return null;
}

/** `/r/<repo>/diff` → `/r/<repo>/tree?path=…&type=file&line=N`. */
export function treeUrlFor(pathname: string, path: string, line: number): string {
  const base = pathname.replace(/\/(diff|overview|tree)$/, '');
  return `${base}/tree?path=${encodeURIComponent(path)}&type=file&line=${line}`;
}

/**
 * Opening a file outside the repo: the editor command resolves a repo-relative path inside the repo it is
 * given, so hand it the file's own directory and its name, which keeps the line number working.
 */
export function editorTargetFor(location: LspLocation): { dir: string; name: string } {
  const cut = location.path.lastIndexOf('/');
  return { dir: location.path.slice(0, cut) || '/', name: location.path.slice(cut + 1) };
}

interface Pending {
  x: number;
  y: number;
  /** Empty when nothing was found; only set once the answer is in. */
  locations: LspLocation[];
  error: string | null;
}

const ITEM =
  'flex w-full items-center gap-1.5 rounded px-2.5 py-1.5 text-left text-xs text-text hover:bg-fill cursor-pointer';

export function useDefinitionJump(options: { ref: string; files: DiffFile[]; enabled: boolean }) {
  const { ref, files, enabled } = options;
  const [pending, setPending] = useState<Pending | null>(null);
  const navigate = useNavigate();
  const { pathname } = useLocation();

  const go = useCallback(
    (location: LspLocation) => {
      setPending(null);
      if (location.insideRepo) {
        navigate(treeUrlFor(pathname, location.path, location.line));
        return;
      }
      const { dir, name } = editorTargetFor(location);
      void tauri.openInEditor(dir, name, location.line).catch(() => {});
    },
    [navigate, pathname],
  );

  const onMouseDown = useCallback(
    async (event: React.MouseEvent) => {
      // ⌘-click, like an editor. Plain clicks stay free for selecting lines and commenting.
      if (!enabled || !event.metaKey || event.button !== 0) {
        return;
      }
      const target = symbolAtPoint(event.clientX, event.clientY);
      // v0 speaks Go: anything else would be a query that can only fail.
      if (!target || !target.path.endsWith('.go')) {
        return;
      }
      const lineText = lineTextAt(files, target.path, target.side, target.line);
      if (lineText === null) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const at = { x: event.clientX, y: event.clientY };
      try {
        // An empty line renders a phantom newline the caret can sit in, so the offset is bounded by the
        // line's real length before it is sent.
        const character = Math.max(0, Math.min(target.character, lineText.length));
        const locations = await fetchDefinition({ ...target, ref, character, lineText });
        // Go there. The menu exists only to disambiguate, so one answer means no menu at all.
        if (locations.length === 1) {
          go(locations[0]);
          return;
        }
        setPending({ ...at, locations, error: null });
      } catch (error) {
        setPending({ ...at, locations: [], error: errorMessage(error) });
      }
    },
    [enabled, files, go, ref],
  );

  const close = useCallback(() => setPending(null), []);

  const menu = (
    <ContextMenu position={pending ? { x: pending.x, y: pending.y } : null} onClose={close} width={340}>
      {pending?.error ? (
        <div className="px-2.5 py-2 text-xs text-text-secondary">{pending.error}</div>
      ) : !pending || pending.locations.length === 0 ? (
        <div className="px-2.5 py-2 text-xs text-text-muted">No definition found here.</div>
      ) : (
        <>
          {pending.locations.map((location) => (
            <button
              key={`${location.path}:${location.line}:${location.startCharacter}`}
              type="button"
              className={ITEM}
              onClick={() => go(location)}
            >
              <span className="truncate font-mono">{location.path}</span>
              <span className="shrink-0 text-text-muted">:{location.line}</span>
              {!location.insideRepo && <span className="shrink-0 text-text-muted">· opens in your editor</span>}
            </button>
          ))}
          <MenuSeparator />
          <div className="px-2.5 py-1 text-[11px] text-text-muted">⌘-click for the definition</div>
        </>
      )}
    </ContextMenu>
  );

  return { onMouseDown, menu };
}
