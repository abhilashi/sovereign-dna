import { type ReactNode } from 'react';

interface SmallMultipleProps {
  children: ReactNode;
  columns?: number;
  title?: string;
}

export default function SmallMultiple({ children, columns = 3, title }: SmallMultipleProps) {
  return (
    <section className="mb-8">
      {title && (
        <div className="mb-4 pb-2 border-b border-border">
          <h3 className="text-sm font-semibold uppercase tracking-wider text-text-muted">
            {title}
          </h3>
        </div>
      )}
      <div
        className="grid gap-4"
        style={{ gridTemplateColumns: `repeat(${columns}, 1fr)` }}
      >
        {children}
      </div>
    </section>
  );
}
