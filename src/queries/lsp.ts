import { queryOptions } from '@tanstack/react-query';
import { fetchInlayHints, fetchLspEligibility } from '../lib/api';

/**
 * Whether go-to-click definition applies to the diff for `ref`.
 *
 * Only a working-tree side is eligible: `work` and `unstaged` (and a bare branch ref) are the working tree, a
 * pull request's `origin/<base>...HEAD` is the current HEAD, and `staged` is the index — which is not what a
 * language server reads. The backend decides; this just asks once per ref.
 */
export function lspEligibilityOptions(ref: string) {
  return queryOptions({
    queryKey: ['lsp', 'eligibility', ref],
    queryFn: () => fetchLspEligibility(ref),
    staleTime: 30_000,
    retry: false,
  });
}

/** Inlay hints for one file of one ref, asked for by the file block that is actually rendering it. */
export function inlayHintsOptions(ref: string, path: string, enabled: boolean) {
  return queryOptions({
    queryKey: ['lsp', 'inlay-hints', ref, path],
    queryFn: () => fetchInlayHints(path, 'new'),
    enabled,
    // Hints for a given ref do not move under the reviewer; a revert re-renders the diff and refetches.
    staleTime: 60_000,
    retry: false,
  });
}
