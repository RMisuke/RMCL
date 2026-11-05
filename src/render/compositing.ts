/**
 * Dev-only compositing guard (ui-render-refactor D2 / `02_RENDER_TIER_ENGINE.md` §4).
 *
 * Makes the render-layer invariants observable instead of relying on reviewer
 * memory: outside the `full` tier no element may run a real backdrop-filter
 * (card surfaces are the deliberate exception — they follow the user's blur
 * slider), and no full-screen layer may use `mix-blend-mode`. Production builds
 * skip all of this (the body returns immediately; `import.meta.env.DEV` is false).
 */
const CARD_SURFACES =
  ".soft-card, .soft-launch-card, .rm-card, .instance-card, .loader-card, .skin-panel";

export function installCompositingGuard(): () => void {
  if (!import.meta.env.DEV || typeof window === "undefined") return () => {};

  const warn = (message: string) => console.warn(`[render-guard] ${message}`);

  function check(): void {
    const tier = document.documentElement.dataset.renderTier ?? "unknown";
    const viewport = window.innerWidth * window.innerHeight;
    let blurCount = 0;
    let fullBlend = 0;
    const nodes = document.querySelectorAll<HTMLElement>(".app-root *");
    nodes.forEach((el) => {
      const style = window.getComputedStyle(el);
      const backdrop = style.getPropertyValue("backdrop-filter");
      if (backdrop && backdrop !== "none" && !el.closest(CARD_SURFACES)) blurCount += 1;
      const blend = style.getPropertyValue("mix-blend-mode");
      if (blend && blend !== "normal") {
        const rect = el.getBoundingClientRect();
        if (rect.width * rect.height > viewport * 0.6) fullBlend += 1;
      }
    });
    if (tier !== "full" && blurCount > 0) {
      warn(`${blurCount} backdrop-filter surface(s) active outside the full tier`);
    }
    if (fullBlend > 0) {
      warn(`${fullBlend} full-screen mix-blend-mode layer(s)`);
    }
  }

  const first = window.setTimeout(check, 1800);
  const second = window.setTimeout(check, 6000);
  return () => {
    window.clearTimeout(first);
    window.clearTimeout(second);
  };
}
