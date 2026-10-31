/**
 * Inline SVG icon subset (ui-design 01 §6).
 *
 * Stroke-based 24px icons on `currentColor`; no icon fonts, no CDN, no whole
 * Material Symbols pack (bundle red line, ADR-011 budget). Selected states are
 * handled by the consumer (e.g. the rail uses weight + accent color), not by
 * separate `_fill` files.
 */
import type { JSX } from "solid-js";

export type IconName =
  | "home"
  | "gamepad"
  | "download"
  | "person"
  | "extension"
  | "settings"
  | "activity"
  | "play"
  | "square"
  | "search"
  | "refresh"
  | "plus"
  | "x"
  | "check"
  | "chevron-down"
  | "chevron-up"
  | "chevron-right"
  | "chevron-left"
  | "more-horizontal"
  | "copy"
  | "external"
  | "folder"
  | "file"
  | "trash"
  | "edit"
  | "lock"
  | "unlock"
  | "alert"
  | "info"
  | "sparkles"
  | "shield"
  | "pause"
  | "cpu"
  | "wrench"
  | "package"
  | "grid"
  | "list"
  | "clock"
  | "bot"
  | "key"
  | "log-out"
  | "sliders"
  | "eye"
  | "eye-off"
  | "terminal"
  | "layers"
  | "hard-drive"
  | "image";

const PATHS: Record<IconName, string> = {
  home: "M3 10.5 12 3l9 7.5M5 9.5V21h14V9.5M9.5 21v-6h5v6",
  gamepad: "M6 12h4M8 10v4M15.5 11h.01M17.5 13h.01M7 6h10a5 5 0 0 1 5 5v3a5 5 0 0 1-5 5H7a5 5 0 0 1-5-5v-3a5 5 0 0 1 5-5Z",
  download: "M12 3v12m0 0 4.5-4.5M12 15l-4.5-4.5M4 19h16",
  person: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm-7.5 9a7.5 7.5 0 0 1 15 0",
  extension: "M10 4.5a2 2 0 1 1 4 0H17a1 1 0 0 1 1 1V9a2 2 0 1 1 0 4v4a1 1 0 0 1-1 1h-3.5a2 2 0 1 0-4 0H6a1 1 0 0 1-1-1v-3.5a2 2 0 1 1 0-4V5.5a1 1 0 0 1 1-1h4Z",
  settings: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm7.4-3a7.4 7.4 0 0 0-.1-1.2l2-1.5-2-3.5-2.4 1a7.6 7.6 0 0 0-2-1.2L14.5 3h-5l-.4 2.6a7.6 7.6 0 0 0-2 1.2l-2.4-1-2 3.5 2 1.5a7.4 7.4 0 0 0 0 2.4l-2 1.5 2 3.5 2.4-1a7.6 7.6 0 0 0 2 1.2l.4 2.6h5l.4-2.6a7.6 7.6 0 0 0 2-1.2l2.4 1 2-3.5-2-1.5c.1-.4.1-.8.1-1.2Z",
  activity: "M3 12h4l3-8 4 16 3-8h4",
  play: "M7 4.5 19 12 7 19.5v-15Z",
  square: "M6 6h12v12H6z",
  search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13Zm9.5 3-4.9-4.9",
  refresh: "M20 5v5h-5M4 19v-5h5m11-1a8 8 0 0 1-14.6 4M4.5 11A8 8 0 0 1 19 7",
  plus: "M12 5v14M5 12h14",
  x: "M6 6l12 12M18 6 6 18",
  check: "M4.5 12.5 10 18 19.5 6.5",
  "chevron-down": "M6 9.5l6 6 6-6",
  "chevron-up": "M6 14.5l6-6 6 6",
  "chevron-right": "M9.5 6l6 6-6 6",
  "chevron-left": "M14.5 6l-6 6 6 6",
  "more-horizontal": "M5.5 12h.01M12 12h.01M18.5 12h.01",
  copy: "M8 8h11v11H8zM5 16H4V4h12v1",
  external: "M14 4h6v6M20 4 11 13M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5",
  folder: "M3 6.5A1.5 1.5 0 0 1 4.5 5h5L12 8h7.5A1.5 1.5 0 0 1 21 9.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5v-12Z",
  file: "M6 3h8l4 4v14H6zM14 3v4h4",
  trash: "M4 7h16M9 7V4h6v3m-9 0 1 14h10l1-14M10 11v6M14 11v6",
  edit: "M4 20h4L20 8l-4-4L4 16v4ZM14.5 5.5l4 4",
  lock: "M7 11V8a5 5 0 0 1 10 0v3M5.5 11h13V21h-13zM12 15v2.5",
  unlock: "M7 11V8a5 5 0 0 1 9.9-1M5.5 11h13V21h-13zM12 15v2.5",
  alert: "M12 3 2.5 20h19L12 3Zm0 6.5V14m0 3h.01",
  info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm0-13h.01M12 11v6",
  sparkles: "M12 4l1.8 4.7L18.5 10.5l-4.7 1.8L12 17l-1.8-4.7L5.5 10.5l4.7-1.8L12 4ZM19 15l.9 2.1L22 18l-2.1.9L19 21l-.9-2.1L16 18l2.1-.9L19 15Z",
  shield: "M12 3 5 6v5c0 4.5 3 8.4 7 10 4-1.6 7-5.5 7-10V6l-7-3Z",
  pause: "M8 5v14M16 5v14",
  cpu: "M8 8h8v8H8zM5 5h14v14H5zM9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3",
  wrench: "M14.5 6.5a4.5 4.5 0 0 1 5.7-4.3l-3.2 3.2 1.6 1.6 3.2-3.2a4.5 4.5 0 0 1-6 5.6L7 18.2a2 2 0 1 1-2.8-2.8L13 6.6",
  package: "M12 3 3 7.5v9L12 21l9-4.5v-9L12 3Zm0 0v18M3 7.5 12 12l9-4.5",
  grid: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
  list: "M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01",
  clock: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm0-13.5V12l3.5 2",
  bot: "M9 3h6M12 3v3M5 6h14a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1Zm4 5v2m6-2v2M9.5 16.5h5",
  key: "M15 9a6 6 0 1 0-5.6 6L11 17h2v2h2v2h4v-3l-4.4-4.4A6 6 0 0 0 15 9Zm-2.5 0h.01",
  "log-out": "M9 21H4V3h5m7 13 5-4-5-4m5 4H10",
  sliders: "M4 8h10M18 8h2M4 16h4M12 16h8M14 5.5v5M8 13.5v5",
  eye: "M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Zm9.5 2.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z",
  "eye-off": "M4 4l16 16M10 5.9A9.7 9.7 0 0 1 12 5.7c6 0 9.5 6.3 9.5 6.3a17 17 0 0 1-3 3.7m-3.4 1.6A9.4 9.4 0 0 1 12 18.3c-6 0-9.5-6.3-9.5-6.3a17 17 0 0 1 4-4.6M9.9 9.9a3 3 0 0 0 4.2 4.2",
  terminal: "M4 5h16v14H4zM7.5 9.5 10 12l-2.5 2.5M12.5 15H17",
  layers: "M12 3 3 8l9 5 9-5-9-5ZM3 12.5l9 5 9-5M3 17l9 5 9-5",
  "hard-drive": "M4 5h16v14H4zM4 12h16M8 8.5h.01M8 15.5h.01",
  image: "M4 5h16v14H4zM4 15l4.5-4.5 4 4L15 11l5 5M9 9.5h.01",
};

interface IconProps {
  name: IconName;
  size?: number;
  class?: string;
}

/** Inline stroke icon; color comes from `currentColor`. */
export function Icon(props: IconProps): JSX.Element {
  return (
    <svg
      class={props.class}
      width={props.size ?? 20}
      height={props.size ?? 20}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[props.name]} />
    </svg>
  );
}
