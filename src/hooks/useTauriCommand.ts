import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface UseTauriCommandResult<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
  refetch: () => void;
}

export function useTauriCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): UseTauriCommandResult<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const serializedArgs = args ? JSON.stringify(args) : '';

  const fetch = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const parsedArgs = serializedArgs ? JSON.parse(serializedArgs) : undefined;
      const result = await invoke<T>(command, parsedArgs);
      setData(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [command, serializedArgs]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  return { data, loading, error, refetch: fetch };
}
