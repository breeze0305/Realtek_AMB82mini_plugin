import { describe, expect, it, vi } from "vitest";

import { createHomeCardGroups } from "./homeCards";
import { translations } from "./i18n";
import type { ArduinoCliStatus, InstallationResult } from "./types";

type CardGroupParams = Parameters<typeof createHomeCardGroups>[0];

function createGroups(internetConnected: boolean, overrides: Partial<CardGroupParams> = {}) {
  const onOpenImageConverter = vi.fn();
  const onOpenResourceCategory = vi.fn();
  const runAction = vi.fn<CardGroupParams["runAction"]>(async () => undefined);
  const groups = createHomeCardGroups({
    dashboard: null,
    internetConnected,
    language: "zh_TW",
    onOpenAnnotator: vi.fn(),
    onOpenCamera: vi.fn(),
    onOpenConverter: vi.fn(),
    onOpenImageConverter,
    onOpenResourceCategory,
    onOpenVersionUpdate: vi.fn(),
    onVersionChecked: vi.fn(),
    runAction: runAction as CardGroupParams["runAction"],
    t: translations.zh_TW,
    versionCheck: null,
    ...overrides,
  });

  return { groups, onOpenImageConverter, onOpenResourceCategory, runAction };
}

describe("createHomeCardGroups", () => {
  it("creates two independent resource entries before the six main functions", () => {
    const { groups, onOpenImageConverter, onOpenResourceCategory } = createGroups(true);

    expect(groups.resourceEntryCards.map((card) => card.id)).toEqual(["resource-installers", "resource-weights"]);
    expect(groups.resourceEntryCards.every((card) => card.wholeCardAction)).toBe(true);
    expect(groups.mainCards.map((card) => card.id)).toEqual([
      "camera",
      "converter",
      "annotator",
      "image-converter",
      "realtek-folder",
      "version",
    ]);
    expect(groups.installerCards.map((card) => card.key)).toEqual(["driver", "arduino", "arduinoCliPath", "vlc"]);
    expect(groups.installerCards.find((card) => card.key === "arduino")?.detail).toBe(
      "arduino-ide_latest_Windows_64bit.exe",
    );
    expect(groups.weightCards.map((card) => card.key)).toEqual(["hand", "box", "japan", "taiwan", "singapore"]);

    groups.resourceEntryCards[0].action();
    groups.resourceEntryCards[1].action();
    groups.mainCards.find((card) => card.id === "image-converter")?.action();
    expect(onOpenResourceCategory.mock.calls).toEqual([["installers"], ["weights"]]);
    expect(onOpenImageConverter).toHaveBeenCalledOnce();
  });

  it("keeps installer and embedded resource cards available offline", () => {
    const { groups } = createGroups(false);

    expect(groups.installerCards.map((card) => [card.key, card.disabled])).toEqual([
      ["driver", false],
      ["arduino", false],
      ["arduinoCliPath", true],
      ["vlc", false],
    ]);
    expect(groups.weightCards.every((card) => !card.disabled)).toBe(true);
  });

  it("still disables unrelated network-only functions while offline", () => {
    const { groups } = createGroups(false);

    expect(groups.mainCards.find((card) => card.id === "converter")?.disabled).toBe(true);
    expect(groups.mainCards.find((card) => card.id === "version")?.disabled).toBe(true);
  });

  it("represents all three CLI states independently of internet connectivity", () => {
    const t = translations.zh_TW;
    for (const [status, disabled, label, detail] of [
      ["not_installed", true, t.arduinoCliNotInstalled, t.arduinoCliNotInstalledDetail],
      ["not_on_path", false, t.arduinoCliAddPath, t.arduinoCliNotOnPathDetail],
      ["on_path", true, t.arduinoCliOnPath, t.arduinoCliOnPathDetail],
    ] as const) {
      const { groups } = createGroups(false, { arduinoCliStatus: { status, cli_path: null } });
      const card = groups.installerCards.find((item) => item.key === "arduinoCliPath")!;
      expect(card).toMatchObject({ disabled, label, detail, disabledReason: detail });
    }
  });

  it("keeps pending and failed detection disabled without claiming Arduino is uninstalled", () => {
    for (const [arduinoCliStatusError, expected] of [
      [null, translations.zh_TW.arduinoCliChecking],
      ["Registry lookup failed", translations.zh_TW.arduinoCliStatusUnavailable],
    ]) {
      const { groups } = createGroups(true, { arduinoCliStatusError });
      const card = groups.installerCards.find((item) => item.key === "arduinoCliPath")!;
      expect(card.disabled).toBe(true);
      expect(card.appearance).toBe("muted");
      expect(card.disabledReason).toBe(expected);
      expect(card.detail).not.toBe(translations.zh_TW.arduinoCliNotInstalledDetail);
    }
  });

  it("adds CLI to PATH using the separate local command and updates its status", () => {
    const onArduinoCliStatusChanged = vi.fn();
    const { groups, runAction } = createGroups(false, {
      arduinoCliStatus: { status: "not_on_path", cli_path: "C:\\Arduino\\arduino-cli.exe" },
      onArduinoCliStatusChanged,
    });
    groups.installerCards.find((item) => item.key === "arduinoCliPath")!.action();
    expect(runAction).toHaveBeenCalledOnce();
    const [key, command, next] = runAction.mock.calls[0];
    expect([key, command]).toEqual(["arduinoCliPath", "add_arduino_cli_to_path"]);
    const status: ArduinoCliStatus = { status: "on_path", cli_path: "C:\\Arduino\\arduino-cli.exe" };
    expect(next(status)).toBe(translations.zh_TW.arduinoCliPathSuccess);
    expect(onArduinoCliStatusChanged).toHaveBeenCalledExactlyOnceWith(status);
  });

  it("preserves installer retrieval while reporting completed automatic installation and PATH setup", () => {
    const onArduinoCliStatusChanged = vi.fn();
    const { groups, runAction } = createGroups(true, { onArduinoCliStatusChanged });
    const card = groups.installerCards.find((item) => item.key === "arduino")!;
    card.action();
    expect(runAction.mock.calls[0][1]).toBe("download_arduino_ide_as");
    runAction.mockClear();
    card.menuActions![0].action();
    const [key, command, next] = runAction.mock.calls[0];
    expect([key, command]).toEqual(["arduino", "download_and_install_arduino_ide"]);
    const result: InstallationResult = {
      path: "C:\\cache\\arduino.exe",
      reboot_required: false,
      arduino_cli: { status: "on_path", cli_path: "C:\\Arduino\\arduino-cli.exe" },
      path_error: null,
    };
    const message = next(result);
    expect(message).toContain(translations.zh_TW.installerComplete.replace("{app}", card.title));
    expect(message).toContain(translations.zh_TW.arduinoCliPathSuccess);
    expect(message).not.toContain(result.path);
    expect(onArduinoCliStatusChanged).toHaveBeenCalledExactlyOnceWith(result.arduino_cli);
  });

  it("reports successful installation with a recoverable PATH failure and reboot notice", () => {
    const onArduinoCliStatusChanged = vi.fn();
    const { groups, runAction } = createGroups(true, { onArduinoCliStatusChanged });
    const card = groups.installerCards.find((item) => item.key === "arduino")!;
    card.menuActions![0].action();
    const result: InstallationResult = {
      path: "C:\\cache\\arduino.exe",
      reboot_required: true,
      arduino_cli: { status: "not_on_path", cli_path: "C:\\Arduino\\arduino-cli.exe" },
      path_error: "Access denied",
    };
    const message = runAction.mock.calls[0][2](result);
    expect(message).toContain(translations.zh_TW.installerComplete.replace("{app}", card.title));
    expect(message).toContain(translations.zh_TW.arduinoCliAutomaticPathFailed);
    expect(message).toContain("Access denied");
    expect(message).toContain(translations.zh_TW.installerRebootRequired);
    expect(message).not.toContain(translations.zh_TW.arduinoCliPathSuccess);
    expect(onArduinoCliStatusChanged).toHaveBeenCalledExactlyOnceWith(result.arduino_cli);
  });

  it("reports VLC installation completion without changing Arduino CLI status", () => {
    const onArduinoCliStatusChanged = vi.fn();
    const { groups, runAction } = createGroups(true, { onArduinoCliStatusChanged });
    const card = groups.installerCards.find((item) => item.key === "vlc")!;
    card.menuActions![0].action();
    expect(runAction.mock.calls[0][1]).toBe("download_and_install_vlc");
    const result: InstallationResult = {
      path: "C:\\cache\\vlc.exe",
      reboot_required: false,
      arduino_cli: null,
      path_error: null,
    };
    expect(runAction.mock.calls[0][2](result)).toBe(translations.zh_TW.installerComplete.replace("{app}", card.title));
    expect(onArduinoCliStatusChanged).not.toHaveBeenCalled();
  });

  it("shows installation and PATH setup phases while waiting for completion", () => {
    const { groups } = createGroups(true, { installerProgress: { arduino: "configuring_path", vlc: "installing" } });
    expect(groups.installerCards.find((item) => item.key === "arduino")).toMatchObject({
      label: translations.zh_TW.arduinoCliConfiguring,
      detail: translations.zh_TW.installerWaitingDetail,
    });
    expect(groups.installerCards.find((item) => item.key === "vlc")).toMatchObject({
      label: translations.zh_TW.installerInstalling,
      detail: translations.zh_TW.installerWaitingDetail,
    });
  });
});
