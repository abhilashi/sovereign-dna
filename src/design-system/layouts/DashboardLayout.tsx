import { useEffect, useState } from 'react';
import { Outlet, useLocation } from 'react-router-dom';
import { getNewResearchCount } from '../../lib/tauri-bridge';
import { useGenomeStore } from '../../stores/genomeStore';

export default function DashboardLayout() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const [researchBadge, setResearchBadge] = useState(0);
  const location = useLocation();

  useEffect(() => {
    let cancelled = false;

    async function fetchBadge() {
      if (!activeGenomeId) {
        if (!cancelled) setResearchBadge(0);
        return;
      }
      try {
        const count = await getNewResearchCount(activeGenomeId);
        if (!cancelled) setResearchBadge(count);
      } catch { /* ignore */ }
    }

    fetchBadge();
    const interval = setInterval(fetchBadge, 60000);
    return () => { cancelled = true; clearInterval(interval); };
  }, [activeGenomeId]);

  // Research page is full-bleed (no padding, no separate sidebar — it has its own thread panel)
  const isResearchPage = location.pathname === '/' || location.pathname === '/ask';

  if (isResearchPage) {
    return (
      <div className="min-h-screen bg-surface">
        <Outlet context={{ researchBadge }} />
      </div>
    );
  }

  // All other pages get minimal padding, no sidebar
  return (
    <div className="min-h-screen bg-surface">
      <main className="max-w-5xl mx-auto p-8">
        <Outlet context={{ researchBadge }} />
      </main>
    </div>
  );
}
