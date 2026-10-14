import { useState } from 'react';
import { openSettingsAt } from '../../lib/ui-store';
import { buttonGhost } from '../../components/ui/button-styles';
import { SparkleIcon, XIcon } from '../../components/ui/icon';
import { AGENTS, useAgents } from '../claude/agents';

const DISMISSED_KEY = 'diffity.agents-card-dismissed';

function readDismissed(): boolean {
  try {
    return localStorage.getItem(DISMISSED_KEY) === '1';
  } catch {
    return false;
  }
}

function writeDismissed() {
  try {
    localStorage.setItem(DISMISSED_KEY, '1');
  } catch {
    return;
  }
}

/** Shown only when no agent is installed; with one, there is nothing to set up. */
export function AgentsCard() {
  const { data: agents } = useAgents();
  const [dismissed, setDismissed] = useState(readDismissed);

  if (!agents || dismissed || agents.some((agent) => agent.installed)) {
    return null;
  }

  const dismiss = () => {
    writeDismissed();
    setDismissed(true);
  };

  return (
    <div className="mt-6 rounded-lg border border-border bg-bg-secondary px-4 py-3">
      <div className="flex items-start gap-2.5">
        <SparkleIcon className="w-4 h-4 mt-0.5 shrink-0 text-claude" />
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-medium text-text">No coding agent found</div>
          <p className="mt-0.5 text-xs text-text-secondary">Diffract works without one. To have an agent review diffs and fix comments, install one and sign in:</p>
          <ul className="mt-1.5 flex flex-col gap-1">
            {AGENTS.map((agent) => (
              <li key={agent.id} className="text-xs text-text-secondary">
                {agent.name}: <code className="font-mono text-[11px] text-text select-text">{agent.install}</code>
              </li>
            ))}
          </ul>
          <button onClick={() => openSettingsAt('agents')} className="mt-2 text-xs text-text-secondary underline decoration-text-muted/50 underline-offset-2 hover:text-text cursor-pointer">
            Agent settings
          </button>
        </div>
        <button onClick={dismiss} className={buttonGhost} title="Dismiss" aria-label="Dismiss">
          <XIcon className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
}
