/**
 * Account avatar (ui-design 06 §3): Microsoft accounts get the Minecraft face
 * cropped out of the skin texture published by Mojang's session server.
 *
 * 缓存链路（感知不到缓存，但数据保持新鲜）：
 * 1. 首帧直读 localStorage 里上次记录的皮肤 URL → `rmcccicon://` 磁盘直出，
 *    启动器刚打开就显示头像（零网络）；
 * 2. 后台向 sessionserver 重验 URL（换皮肤 = URL 变化），变化即换图；
 * 3. `cache_icon_fetch` 条件刷新缓存文件（ETag/304），失败保留旧档。
 * 离线账号固定 Steve 脸。
 */
import { Show, createResource, createSignal, createEffect } from "solid-js";
import type { JSX } from "solid-js";
import { initialOf } from "../lib/format";
import { auth } from "../ipc/api";
import { cacheStore } from "../stores/cache";
import steveFace from "../assets/steve-face.png";

const textureCache = new Map<string, Promise<string | null>>();

/** Cache key `uuid@rev` → skin texture URL, or null when unavailable. The rev
 *  comes from cacheStore.avatarRev (bumped on warmAvatar), so a changed skin
 *  re-resolves instead of serving the old texture until restart (P2-9). */
function faceTextureUrl(key: string): Promise<string | null> {
  let pending = textureCache.get(key);
  if (!pending) {
    const uuid = key.split("@")[0];
    pending = auth.skinUrl(uuid).then((res) => (res.ok ? res.data?.skin ?? null : null));
    textureCache.set(key, pending);
  }
  return pending;
}

/** 上次会话记录的皮肤 URL —— 首帧零网络直出。 */
function rememberedTextureUrl(uuid: string): string | null {
  try {
    return localStorage.getItem(`rmccc.avatarUrl.${uuid.replace(/-/g, "")}`);
  } catch {
    return null;
  }
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
  const useSkin = () =>
    props.provider === "microsoft" && !!props.uuid && props.uuid.trim() !== "";
  const undashed = () => (useSkin() ? (props.uuid as string).replace(/-/g, "").toLowerCase() : null);
  const cacheKey = () =>
    undashed() ? `${undashed()}@${cacheStore.avatarRev()[undashed()!] ?? 0}` : null;

  const [texture] = createResource(cacheKey, faceTextureUrl);

  // URL 解析失败/未完成时用上次记住的 URL（首帧命中磁盘缓存）。
  const effective = (): string | null =>
    texture() ?? (undashed() ? rememberedTextureUrl(undashed()!) : null);

  // 拿到新 URL：记住 + 后台条件刷新缓存（不阻塞渲染）。
  createEffect(() => {
    const url = texture();
    if (!url || !undashed()) return;
    try {
      localStorage.setItem(`rmccc.avatarUrl.${undashed()!}`, url);
    } catch {
      /* 仅影响下次首帧 */
    }
    cacheStore.refreshIcon(url, "avatar");
  });

  // 图片回退链：缓存协议 → 直连原图 → 首字母。
  const [stage, setStage] = createSignal<"cache" | "direct" | "none">("cache");
  createEffect(() => {
    void effective();
    setStage("cache");
  });
  const src = () => {
    const url = effective();
    if (!url) return null;
    if (stage() === "none") return null;
    return stage() === "cache" ? cacheStore.iconSrc(url) : url;
  };

  const offlineSteve = () => !useSkin();
  return (
    <Show
      when={src() ?? (offlineSteve() ? steveFace : null)}
      fallback={
        <div class={`account-avatar${props.small ? " is-small" : ""}`}>{initialOf(props.name)}</div>
      }
    >
      {(url) => (
        <div
          class={`account-avatar-face${props.small ? " is-small" : ""}${
            offlineSteve() ? " is-builtin" : ""
          }`}
          style={{
            "background-image": `url("${offlineSteve() ? steveFace : url()}")`,
          }}
          title={props.name}
        >
          <Show when={!offlineSteve() && stage() !== "none"}>
            <img
              src={url()}
              alt=""
              style={{ width: "0", height: "0", visibility: "hidden" }}
              onError={() => setStage(stage() === "cache" ? "direct" : "none")}
            />
          </Show>
        </div>
      )}
    </Show>
  );
}
