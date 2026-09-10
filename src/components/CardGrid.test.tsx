import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { createHomeCardGroups } from "../homeCards";
import { translations } from "../i18n";
import type { ArduinoCliStatus, DownloadKey, RunningAction } from "../types";
import { CardGrid, type HomeCard } from "./CardGrid";

function createCards(arduinoCliStatus: ArduinoCliStatus) {
  return createHomeCardGroups({
    arduinoCliStatus,
    dashboard: null,
    internetConnected: false,
    language: "en_US",
    onOpenAnnotator: vi.fn(),
    onOpenCamera: vi.fn(),
    onOpenConverter: vi.fn(),
    onOpenImageConverter: vi.fn(),
    onOpenResourceCategory: vi.fn(),
    onOpenVersionUpdate: vi.fn(),
    onVersionChecked: vi.fn(),
    runAction: async () => undefined,
    t: translations.en_US,
    versionCheck: null,
  });
}

function renderCards(cards: HomeCard[], running: RunningAction = null) {
  return render(
    <CardGrid
      cards={cards}
      downloadProgress={{}}
      isDownloadKey={(key: RunningAction): key is DownloadKey => key === "arduino" || key === "vlc"}
      openActionMenu={null}
      running={running}
      setOpenActionMenu={vi.fn()}
      t={translations.en_US}
    />,
  );
}

describe("CardGrid CLI PATH action", () => {
  it.each([
    ["not_installed", "Not installed", translations.en_US.arduinoCliNotInstalledDetail],
    ["on_path", "On PATH", translations.en_US.arduinoCliOnPathDetail],
  ] as const)("disables the %s state with its actual explanation", async (status, label, detail) => {
    const user = userEvent.setup();
    const groups = createCards({ status, cli_path: null });
    const card = groups.installerCards.find((item) => item.key === "arduinoCliPath")!;
    card.action = vi.fn();
    renderCards([card]);

    expect(screen.getByRole("button", { name: label })).toBeDisabled();
    expect(screen.getByText(detail)).toBeInTheDocument();
    expect(screen.getByRole("article")).toHaveClass(status === "not_installed" ? "isMuted" : "isComplete");
    expect(screen.getByRole("article")).toHaveClass("hasWrappedDetail");
    expect(screen.queryByText(translations.en_US.unavailableOffline)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: label }));
    expect(card.action).not.toHaveBeenCalled();
  });

  it("allows local PATH setup while offline after CLI is detected", async () => {
    const user = userEvent.setup();
    const groups = createCards({ status: "not_on_path", cli_path: "C:\\Arduino\\arduino-cli.exe" });
    const card = groups.installerCards.find((item) => item.key === "arduinoCliPath")!;
    card.action = vi.fn();
    renderCards([card]);

    expect(screen.getByRole("button", { name: "Add to PATH" })).toBeEnabled();
    expect(screen.getByRole("article")).not.toHaveClass("isMuted", "isComplete");
    expect(screen.getByRole("article")).toHaveClass("hasWrappedDetail");
    await user.click(screen.getByRole("button", { name: "Add to PATH" }));
    expect(card.action).toHaveBeenCalledOnce();
  });

  it("blocks PATH setup while an installer is running", async () => {
    const user = userEvent.setup();
    const groups = createCards({ status: "not_on_path", cli_path: "C:\\Arduino\\arduino-cli.exe" });
    const card = groups.installerCards.find((item) => item.key === "arduinoCliPath")!;
    card.action = vi.fn();
    renderCards([card], "arduino");

    expect(screen.getByRole("button", { name: "Add to PATH" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Add to PATH" }));
    expect(card.action).not.toHaveBeenCalled();
  });

  it("retains the offline explanation for unrelated network-only cards", () => {
    const groups = createCards({ status: "not_installed", cli_path: null });
    renderCards([groups.mainCards.find((item) => item.id === "converter")!]);

    expect(screen.getByText(translations.en_US.unavailableOffline)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: translations.en_US.open })).toBeDisabled();
  });
});
