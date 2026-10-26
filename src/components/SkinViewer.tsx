/**
 * 3D 玩家模型预览（skinview3d, MIT — github.com/bs-community/skinview3d）。
 *
 * - 皮肤/披风：正版玩家取 sessionserver 纹理 URL，经图标缓存协议拿字节后
 *   转 blob 喂给 WebGL（绕开跨域纹理限制）；离线玩家用内置 Steve 皮肤。
 * - 交互：OrbitControls 拖拽旋转/滚轮缩放（库自带）；默认走路动画，
 *   可切换姿势、重置视角；正版"更换皮肤"跳官网编辑页，离线可选本地 png。
 * - 库走动态 import 独立分包（three.js 体积大，不进首屏 chunk）。
 *
 * 渲染架构（三轮踩坑后的定版，改动前先读）：
 * 1. canvas 归"常驻舞台层"所有——一个挂在 document.body 上的
 *    `position:fixed` 元素，应用生命周期内只创建一次；组件挂载时把舞台
 *    按占位元素的 getBoundingClientRect() 定位过去（几何同步），卸载时
 *    隐藏舞台并暂停渲染。**canvas 节点从不移动、从不重建。**
 *    背景：把带 WebGL 上下文的 canvas 在 DOM 里摘下再挂回（旧的单例
 *    re-attach 方案），WKWebView 的合成层不会跟着重建——rAF 照跑、上下文
 *    活着、纹理加载成功，但画面永久空白（日志可证：contextLost=false）。
 * 2. viewer 只建一次（首个 50×80 以上的占位出现时），尺寸随舞台同步。
 * 3. 皮肤 blob URL 模块级记忆化：同一 URL 不再重复走 rmcccicon 协议。
 *    快速切页/切账户时的协议请求风暴会放大 wry scheme-task 竞速（曾致
 *    主线程 panic abort 闪退）。
 */
import { Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import type { JSX } from "solid-js";
import { RmButton, RmIconButton } from "./rm/controls";
import { auth, external } from "../ipc/api";
import { cacheStore } from "../stores/cache";
import { authStore } from "../stores/auth";
import { toast } from "../stores/ui";
import { t } from "../i18n";
import steveSkin from "../assets/steve-skin.png";

type PoseId = "walk" | "run" | "fly" | "idle";
const POSES: PoseId[] = ["walk", "run", "fly", "idle"];

/** 动态分包的库引用（模块级缓存，姿势切换复用）。 */
let svLib: typeof import("skinview3d") | null = null;

/** 常驻舞台：canvas + viewer 的唯一宿主，全应用生命周期一份。 */
interface SkinStage {
  root: HTMLDivElement;
  canvas: HTMLCanvasElement;
  viewer: any;
}
let stage: SkinStage | null = null;
let stageInit: Promise<SkinStage> | null = null;

/** url → blob: URL 记忆化（WebGL 纹理需要同源；同一皮肤不重复请求协议）。
 *  上限 8 条（P2-8）：淘汰最旧条目时 revoke，blob 不再无界驻留。 */
const blobUrls = new Map<string, string>();
const BLOB_CACHE_MAX = 8;

function rememberBlob(url: string, obj: string): string {
  blobUrls.set(url, obj);
  while (blobUrls.size > BLOB_CACHE_MAX) {
    const oldest = blobUrls.keys().next().value;
    if (oldest === undefined) break;
    URL.revokeObjectURL(blobUrls.get(oldest)!);
    blobUrls.delete(oldest);
  }
  return obj;
}

async function toObjectUrl(url: string): Promise<string | null> {
  const memo = blobUrls.get(url);
  if (memo) return memo;
  try {
    const res = await fetch(cacheStore.iconSrc(url));
    if (res.ok) {
      return rememberBlob(url, URL.createObjectURL(await res.blob()));
    }
  } catch {
    /* 缓存协议不可用时直连 */
  }
  try {
    const res = await fetch(url, { mode: "cors" });
    if (res.ok) {
      return rememberBlob(url, URL.createObjectURL(await res.blob()));
    }
  } catch {
    /* 直连也失败（离线）→ 由调用方回退 */
  }
  return null;
}

/** 诊断：写入启动器日志（reportFrontendMessage 通道），便于远程排查渲染问题。 */
function diag(message: string): void {
  void import("../ipc/api").then(({ reportFrontendMessage }) =>
    reportFrontendMessage(`[skinview] ${message}`)
  );
}

/** 创建（或复用）常驻舞台。w/h 为首个占位的尺寸，避免 0×0 画布。 */
function ensureStage(w: number, h: number): Promise<SkinStage> {
  if (stage) {
    if (stage.viewer.width !== w || stage.viewer.height !== h) {
      stage.viewer.width = w;
      stage.viewer.height = h;
    }
    return Promise.resolve(stage);
  }
  stageInit ??= (async () => {
    const lib = await import("skinview3d");
    svLib = lib;
    const root = document.createElement("div");
    root.className = "skin-stage";
    document.body.appendChild(root);
    const canvas = document.createElement("canvas");
    root.appendChild(canvas);
    const viewer = new lib.SkinViewer({
      canvas,
      width: Math.max(w, 50),
      height: Math.max(h, 80),
    });
    viewer.zoom = 0.85;
    viewer.animation = new lib.WalkingAnimation();
    // 建好即暂停：等首个组件挂载（几何同步到位）再恢复渲染。
    viewer.renderPaused = true;
    canvas.addEventListener("webglcontextlost", () => diag("webgl context lost"));
    canvas.addEventListener("webglcontextrestored", () => diag("webgl context restored"));
    stage = { root, canvas, viewer };
    diag(`stage created at ${Math.max(w, 50)}x${Math.max(h, 80)}`);
    return stage;
  })();
  return stageInit;
}

function hideStage(): void {
  if (!stage) return;
  stage.root.classList.remove("is-live");
  stage.viewer.renderPaused = true;
}

export function SkinViewer(): JSX.Element {
  let slot: HTMLDivElement | undefined;
  let fileInput: HTMLInputElement | undefined;
  let viewer: any = null;
  let resizeObserver: ResizeObserver | null = null;
  let onWindowResize: (() => void) | null = null;
  let alive = true;
  let attached = false;
  let attaching: Promise<void> | null = null;
  /** 加载代际：快速连续换肤时，晚到的旧结果不得覆盖新结果。 */
  let loadGen = 0;
  const [ready, setReady] = createSignal(false);
  const [pose, setPose] = createSignal<PoseId>("walk");
  const [customSkin, setCustomSkin] = createSignal<string | null>(null);

  const session = () => authStore.session();
  const isMicrosoft = () => session()?.provider === "microsoft";

  /** 载入（或重载）当前账号的皮肤 + 披风。 */
  async function loadCurrentSkin(): Promise<void> {
    const v = viewer ?? stage?.viewer;
    if (!v) return;
    const gen = ++loadGen;
    const custom = customSkin();
    if (custom) {
      v.loadSkin(custom);
      v.loadCape(null);
      return;
    }
    if (!isMicrosoft() || !session()?.uuid) {
      v.loadSkin(steveSkin);
      v.loadCape(null);
      return;
    }
    const res = await auth.skinUrl(session()!.uuid);
    if (gen !== loadGen) return; // 已被更新的加载取代
    if (!res.ok) {
      v.loadSkin(steveSkin);
      return;
    }
    const textures = res.data;
    if (textures?.skin) {
      const blob = await toObjectUrl(textures.skin);
      if (gen !== loadGen) return;
      if (blob) {
        v.loadSkin(blob, { model: "auto-detect" });
        diag("skin loaded from sessionserver");
        void cacheStore.refreshIcon(textures.skin, "avatar");
      } else {
        diag("skin texture fetch failed; using steve fallback");
        v.loadSkin(steveSkin);
      }
    } else {
      diag("no skin on profile; using steve fallback");
      v.loadSkin(steveSkin);
    }
    if (textures?.cape) {
      const capeBlob = await toObjectUrl(textures.cape);
      if (gen !== loadGen) return;
      if (capeBlob) {
        v.loadCape(capeBlob);
        void cacheStore.refreshIcon(textures.cape, "avatar");
      }
    } else {
      v.loadCape(null);
    }
  }

  function applyPose(id: PoseId): void {
    const v = viewer ?? stage?.viewer;
    if (!v || !svLib) return;
    const anim =
      id === "walk" ? new svLib.WalkingAnimation()
      : id === "run" ? new svLib.RunningAnimation()
      : id === "fly" ? new svLib.FlyingAnimation()
      : new svLib.IdleAnimation();
    anim.speed = 1;
    v.animation = anim;
  }

  /** 舞台矩形同步到占位元素（几何挂接，canvas 不动）。 */
  function syncRect(): void {
    if (!slot || !stage) return;
    const rect = slot.getBoundingClientRect();
    if (rect.width < 10 || rect.height < 10) return;
    const s = stage.root.style;
    s.left = `${rect.left}px`;
    s.top = `${rect.top}px`;
    s.width = `${rect.width}px`;
    s.height = `${rect.height}px`;
    if (stage.viewer.width !== rect.width || stage.viewer.height !== rect.height) {
      stage.viewer.width = rect.width;
      stage.viewer.height = rect.height;
    }
  }

  /** Init failure message (P2-13): replaces the eternal "loading" placeholder. */
  const [failed, setFailed] = createSignal<string | null>(null);

  /** 挂接常驻舞台（幂等；占位尺寸就绪后才执行）。 */
  function scheduleAttach(): void {
    if (attached || attaching || !slot) return;
    const w = slot.clientWidth;
    const h = slot.clientHeight;
    if (w < 50 || h < 80) return; // 布局未就绪，等 ResizeObserver 下一次回调
    attaching = (async () => {
      setFailed(null);
      try {
        const st = await ensureStage(w, h);
        // await 期间组件可能已卸载（快速切页）：立刻隐藏，保持舞台静默。
        if (!alive) {
          hideStage();
          return;
        }
        stage = st;
        viewer = st.viewer;
        syncRect();
        st.root.classList.add("is-live");
        st.viewer.renderPaused = false;
        setReady(true);
        setPose("walk");
        applyPose("walk");
        diag(`stage attached at ${w}x${h}`);
        attached = true;
        void loadCurrentSkin();
      } catch (e) {
        setFailed(String(e));
        diag(`stage init failed: ${String(e)}`);
      } finally {
        attaching = null;
      }
    })();
  }

  onMount(() => {
    if (!slot) return;
    scheduleAttach();
    resizeObserver = new ResizeObserver(() => {
      scheduleAttach(); // 布局晚就绪时的补挂接
      if (attached) syncRect();
    });
    resizeObserver.observe(slot);
    onWindowResize = () => {
      if (attached) syncRect();
    };
    window.addEventListener("resize", onWindowResize);
  });

  // 顶层注册清理（async 回调内的 onCleanup 不会挂到组件 owner）。
  // 只隐藏舞台 + 暂停渲染：canvas/viewer 常驻复用，绝不 dispose。
  onCleanup(() => {
    alive = false;
    resizeObserver?.disconnect();
    resizeObserver = null;
    if (onWindowResize) window.removeEventListener("resize", onWindowResize);
    hideStage();
    viewer = null;
  });

  // 账号切换实时换肤。
  createEffect(() => {
    void session()?.account_id;
    if (ready()) void loadCurrentSkin();
  });

  function cyclePose(): void {
    const next = POSES[(POSES.indexOf(pose()) + 1) % POSES.length];
    setPose(next);
    applyPose(next);
  }

  function resetViewer(): void {
    const v = viewer ?? stage?.viewer;
    if (!v) return;
    // skinview3d 3.4.2：复位相机用 resetCameraPose()（无 resetRender）。
    v.resetCameraPose();
    setPose("walk");
    applyPose("walk");
  }

  async function changeSkin(): Promise<void> {
    if (isMicrosoft()) {
      await external.openUrl("https://www.minecraft.net/zh-hans/msaprofile/mygames/editskin");
      return;
    }
    // 离线玩家：选本地 png 直接在模型上预览。
    fileInput?.click();
  }

  function onPickSkin(files: FileList | null): void {
    const file = files?.[0];
    if (!file) return;
    if (!file.type.startsWith("image/")) {
      toast({ message: t("instanceDetail.settings.icon.notImage"), tone: "error" });
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      setCustomSkin(String(reader.result));
      void loadCurrentSkin();
    };
    reader.readAsDataURL(file);
    if (fileInput) fileInput.value = "";
  }

  return (
    <div class="skin-panel">
      {/* 占位元素：决定舞台层的几何位置（自身不承载 canvas） */}
      <div class="skin-canvas-host" ref={slot}>
        <Show when={!ready()}>
          <Show when={failed()} fallback={<div class="hero-empty">{t("common.loading")}</div>}>
            {(err) => (
              <div class="skin-error">
                <div class="hero-empty">{t("skin.error.title")}</div>
                <div class="muted-line">{err()}</div>
                <RmButton size="sm" onClick={() => { setFailed(null); scheduleAttach(); }}>
                  {t("common.retry")}
                </RmButton>
              </div>
            )}
          </Show>
        </Show>
      </div>
      <input
        ref={fileInput}
        type="file"
        accept="image/png"
        class="visually-hidden"
        onChange={(e) => onPickSkin(e.currentTarget.files)}
      />
      <div class="skin-controls">
        <RmButton size="sm" icon="refresh" onClick={() => cyclePose()}>
          {t(`skin.pose.${pose()}`)}
        </RmButton>
        <RmButton size="sm" icon="check" onClick={() => resetViewer()}>
          {t("skin.reset")}
        </RmButton>
        <RmButton size="sm" variant="primary" icon="person" onClick={() => void changeSkin()}>
          {t("skin.change")}
        </RmButton>
        <Show when={customSkin()}>
          <RmIconButton
            icon="x"
            label={t("skin.clearCustom")}
            onClick={() => {
              setCustomSkin(null);
              void loadCurrentSkin();
            }}
          />
        </Show>
      </div>
      <div class="muted-line skin-note">{t("skin.dragHint")}</div>
    </div>
  );
}
