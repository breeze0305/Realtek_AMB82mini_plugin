import { Box, CheckCircle2 } from "lucide-react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { translations } from "../i18n";
import type { DownloadKey, ResourceCategory, RunningAction } from "../types";
import type { HomeCard } from "./CardGrid";
import { ResourceLibraryView } from "./ResourceLibraryView";

function createCard(id: string, title: string, key: HomeCard["key"] = null): HomeCard {
  return {
    action: vi.fn(),
    actionIcon: CheckCircle2,
    detail: "",
    disabled: false,
    icon: Box,
    id,
    key,
    label: "Open",
    title,
  };
}

const resources = [
  { key: "hand", nameKey: "hand", summaryKey: "resourceHandGuideSummary" },
  { key: "box", nameKey: "objectBoxTracking", summaryKey: "resourceBoxGuideSummary" },
  { key: "japan", nameKey: "japanModel", summaryKey: "resourceJapanGuideSummary" },
  { key: "taiwan", nameKey: "taiwanModel", summaryKey: "resourceTaiwanGuideSummary" },
  { key: "singapore", nameKey: "singaporeModel", summaryKey: "resourceSingaporeGuideSummary" },
] as const;

const classificationImageFiles = [
  "classification/model-selection.png",
  "classification/classification-class-list-tab.png",
  "classification/classification-class-list.png",
  "gesture/open-amebapro2-folder.png",
  "classification/weight-folder-location.png",
];

function createWeightCards(t = translations.en_US) {
  return resources.map(({ key, nameKey }) => ({
    ...createCard(`resource-${key}`, t[nameKey], key),
    detail:
      key === "hand"
        ? "hand_code.txt / yolov7_tiny.nb"
        : key === "box"
          ? "code.txt / yolov7_tiny.nb"
          : "img_class_cnn.nb (box/money/mouse)",
  }));
}

function renderResourceLibrary(
  category: ResourceCategory,
  running: RunningAction = null,
  cards = category === "installers" ? [createCard("installer", "Installer card", "arduino")] : createWeightCards(),
  onOpenUrl = vi.fn(),
  t = translations.en_US,
) {
  return render(
    <ResourceLibraryView
      cards={cards}
      category={category}
      onOpenUrl={onOpenUrl}
      downloadProgress={{}}
      isDownloadKey={(key: RunningAction): key is DownloadKey => key === "arduino" || key === "vlc"}
      openActionMenu={null}
      running={running}
      setOpenActionMenu={vi.fn()}
      t={t}
    />,
  );
}

describe("ResourceLibraryView", () => {
  it("renders the installer page without a shared category switcher and preserves its action", async () => {
    const user = userEvent.setup();
    const card = createCard("installer", "Installer card", "arduino");
    renderResourceLibrary("installers", null, [card]);

    expect(screen.getByRole("heading", { name: "Installers" })).toBeInTheDocument();
    expect(screen.getByText("Installer card")).toBeInTheDocument();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /View guide:/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Open" }));

    expect(card.action).toHaveBeenCalledOnce();
  });

  it("blocks resource actions during another operation while allowing guide selection", async () => {
    const user = userEvent.setup();
    const cards = createWeightCards();
    renderResourceLibrary("weights", "arduino", cards);

    expect(screen.getByRole("heading", { name: "Code & Model Weights" })).toBeInTheDocument();
    for (const action of screen.getAllByRole("button", { name: "Open" })) {
      expect(action).toBeDisabled();
      await user.click(action);
    }
    expect(screen.queryByText("Installer card")).not.toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    expect(screen.queryByText(translations.en_US.installerFilesDetail)).not.toBeInTheDocument();

    const secondGuideButton = screen.getByRole("button", { name: `View guide: ${cards[1].title}` });
    expect(secondGuideButton).toBeEnabled();
    await user.click(secondGuideButton);

    expect(screen.getByRole("complementary", { name: cards[1].title })).toBeInTheDocument();
    expect(secondGuideButton).toHaveAttribute("aria-pressed", "true");
    for (const card of cards) expect(card.action).not.toHaveBeenCalled();
  });

  it("shows the first guide by default and switches each card's text and image without retrieving files", async () => {
    const user = userEvent.setup();
    const cards = createWeightCards();
    renderResourceLibrary("weights", null, cards);

    expect(screen.getByRole("complementary", { name: cards[0].title })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `View guide: ${cards[0].title}` })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    for (const [index, card] of cards.entries()) {
      const button = screen.getByRole("button", { name: `View guide: ${card.title}` });
      await user.click(button);

      const guide = screen.getByRole("complementary", { name: card.title });
      expect(button).toHaveAttribute("aria-controls", guide.id);
      expect(within(guide).getByText(translations.en_US[resources[index].summaryKey])).toBeInTheDocument();
      expect(within(guide).getByText(card.detail)).toBeInTheDocument();
      const resourceKey = resources[index].key;
      if (resourceKey === "hand" || resourceKey === "box") {
        const images = within(guide).getAllByRole("img");
        const imageFiles = [
          "gesture/model-selection.png",
          "gesture/object-class-list-tab.png",
          resourceKey === "hand" ? "gesture/gesture-class-list.png" : "box/box-class-list.png",
          "gesture/open-amebapro2-folder.png",
          "gesture/weight-folder-location.png",
          "gesture/car-wiring-diagram.png",
        ];
        expect(images).toHaveLength(6);
        for (const [imageIndex, image] of images.entries()) {
          expect(image).toHaveAttribute("src", expect.stringContaining(`/resource-guides/${imageFiles[imageIndex]}`));
          expect(image.getAttribute("alt")?.trim()).toBeTruthy();
        }
        expect(
          within(guide).getByText("ObjDet.modelSelect(OBJECT_DETECTION, DEFAULT_YOLOV4TINY, NA_MODEL, NA_MODEL);"),
        ).toBeInTheDocument();
        expect(
          within(guide).getByText("ObjDet.modelSelect(OBJECT_DETECTION, CUSTOMIZED_YOLOV7TINY, NA_MODEL, NA_MODEL);"),
        ).toBeInTheDocument();
        expect(
          within(guide).getByText(
            resourceKey === "hand"
              ? translations.en_US.resourceHandClassesBody
              : translations.en_US.resourceBoxClassesBody,
          ),
        ).toHaveTextContent(resourceKey === "hand" ? "itemList[5]" : "itemList[1]");
        expect(
          within(guide).getByText(
            resourceKey === "hand" ? translations.en_US.resourceHandCarBody : translations.en_US.resourceBoxCarBody,
          ),
        ).toHaveTextContent(resourceKey === "hand" ? "hand_code.txt" : "code.txt");
        expect(within(guide).queryByText(translations.en_US.resourceGuidePlaceholder)).not.toBeInTheDocument();
      } else {
        const images = within(guide).getAllByRole("img");
        expect(images).toHaveLength(classificationImageFiles.length);
        for (const [imageIndex, image] of images.entries()) {
          expect(image).toHaveAttribute(
            "src",
            expect.stringContaining(`/resource-guides/${classificationImageFiles[imageIndex]}`),
          );
          expect(image.getAttribute("alt")?.trim()).toBeTruthy();
        }
        expect(
          within(guide).getByText(
            "imgclass.modelSelect(IMAGE_CLASSIFICATION, NA_MODEL, NA_MODEL, NA_MODEL, NA_MODEL, DEFAULT_IMGCLASS);",
          ),
        ).toBeInTheDocument();
        expect(
          within(guide).getByText(
            "imgclass.modelSelect(IMAGE_CLASSIFICATION, NA_MODEL, NA_MODEL, NA_MODEL, NA_MODEL, CUSTOMIZED_IMGCLASS);",
          ),
        ).toBeInTheDocument();
        expect(within(guide).queryByText(translations.en_US.resourceGuidePlaceholder)).not.toBeInTheDocument();
      }
      for (const other of cards) {
        expect(screen.getByRole("button", { name: `View guide: ${other.title}` })).toHaveAttribute(
          "aria-pressed",
          String(other.id === card.id),
        );
        expect(other.action).not.toHaveBeenCalled();
      }
    }
  });

  it("opens the assembly video through the external URL handler without retrieving files", async () => {
    const user = userEvent.setup();
    const cards = createWeightCards();
    const onOpenUrl = vi.fn();
    renderResourceLibrary("weights", null, cards, onOpenUrl);

    for (const selectedCard of cards.slice(0, 2)) {
      await user.click(screen.getByRole("button", { name: `View guide: ${selectedCard.title}` }));
      await user.click(screen.getByRole("button", { name: translations.en_US.resourceHandAssemblyLink }));

      expect(onOpenUrl).toHaveBeenCalledExactlyOnceWith("https://www.youtube.com/watch?v=UpYyOiEFA0k");
      expect(screen.getByRole("complementary", { name: selectedCard.title })).toBeInTheDocument();
      for (const card of cards) expect(card.action).not.toHaveBeenCalled();
      onOpenUrl.mockClear();
    }
  });

  it.each(["zh_TW", "en_US", "ja_JP"] as const)(
    "shows complete box-specific instructions in %s without gesture configuration",
    (language) => {
      const t = translations[language];
      const boxCard = createWeightCards(t)[1];
      renderResourceLibrary("weights", null, [boxCard], vi.fn(), t);

      const guide = screen.getByRole("complementary", { name: boxCard.title });
      expect(within(guide).getByText(t.resourceBoxGuideSummary)).toBeInTheDocument();
      expect(within(guide).getByText(t.resourceBoxClassesBody)).toHaveTextContent("itemList[1]");
      expect(within(guide).getByText(t.resourceBoxClassesBody)).toHaveTextContent("box");
      expect(within(guide).getByText(t.resourceBoxCarBody)).toHaveTextContent(/\bcode\.txt\b/);
      expect(within(guide).getByText(t.resourceHandWeightLocationBody)).toHaveTextContent("yolov7_tiny.nb");
      expect(within(guide).queryByText(t.resourceGuidePlaceholder)).not.toBeInTheDocument();
      expect(within(guide).queryByText(t.resourceImagePlaceholder)).not.toBeInTheDocument();
      expect(guide).not.toHaveTextContent(/itemList\[5\]|gesture[1-5]|hand_code\.txt/);
      for (const image of within(guide).getAllByRole("img")) {
        expect(image.getAttribute("alt")?.trim()).toBeTruthy();
        expect(image).not.toHaveAttribute("alt", t.resourceGuidePlaceholder);
      }
    },
  );

  it.each(["zh_TW", "en_US", "ja_JP"] as const)(
    "shares classification setup across regional weights in %s while preserving their distinct summaries",
    async (language) => {
      const user = userEvent.setup();
      const t = translations[language];
      const cards = createWeightCards(t).slice(2);
      const moneyDescriptions = {
        zh_TW: ["日本硬幣", "台灣紙鈔", "新加坡紙鈔"],
        en_US: ["Japanese coins", "Taiwanese banknotes", "Singaporean banknotes"],
        ja_JP: ["日本の硬貨", "台湾の紙幣", "シンガポールの紙幣"],
      }[language];
      renderResourceLibrary("weights", null, cards, vi.fn(), t);
      let sharedSteps: string[] | undefined;
      const summaries = new Set<string>();

      for (const [index, card] of cards.entries()) {
        await user.click(screen.getByRole("button", { name: `${t.resourceViewGuide}: ${card.title}` }));
        const guide = screen.getByRole("complementary", { name: card.title });
        const summary = t[resources[index + 2].summaryKey];
        expect(within(guide).getByText(summary)).toHaveTextContent(moneyDescriptions[index]);
        summaries.add(summary);

        expect(within(guide).getByText(t.resourceClassificationCodeGuide)).toHaveTextContent("RTSPImageClassification");
        expect(within(guide).getByText(t.resourceClassificationClassTabBody)).toHaveTextContent(
          "ClassificationClassList.h",
        );
        const classInstructions = within(guide).getByText(t.resourceClassificationClassesBody);
        for (const name of ["imgclassItemList", "box", "money", "mouse"]) {
          expect(classInstructions).toHaveTextContent(name);
        }
        const weightInstructions = within(guide).getByText(t.resourceClassificationWeightLocationBody);
        for (const pathPart of [
          "libraries",
          "NeuralNetwork",
          "examples",
          "RTSPImageClassification",
          "img_class_cnn.nb",
        ]) {
          expect(weightInstructions).toHaveTextContent(pathPart);
        }
        expect(within(guide).queryByText(t.resourceGuidePlaceholder)).not.toBeInTheDocument();
        expect(within(guide).queryByText(t.resourceImagePlaceholder)).not.toBeInTheDocument();
        expect(guide).not.toHaveTextContent(/gesture[1-5]|hand_code\.txt|yolov7_tiny\.nb|ObjectDetectionLoop/);
        expect(within(guide).queryByText(t.resourceHandCarBody)).not.toBeInTheDocument();
        expect(within(guide).queryByRole("button", { name: t.resourceHandAssemblyLink })).not.toBeInTheDocument();

        const steps = Array.from(guide.querySelectorAll("section"), (section) => section.textContent ?? "");
        expect(steps).toHaveLength(5);
        if (sharedSteps) expect(steps).toEqual(sharedSteps);
        else sharedSteps = steps;
        for (const [imageIndex, image] of within(guide).getAllByRole("img").entries()) {
          expect(image).toHaveAttribute(
            "src",
            expect.stringContaining(`/resource-guides/${classificationImageFiles[imageIndex]}`),
          );
          expect(image.getAttribute("alt")?.trim()).toBeTruthy();
          expect(image).not.toHaveAttribute("alt", t.resourceGuidePlaceholder);
        }
      }
      expect(summaries.size).toBe(3);
      for (const card of cards) expect(card.action).not.toHaveBeenCalled();
    },
  );

  it("supports Enter and Space for selecting a guide", async () => {
    const user = userEvent.setup();
    const cards = createWeightCards();
    renderResourceLibrary("weights", null, cards);

    screen.getByRole("button", { name: `View guide: ${cards[1].title}` }).focus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("complementary", { name: cards[1].title })).toBeInTheDocument();

    screen.getByRole("button", { name: `View guide: ${cards[2].title}` }).focus();
    await user.keyboard(" ");
    expect(screen.getByRole("complementary", { name: cards[2].title })).toBeInTheDocument();
    for (const card of cards) expect(card.action).not.toHaveBeenCalled();
  });

  it("keeps each retrieve button connected to its original action independently of the selected guide", async () => {
    const user = userEvent.setup();
    const cards = createWeightCards();
    renderResourceLibrary("weights", null, cards);

    const cardTitle = screen.getByRole("heading", { name: cards[1].title });
    const card = cardTitle.closest("article");
    expect(card).not.toBeNull();
    await user.click(within(card!).getByRole("button", { name: "Open" }));

    expect(cards[1].action).toHaveBeenCalledOnce();
    expect(screen.getByRole("complementary", { name: cards[0].title })).toBeInTheDocument();
    for (const other of cards.filter((item) => item.id !== cards[1].id)) {
      expect(other.action).not.toHaveBeenCalled();
    }
  });
});
