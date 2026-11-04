/**
 * App-scope decor layer (L1). Theme effects live here, above the backdrop but
 * below all content, inside their own stacking context so their z-index cannot
 * climb over `.app-body`.
 */
import type { JSX } from "solid-js";
import { ThemeEffects } from "../components/ThemeSurface";

export function DecorLayer(): JSX.Element {
  return (
    <div class="decor-layer" aria-hidden="true">
      <ThemeEffects area="app" />
    </div>
  );
}
