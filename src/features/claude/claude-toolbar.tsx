import { useCallback, useEffect, useRef, useState } from 'react';
import { TREE_REF } from '../../lib/types';
import { runLabel, runViewLabel, stopClaude, useActiveRun, useQueuedCount } from './claude-runner';
import { useCurrentViewRef } from '../../hooks/use-current-view';
import { useRepoPath } from '../../hooks/use-repo';
import { buttonClaude } from '../../components/ui/button-styles';
import { AskClaudePopover, useAskClaudeRequest } from './ask-claude-review';
import { cn } from '../../lib/cn';
import { toast } from 'sonner';
import { useReviewState } from '../review/review-state';
import { SparkleIcon, StopIcon } from '../../components/ui/icon';
import { Popover } from '../../components/ui/popover';
import { Spinner } from '../../components/icons/spinner';
import { useRunPick } from './model-setting';
import { ActivityPeek, formatElapsed, useNow } from './activity-panel';
import { toggleActivity, useRunActivity } from './run-activity';
import { agentMeta } from './agents';

interface ClaudeToolbarProps {
  diffRef: string | null;
  sessionId: string | null;
  hasChanges?: boolean;
  focusedFile?: string | null;
}

export function ClaudeStatus() {
  const repoPath = useRepoPath();
  const run = useActiveRun(repoPath);
  const queued = useQueuedCount(repoPath);
  const now = useNow(run !== null);
  const currentRef = useCurrentViewRef();
  const activityOpen = useRunActivity((state) => state.open);
  const pillRef = useRef<HTMLDivElement>(null);
  const [peek, setPeek] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (timer.current) {
      clearTimeout(timer.current);
    }
  }, []);

  if (!run) {
    return null;
  }

  const hover = (open: boolean) => {
    if (timer.current) {
      clearTimeout(timer.current);
    }
    timer.current = setTimeout(() => setPeek(open && !useRunActivity.getState().open), open ? 300 : 150);
  };
  const name = run.agentId ? agentMeta(run.agentId).short : 'Agent';
  const elsewhere = !!run.ref && run.ref !== currentRef;
  const where = run.ref ? runViewLabel(run.context.repoPath, run.ref) : null;

  return (
    <div ref={pillRef} className="flex items-stretch h-7 rounded-md border border-control-border bg-raised overflow-hidden text-xs min-w-0">
      <button
        onClick={() => {
          setPeek(false);
          toggleActivity();
        }}
        onMouseEnter={() => hover(true)}
        onMouseLeave={() => hover(false)}
        aria-pressed={activityOpen}
        className={cn(
          'flex items-center gap-2 pl-2.5 pr-2 whitespace-nowrap min-w-0 overflow-hidden transition-colors cursor-pointer',
          activityOpen ? 'bg-claude/12 text-claude' : 'text-text hover:bg-control-hover',
        )}
        title={where && elsewhere ? `Working on ${where} · click to see what ${name} is doing` : `See what ${name} is doing`}
      >
        <Spinner className="text-claude" />
        <span className="font-medium truncate @max-3xl/titlebar:hidden">{runLabel(run.action, name)}</span>
        {elsewhere && where && (
          <span className="text-text-secondary truncate max-w-[180px] @max-4xl/titlebar:hidden">on {where}</span>
        )}
        {run.startedAt && <span className="text-text-muted tabular-nums">{formatElapsed(now - run.startedAt)}</span>}
        {queued > 0 && <span className="text-text-muted @max-3xl/titlebar:hidden">+{queued} queued</span>}
      </button>
      <button
        onClick={() => void stopClaude(repoPath)}
        className="flex items-center gap-1 px-2 shrink-0 border-l border-control-border text-text-secondary hover:text-text hover:bg-control-hover transition-colors cursor-pointer"
        title={`Stop ${name}`}
      >
        <StopIcon size="xs" />
        Stop
      </button>
      <Popover open={peek} onClose={() => setPeek(false)} anchorRef={pillRef} align="end" width={340} className="p-0">
        <div onMouseEnter={() => hover(true)} onMouseLeave={() => hover(false)}>
          <ActivityPeek onOpen={() => setPeek(false)} />
        </div>
      </Popover>
    </div>
  );
}

export function ClaudeToolbar(props: ClaudeToolbarProps) {
  const { diffRef, sessionId, hasChanges = true, focusedFile } = props;
  const [open, setOpen] = useState(false);
  const anchorRef = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => setOpen(false), []);
  const repoPath = useRepoPath();
  const run = useActiveRun(repoPath);
  const agent = agentMeta(useRunPick('review', repoPath).agent);
  const { prMode } = useReviewState();
  const requested = useAskClaudeRequest((state) => state.ref);

  const reviewRef = diffRef && diffRef !== TREE_REF && hasChanges ? diffRef : null;

  useEffect(() => {
    if (!requested || requested !== reviewRef) {
      return;
    }
    useAskClaudeRequest.setState({ ref: null });
    if (run) {
      toast.info(`${run.agentId ? agentMeta(run.agentId).short : 'The agent'} is already working. Wait for it to finish or stop it first.`);
      return;
    }
    setOpen(true);
  }, [requested, reviewRef, run]);

  if (run) {
    return <ClaudeStatus />;
  }
  if (!reviewRef) {
    return null;
  }

  return (
    <>
      <button
        ref={anchorRef}
        onClick={() => setOpen(!open)}
        className={cn(buttonClaude, open && 'bg-claude/16')}
        title={prMode ? `${agent.short} reviews this pull request and leaves its comments in Diffract only (marked ${agent.short}). Use “Add to my review” on any you want to post to GitHub.` : `Ask ${agent.short} to review these changes and leave comments, or to make a change for you.`}
        aria-expanded={open}
      >
        <SparkleIcon size="md" />
        <span className="@max-3xl/titlebar:hidden">Ask {agent.short}</span>
      </button>
      <AskClaudePopover open={open} onClose={close} anchorRef={anchorRef} diffRef={reviewRef} sessionId={sessionId} focusedFile={focusedFile} />
    </>
  );
}
