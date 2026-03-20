import { useState, useCallback, useEffect, type RefObject } from 'react';

interface UseFileDropResult {
  isDragging: boolean;
  droppedFile: string | null;
  error: string | null;
  reset: () => void;
}

export function useFileDrop(ref: RefObject<HTMLElement | null>): UseFileDropResult {
  const [isDragging, setIsDragging] = useState(false);
  const [droppedFile, setDroppedFile] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reset = useCallback(() => {
    setDroppedFile(null);
    setError(null);
    setIsDragging(false);
  }, []);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;

    let dragCounter = 0;

    const handleDragEnter = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter++;
      setIsDragging(true);
    };

    const handleDragLeave = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter--;
      if (dragCounter === 0) {
        setIsDragging(false);
      }
    };

    const handleDragOver = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
    };

    const handleDrop = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter = 0;
      setIsDragging(false);

      const files = e.dataTransfer?.files;
      if (files && files.length > 0) {
        const file = files[0];
        const name = file.name.toLowerCase();

        if (name.endsWith('.txt') || name.endsWith('.csv') || name.endsWith('.tsv') || name.endsWith('.zip')) {
          const reader = new FileReader();
          reader.onload = () => {
            setDroppedFile(file.name);
          };
          reader.onerror = () => {
            setError('Failed to read file');
          };
          setDroppedFile(file.name);
        } else {
          setError('Unsupported file format. Please use a 23andMe, AncestryDNA, or similar raw data file (.txt, .csv, .tsv).');
        }
      }
    };

    element.addEventListener('dragenter', handleDragEnter);
    element.addEventListener('dragleave', handleDragLeave);
    element.addEventListener('dragover', handleDragOver);
    element.addEventListener('drop', handleDrop);

    return () => {
      element.removeEventListener('dragenter', handleDragEnter);
      element.removeEventListener('dragleave', handleDragLeave);
      element.removeEventListener('dragover', handleDragOver);
      element.removeEventListener('drop', handleDrop);
    };
  }, [ref]);

  return { isDragging, droppedFile, error, reset };
}
