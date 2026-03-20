import { useEffect, type RefObject, type DependencyList } from 'react';

export function usePlot(
  containerRef: RefObject<HTMLElement | null>,
  createPlot: () => (HTMLElement | SVGElement) & { remove?: () => void },
  deps: DependencyList,
): void {
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const plot = createPlot();
    container.innerHTML = '';
    container.appendChild(plot);

    return () => {
      if (plot.remove) {
        plot.remove();
      }
      container.innerHTML = '';
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
