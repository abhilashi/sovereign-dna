import { useRef, useState, useMemo, type ReactNode } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';

interface Column {
  key: string;
  label: string;
  width?: number;
  render?: (value: unknown, row: Record<string, unknown>) => ReactNode;
}

interface DataTableProps {
  columns: Column[];
  data: Record<string, unknown>[];
  rowHeight?: number;
  onRowClick?: (row: Record<string, unknown>, index: number) => void;
  searchable?: boolean;
  searchPlaceholder?: string;
}

export default function DataTable({
  columns,
  data,
  rowHeight = 36,
  onRowClick,
  searchable = false,
  searchPlaceholder = 'Search...',
}: DataTableProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [search, setSearch] = useState('');

  const filteredData = useMemo(() => {
    if (!search.trim()) return data;
    const term = search.toLowerCase();
    return data.filter((row) =>
      columns.some((col) => {
        const val = row[col.key];
        return val != null && String(val).toLowerCase().includes(term);
      }),
    );
  }, [data, search, columns]);

  const virtualizer = useVirtualizer({
    count: filteredData.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeight,
    overscan: 20,
  });

  return (
    <div className="flex flex-col h-full">
      {searchable && (
        <div className="pb-3">
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={searchPlaceholder}
            aria-label={searchPlaceholder}
            className="w-full px-3 py-2 text-sm border border-border rounded-sm bg-surface text-text placeholder:text-text-muted focus:outline-none focus:border-accent font-sans"
          />
        </div>
      )}
      <div className="border border-border rounded-sm overflow-hidden flex flex-col flex-1 min-h-0">
        <div className="flex border-b border-border bg-surface shrink-0">
          {columns.map((col) => (
            <div
              key={col.key}
              className="px-3 py-2 text-xs font-semibold uppercase tracking-wider text-text-muted font-sans"
              style={{ width: col.width ?? 'auto', flex: col.width ? 'none' : 1 }}
            >
              {col.label}
            </div>
          ))}
        </div>
        <div ref={parentRef} className="overflow-auto flex-1">
          <div
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: '100%',
              position: 'relative',
            }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const row = filteredData[virtualRow.index];
              const isEven = virtualRow.index % 2 === 0;
              return (
                <div
                  key={virtualRow.index}
                  className={`flex items-center absolute w-full ${
                    isEven ? 'bg-surface' : 'bg-[#F5F5F3]'
                  } ${onRowClick ? 'cursor-pointer hover:bg-border/40' : ''}`}
                  style={{
                    height: `${virtualRow.size}px`,
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                  onClick={() => onRowClick?.(row, virtualRow.index)}
                >
                  {columns.map((col) => (
                    <div
                      key={col.key}
                      className="px-3 text-sm font-mono text-text truncate"
                      style={{
                        width: col.width ?? 'auto',
                        flex: col.width ? 'none' : 1,
                        lineHeight: `${virtualRow.size}px`,
                      }}
                    >
                      {col.render
                        ? col.render(row[col.key], row)
                        : String(row[col.key] ?? '')}
                    </div>
                  ))}
                </div>
              );
            })}
          </div>
        </div>
      </div>
      <div className="pt-2 text-xs text-text-muted font-sans">
        {filteredData.length.toLocaleString()} rows
        {search && ` (filtered from ${data.length.toLocaleString()})`}
      </div>
    </div>
  );
}
