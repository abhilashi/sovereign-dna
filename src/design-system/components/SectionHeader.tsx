import { type ReactNode } from 'react';

interface SectionHeaderProps {
  title: string;
  description?: string;
  action?: ReactNode;
}

export default function SectionHeader({ title, description, action }: SectionHeaderProps) {
  return (
    <div className="mb-6">
      <div className="border-t border-border pt-4 flex items-start justify-between">
        <div>
          <h2 className="text-sm font-semibold uppercase tracking-wider text-text">
            {title}
          </h2>
          {description && (
            <p className="text-xs text-text-muted mt-1">{description}</p>
          )}
        </div>
        {action && <div className="shrink-0 ml-4">{action}</div>}
      </div>
    </div>
  );
}
