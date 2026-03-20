import { NavLink } from 'react-router-dom';

export interface SidebarItem {
  readonly label: string;
  readonly path: string;
  readonly badge?: string | number;
}

interface SidebarProps {
  items: readonly SidebarItem[];
}

export default function Sidebar({ items }: SidebarProps) {
  return (
    <nav className="w-[200px] shrink-0 border-r border-border bg-surface h-screen sticky top-0 flex flex-col">
      <div className="px-5 py-6">
        <h1 className="text-[10px] font-semibold tracking-[0.2em] uppercase text-text-muted">
          Genome Studio
        </h1>
      </div>
      <div className="flex flex-col gap-0.5 px-3 flex-1">
        {items.map((item) => (
          <NavLink
            key={item.path}
            to={item.path}
            className={({ isActive }) =>
              `block px-3 py-2 text-sm transition-colors duration-100 border-l-2 ${
                isActive
                  ? 'text-accent border-accent font-medium'
                  : 'text-text-muted border-transparent hover:text-text hover:border-border'
              }`
            }
          >
            <span className="flex items-center justify-between">
              <span>{item.label}</span>
              {item.badge !== undefined && (
                <span className="text-[10px] font-mono text-text-muted bg-border rounded-sm px-1.5 py-0.5">
                  {item.badge}
                </span>
              )}
            </span>
          </NavLink>
        ))}
      </div>
      <div className="px-5 py-4 border-t border-border">
        <p className="text-[10px] text-text-muted">v1.0.0 &middot; Local only</p>
      </div>
    </nav>
  );
}
