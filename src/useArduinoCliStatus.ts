import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

import type { ArduinoCliStatus } from "./types";

export function useArduinoCliStatus(polling: boolean, busy: boolean) {
  const [status, setStatus] = useState<ArduinoCliStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const revision = useRef(0);

  const updateStatus = useCallback((next: ArduinoCliStatus) => {
    revision.current += 1;
    setStatus(next);
    setError(null);
  }, []);

  useEffect(() => {
    if (busy) return;
    let disposed = false;
    let pending = false;
    async function refresh() {
      if (pending) return;
      pending = true;
      const requestRevision = revision.current;
      try {
        const next = await invoke<ArduinoCliStatus>("get_arduino_cli_status");
        if (!disposed && requestRevision === revision.current) {
          setStatus(next);
          setError(null);
        }
      } catch (reason) {
        if (!disposed && requestRevision === revision.current) {
          setError(String(reason));
        }
      } finally {
        pending = false;
      }
    }
    const onFocus = () => void refresh();
    void refresh();
    window.addEventListener("focus", onFocus);
    const timer = polling ? window.setInterval(onFocus, 5000) : undefined;
    return () => {
      disposed = true;
      window.removeEventListener("focus", onFocus);
      if (timer !== undefined) window.clearInterval(timer);
    };
  }, [polling, busy]);

  return { status, error, updateStatus };
}
