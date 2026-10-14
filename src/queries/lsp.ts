import { queryOptions } from '@tanstack/react-query';
import { fetchLspEligibility } from '../lib/api';

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
