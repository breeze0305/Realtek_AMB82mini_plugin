import { ChevronDown, ChevronRight, type LucideIcon, RefreshCcw } from "lucide-react";
import type { CSSProperties, Dispatch, SetStateAction } from "react";

import type { DownloadKey, RunningAction } from "../types";

export type ActionMenuKey = string;

export type HomeCard = {
  action: () => void;
  actionIcon: LucideIcon;
  appearance?: "muted" | "complete";
  detail: string;
  disabled: boolean;
  disabledReason?: string;
  id: string;
  icon: LucideIcon;
  key: Exclude<RunningAction, null> | null;
  label: string;
  menuActions?: Array<{
    action: () => void;
    label: string;
  }>;
  title: string;
  wholeCardAction?: boolean;
  wrapDetail?: boolean;
};

export type CardGridProps = {
  cards: HomeCard[];
  downloadProgress: Partial<Record<DownloadKey, number>>;
  isDownloadKey: (key: RunningAction) => key is DownloadKey;
  openActionMenu: ActionMenuKey | null;
  running: RunningAction;
  setOpenActionMenu: Dispatch<SetStateAction<ActionMenuKey | null>>;
  selection?: {
    cardId: string;
    onSelect: (id: string) => void;
    panelId: string;
  };
  startIndex?: number;
  t: Record<string, string>;
};

export function CardGrid({
  cards,
  downloadProgress,
  isDownloadKey,
  openActionMenu,
  running,
  setOpenActionMenu,
  selection,
  startIndex = 1,
  t,
}: CardGridProps) {
  return (
    <div className="menuGrid">
      {cards.map((card, index) => {
        const Icon = card.icon;
        const isRunning = card.key !== null && running === card.key;
        const isActionBlocked = card.key !== null && running !== null;
        const hasOpenActionMenu = card.menuActions !== undefined && openActionMenu === card.id;
        const ActionIcon = card.actionIcon;
        const progress = isDownloadKey(card.key) ? downloadProgress[card.key] : undefined;
        const progressStyle =
          progress === undefined
            ? undefined
            : ({
                "--card-progress": `${Math.max(4, Math.round(progress * 100))}%`,
              } as CSSProperties);

        if (card.wholeCardAction) {
          return (
            <button
              type="button"
              className="menuCard wholeCardAction"
              disabled={card.disabled}
              key={card.id}
              onClick={card.action}
            >
              <span className="cardIndex">{String(startIndex + index).padStart(2, "0")}</span>
              <span className="cardIcon">
                <Icon size={24} />
              </span>
              <span className="cardText wholeCardText">
                <span className="wholeCardTitle">{card.title}</span>
                {(card.disabled || card.detail) && (
                  <span className="wholeCardDetail">
                    {card.disabled ? (card.disabledReason ?? t.unavailableOffline) : card.detail}
                  </span>
                )}
              </span>
              <ChevronRight className="wholeCardChevron" size={20} aria-hidden="true" />
            </button>
          );
        }

        return (
          <article
            className={`menuCard ${progress === undefined ? "" : "isDownloading"} ${
              hasOpenActionMenu ? "hasOpenActionMenu" : ""
            } ${selection ? "isSelectable" : ""} ${selection?.cardId === card.id ? "isSelected" : ""} ${
              card.appearance === "muted" ? "isMuted" : card.appearance === "complete" ? "isComplete" : ""
            } ${card.wrapDetail ? "hasWrappedDetail" : ""}`}
            key={card.id}
            style={progressStyle}
          >
            {selection && (
              <button
                type="button"
                className="cardSelection"
                aria-label={`${t.resourceViewGuide}: ${card.title}`}
                aria-pressed={selection.cardId === card.id}
                aria-controls={selection.panelId}
                onClick={() => selection.onSelect(card.id)}
              />
            )}
            <span className="cardIndex">{String(startIndex + index).padStart(2, "0")}</span>
            <div className="cardIcon">
              <Icon size={24} />
            </div>
            <div className="cardText">
              <h3>{card.title}</h3>
              {(card.disabled || card.detail) && (
                <p>{card.disabled ? (card.disabledReason ?? t.unavailableOffline) : card.detail}</p>
              )}
            </div>
            {card.menuActions ? (
              <div
                className={`splitAction ${hasOpenActionMenu ? "isOpen" : ""}`}
                onBlur={(event) => {
                  const nextTarget = event.relatedTarget as Node | null;
                  if (!nextTarget || !event.currentTarget.contains(nextTarget)) {
                    setOpenActionMenu(null);
                  }
                }}
              >
                <button
                  className="primaryBtn splitMain"
                  onClick={card.action}
                  disabled={card.disabled || isActionBlocked}
                >
                  {isRunning ? <RefreshCcw className="spin" size={17} /> : <ActionIcon size={17} />}
                  {card.label}
                </button>
                <button
                  type="button"
                  className="primaryBtn splitToggle"
                  aria-haspopup="menu"
                  aria-expanded={hasOpenActionMenu}
                  aria-label={card.title}
                  onClick={() => setOpenActionMenu((current) => (current === card.id ? null : card.id))}
                  disabled={card.disabled || isActionBlocked}
                >
                  <ChevronDown size={17} />
                </button>
                {hasOpenActionMenu && (
                  <div className="actionMenu" role="menu">
                    {card.menuActions.map((item) => (
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setOpenActionMenu(null);
                          item.action();
                        }}
                        key={item.label}
                      >
                        {item.label}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            ) : (
              <button className="primaryBtn" onClick={card.action} disabled={card.disabled || isActionBlocked}>
                {isRunning ? <RefreshCcw className="spin" size={17} /> : <ActionIcon size={17} />}
                {card.label}
              </button>
            )}
          </article>
        );
      })}
    </div>
  );
}
