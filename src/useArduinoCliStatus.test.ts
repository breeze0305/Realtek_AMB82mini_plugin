import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ArduinoCliStatus } from "./types";
import { useArduinoCliStatus } from "./useArduinoCliStatus";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const missing: ArduinoCliStatus = { status: "not_installed", cli_path: null };
const installed: ArduinoCliStatus = { status: "not_on_path", cli_path: "C:\\Arduino IDE\\arduino-cli.exe" };
const configured: ArduinoCliStatus = { ...installed, status: "on_path" };

async function settle() {
  await act(async () => {});
}

describe("Arduino CLI status refresh", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invoke.mockReset();
    invoke.mockResolvedValue(missing);
  });
  afterEach(() => vi.useRealTimers());

  it("detects a manual installation while the installer page remains open and on focus", async () => {
    const { result, unmount } = renderHook(() => useArduinoCliStatus(true, false));
    await settle();
    expect(result.current.status).toEqual(missing);
    invoke.mockResolvedValue(installed);
    await act(async () => vi.advanceTimersByTime(5000));
    expect(result.current.status).toEqual(installed);
    invoke.mockResolvedValue(configured);
    await act(async () => window.dispatchEvent(new Event("focus")));
    expect(result.current.status).toEqual(configured);
    unmount();
    const count = invoke.mock.calls.length;
    await act(async () => {
      vi.advanceTimersByTime(10000);
      window.dispatchEvent(new Event("focus"));
    });
    expect(invoke).toHaveBeenCalledTimes(count);
  });

  it("ignores an old query during installation and refreshes after completion", async () => {
    let resolveOld!: (value: ArduinoCliStatus) => void;
    invoke.mockImplementationOnce(() => new Promise<ArduinoCliStatus>((resolve) => (resolveOld = resolve)));
    const { result, rerender } = renderHook(({ busy }) => useArduinoCliStatus(true, busy), {
      initialProps: { busy: false },
    });
    rerender({ busy: true });
    await act(async () => {
      result.current.updateStatus(configured);
      resolveOld(missing);
      vi.advanceTimersByTime(10000);
      window.dispatchEvent(new Event("focus"));
    });
    expect(result.current.status).toEqual(configured);
    expect(invoke).toHaveBeenCalledTimes(1);
    invoke.mockResolvedValue(configured);
    rerender({ busy: false });
    await settle();
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(result.current.status).toEqual(configured);
  });

  it("exposes a query failure and recovers on focus without polling outside the page", async () => {
    invoke.mockRejectedValueOnce("Registry access denied");
    const { result } = renderHook(() => useArduinoCliStatus(false, false));
    await settle();
    expect(result.current.error).toBe("Registry access denied");
    await act(async () => vi.advanceTimersByTime(10000));
    expect(invoke).toHaveBeenCalledTimes(1);
    invoke.mockResolvedValue(installed);
    await act(async () => window.dispatchEvent(new Event("focus")));
    expect(result.current.status).toEqual(installed);
    expect(result.current.error).toBeNull();
  });

  it("does not overlap slow polls or let an older read overwrite an action result", async () => {
    let resolveOld!: (value: ArduinoCliStatus) => void;
    invoke.mockImplementationOnce(() => new Promise<ArduinoCliStatus>((resolve) => (resolveOld = resolve)));
    const { result } = renderHook(() => useArduinoCliStatus(true, false));
    await act(async () => {
      vi.advanceTimersByTime(15000);
      result.current.updateStatus(configured);
      resolveOld(missing);
    });
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(result.current.status).toEqual(configured);
  });
});
