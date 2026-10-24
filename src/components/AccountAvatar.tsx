/**
 * Account avatar (ui-design 06 §3): Microsoft accounts get the Minecraft face
 * cropped out of the skin texture published by Mojang's session server —
 * fetched through the backend (`auth_skin_url`) because WKWebView blocks the
 * direct call with CORS and crafatar-style mirrors proved unreliable
 * (HTTP 500 → default Alex). Offline accounts get the Steve face, matching
 * how the game itself renders skinless profiles.
 */
import { Show, createResource } from "solid-js";
import type { JSX } from "solid-js";
import { initialOf } from "../lib/format";
import { auth } from "../ipc/api";
import steveFace from "../assets/steve-face.png";

const textureCache = new Map<string, Promise<string | null>>();

/** profile UUID (any dash format) → skin texture URL, or null when unavailable. */
function faceTextureUrl(uuid: string): Promise<string | null> {
  const undashed = uuid.replace(/-/g, "").toLowerCase();
  let pending = textureCache.get(undashed);
  if (!pending) {
    pending = auth.skinUrl(uuid).then((res) => (res.ok ? res.data : null));
    textureCache.set(undashed, pending);
  }
  return pending;
}

/**
 * Square avatar: skin-face crop for Microsoft accounts, Steve face for offline
 * accounts, initial letter as the last-resort fallback. `small` matches the
 * account list rows.
 */
export function AccountAvatar(props: {
  uuid?: string | null;
  name: string;
  /** `microsoft` uses the Mojang skin; anything else gets Steve. */
  provider?: string;
  small?: boolean;
}): JSX.Element {
  const useSkin = () => props.provider === "microsoft" && !!props.uuid && props.uuid.trim() !== "";
  const [texture] = createResource(
    () => (useSkin() ? (props.uuid as string).replace(/-/g, "").toLowerCase() : null),
    async (undashed) => faceTextureUrl(undashed)
  );
  const offlineSteve = () => !useSkin();
  return (
    <Show
      when={texture() ?? (offlineSteve() ? steveFace : null)}
      fallback={
        <div class={`account-avatar${props.small ? " is-small" : ""}`}>{initialOf(props.name)}</div>
      }
    >
      {(url) => (
        <div
          class={`account-avatar-face${props.small ? " is-small" : ""}${
            offlineSteve() ? " is-builtin" : ""
          }`}
          style={{ "background-image": `url("${url()}")` }}
          title={props.name}
        />
      )}
    </Show>
  );
}
