import {
  BrainCircuit,
  Camera,
  CheckCircle2,
  Download,
  ExternalLink,
  FileArchive,
  FolderOpen,
  Images,
  PackageCheck,
  PackageOpen,
  RefreshCcw,
  Tags,
  Terminal,
} from "lucide-react";

import type { HomeCard } from "./components/CardGrid";
import { installActionLabels } from "./i18n";
import type {
  ActionResult,
  ArduinoCliStatus,
  Dashboard,
  DownloadResult,
  InstallationResult,
  InstallerProgress,
  Language,
  ResourceCategory,
  RunningAction,
  VersionCheck,
} from "./types";

type RunAction = <T>(key: Exclude<RunningAction, null>, command: string, next: (result: T) => string) => Promise<void>;

type ResourceCardDefinition = {
  category: ResourceCategory;
  command: string;
  detail: string;
  disabled: boolean;
  key: Exclude<RunningAction, null>;
  title: string;
};

type CreateHomeCardGroupsParams = {
  arduinoCliStatus?: ArduinoCliStatus | null;
  arduinoCliStatusError?: string | null;
  dashboard: Dashboard | null;
  internetConnected: boolean;
  language: Language;
  installerProgress?: Partial<Record<InstallerProgress["key"], InstallerProgress["phase"]>>;
  onArduinoCliStatusChanged?: (status: ArduinoCliStatus) => void;
  onOpenAnnotator: () => void;
  onOpenCamera: () => void;
  onOpenConverter: () => void;
  onOpenImageConverter: () => void;
  onOpenResourceCategory: (category: ResourceCategory) => void;
  onOpenVersionUpdate: () => void;
  onVersionChecked: (result: VersionCheck) => void;
  runAction: RunAction;
  t: Record<string, string>;
  versionCheck: VersionCheck | null;
};

export type HomeCardGroups = {
  installerCards: HomeCard[];
  mainCards: HomeCard[];
  resourceEntryCards: HomeCard[];
  weightCards: HomeCard[];
};

export function createHomeCardGroups({
  arduinoCliStatus = null,
  arduinoCliStatusError = null,
  dashboard,
  internetConnected,
  language,
  installerProgress = {},
  onArduinoCliStatusChanged,
  onOpenAnnotator,
  onOpenCamera,
  onOpenConverter,
  onOpenImageConverter,
  onOpenResourceCategory,
  onOpenVersionUpdate,
  onVersionChecked,
  runAction,
  t,
  versionCheck,
}: CreateHomeCardGroupsParams): HomeCardGroups {
  const hasVersionUpdate = versionCheck !== null && !versionCheck.is_latest && !versionCheck.is_beta;
  const resourceDefinitions: ResourceCardDefinition[] = [
    {
      category: "installers",
      title: t.driver,
      detail: "CH341SER.EXE",
      command: "save_driver_as",
      key: "driver",
      disabled: false,
    },
    {
      category: "installers",
      title: t.arduino,
      detail: "arduino-ide_latest_Windows_64bit.exe",
      command: "download_arduino_ide_as",
      key: "arduino",
      disabled: false,
    },
    {
      category: "installers",
      title: t.vlc,
      detail: "vlc-3.0.23-win32.exe",
      command: "download_vlc_as",
      key: "vlc",
      disabled: false,
    },
    {
      category: "weights",
      title: t.hand,
      detail: "hand_code.txt / yolov7_tiny.nb",
      command: "save_hand_resources_as",
      key: "hand",
      disabled: false,
    },
    {
      category: "weights",
      title: t.objectBoxTracking,
      detail: "code.txt / yolov7_tiny.nb",
      command: "save_object_detection_box_resources_as",
      key: "box",
      disabled: false,
    },
    {
      category: "weights",
      title: t.japanModel,
      detail: "img_class_cnn.nb(box/money/mouse)",
      command: "save_image_model_japan_as",
      key: "japan",
      disabled: false,
    },
    {
      category: "weights",
      title: t.taiwanModel,
      detail: "img_class_cnn.nb(box/money/mouse)",
      command: "save_image_model_taiwan_as",
      key: "taiwan",
      disabled: false,
    },
    {
      category: "weights",
      title: t.singaporeModel,
      detail: "img_class_cnn.nb(box/money/mouse)",
      command: "save_image_model_singapore_as",
      key: "singapore",
      disabled: false,
    },
  ];

  function createResourceCard(card: ResourceCardDefinition): HomeCard {
    const installerPhase = card.key === "arduino" || card.key === "vlc" ? installerProgress[card.key] : undefined;
    const phaseLabel =
      installerPhase === "installing"
        ? t.installerInstalling
        : installerPhase === "configuring_path"
          ? t.arduinoCliConfiguring
          : undefined;
    const action = () =>
      void runAction<ActionResult | DownloadResult>(
        card.key,
        card.command,
        (result) => result.path ?? ("message" in result ? result.message : ""),
      );

    return {
      id: `resource-${card.key}`,
      title: card.title,
      detail: installerPhase ? t.installerWaitingDetail : card.detail,
      wrapDetail: installerPhase !== undefined,
      icon: card.category === "installers" ? PackageCheck : BrainCircuit,
      action,
      menuActions:
        card.key === "arduino" || card.key === "vlc"
          ? [
              {
                label: installActionLabels[language].autoInstall,
                action: () =>
                  void runAction<InstallationResult>(
                    card.key,
                    card.key === "arduino" ? "download_and_install_arduino_ide" : "download_and_install_vlc",
                    (result) => {
                      if (result.arduino_cli) onArduinoCliStatusChanged?.(result.arduino_cli);
                      const messages = [t.installerComplete.replace("{app}", card.title)];
                      if (result.path_error) {
                        messages.push(`${t.arduinoCliAutomaticPathFailed} ${result.path_error}`);
                      } else if (result.arduino_cli?.status === "on_path") {
                        messages.push(t.arduinoCliPathSuccess);
                      }
                      if (result.reboot_required) messages.push(t.installerRebootRequired);
                      return messages.join("\n");
                    },
                  ),
              },
            ]
          : undefined,
      label: phaseLabel ?? t.save,
      disabled: card.disabled,
      key: card.key,
      actionIcon: Download,
    };
  }

  const installerCards = resourceDefinitions.filter((card) => card.category === "installers").map(createResourceCard);
  const cliState = arduinoCliStatusError ? null : arduinoCliStatus?.status;
  const cliDetail = arduinoCliStatusError
    ? t.arduinoCliStatusUnavailable
    : cliState === "not_installed"
      ? t.arduinoCliNotInstalledDetail
      : cliState === "not_on_path"
        ? t.arduinoCliNotOnPathDetail
        : cliState === "on_path"
          ? t.arduinoCliOnPathDetail
          : t.arduinoCliChecking;
  const cliLabel = arduinoCliStatusError
    ? t.arduinoCliStatusUnavailableLabel
    : cliState === "not_installed"
      ? t.arduinoCliNotInstalled
      : cliState === "not_on_path"
        ? t.arduinoCliAddPath
        : cliState === "on_path"
          ? t.arduinoCliOnPath
          : t.arduinoCliChecking;
  installerCards.splice(2, 0, {
    id: "resource-arduinoCliPath",
    title: t.arduinoCliPathTitle,
    appearance: cliState === "on_path" ? "complete" : cliState === "not_on_path" ? undefined : "muted",
    detail: cliDetail,
    wrapDetail: true,
    disabledReason: cliDetail,
    icon: cliState === "on_path" ? CheckCircle2 : Terminal,
    key: "arduinoCliPath",
    label: cliLabel,
    disabled: cliState !== "not_on_path",
    actionIcon: cliState === "on_path" ? CheckCircle2 : Terminal,
    action: () => {
      if (cliState !== "not_on_path") return;
      void runAction<ArduinoCliStatus>("arduinoCliPath", "add_arduino_cli_to_path", (result) => {
        onArduinoCliStatusChanged?.(result);
        return result.status === "on_path" ? t.arduinoCliPathSuccess : t.arduinoCliStatusUnavailable;
      });
    },
  });
  const weightCards = resourceDefinitions.filter((card) => card.category === "weights").map(createResourceCard);

  const resourceEntryCards: HomeCard[] = [
    {
      id: "resource-installers",
      title: t.installerFiles,
      detail: t.installerFilesSummary,
      icon: PackageOpen,
      action: () => onOpenResourceCategory("installers"),
      label: t.open,
      disabled: false,
      key: null,
      actionIcon: CheckCircle2,
      wholeCardAction: true,
    },
    {
      id: "resource-weights",
      title: t.modelResources,
      detail: t.modelResourcesSummary,
      icon: BrainCircuit,
      action: () => onOpenResourceCategory("weights"),
      label: t.open,
      disabled: false,
      key: null,
      actionIcon: CheckCircle2,
      wholeCardAction: true,
    },
  ];

  const mainCards: HomeCard[] = [
    {
      id: "camera",
      title: t.camera,
      detail: "",
      icon: Camera,
      action: onOpenCamera,
      label: t.open,
      disabled: false,
      key: null,
      actionIcon: CheckCircle2,
    },
    {
      id: "converter",
      title: t.modelConverter,
      detail: "",
      icon: FileArchive,
      action: onOpenConverter,
      label: t.open,
      disabled: !internetConnected,
      key: null,
      actionIcon: CheckCircle2,
    },
    {
      id: "annotator",
      title: t.objectAnnotator,
      detail: "",
      icon: Tags,
      action: onOpenAnnotator,
      label: t.open,
      disabled: false,
      key: null,
      actionIcon: CheckCircle2,
    },
    {
      id: "image-converter",
      title: t.imageConverter,
      detail: "",
      icon: Images,
      action: onOpenImageConverter,
      label: t.open,
      disabled: false,
      key: null,
      actionIcon: CheckCircle2,
    },
    {
      id: "realtek-folder",
      title: t.folder,
      detail: "",
      icon: FolderOpen,
      action: () =>
        void runAction<ActionResult>("folder", "open_realtek_folder", (result) => result.path ?? result.message),
      label: t.open,
      disabled: false,
      key: "folder",
      actionIcon: CheckCircle2,
    },
    {
      id: "version",
      title: t.version,
      detail: dashboard ? `v${dashboard.metadata.version}` : "",
      icon: RefreshCcw,
      action: hasVersionUpdate
        ? onOpenVersionUpdate
        : () =>
            void runAction<VersionCheck>("version", "check_version", (result) => {
              onVersionChecked(result);
              if (result.is_beta) return t.betaCurrent;
              if (result.is_latest) return `${t.latest}: ${result.local}`;
              return `${t.update}: ${result.remote}`;
            }),
      label: hasVersionUpdate ? t.updateButton : t.check,
      disabled: !internetConnected,
      key: "version",
      actionIcon: hasVersionUpdate ? ExternalLink : CheckCircle2,
    },
  ];

  return {
    installerCards,
    mainCards,
    resourceEntryCards,
    weightCards,
  };
}
