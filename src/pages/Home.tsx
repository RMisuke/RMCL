/**
 * Home is intentionally blank: the wallpaper/video backdrop fills the window
 * and the launch capsule lives in the global shell (bottom-right). No cards,
 * no greeting, no in-page chrome.
 */
import type { JSX } from "solid-js";

export default function Home(): JSX.Element {
  return <div class="home-blank" aria-hidden="true" />;
}
