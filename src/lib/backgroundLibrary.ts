/**
 * Centralized `Background/` catalog + persisted wallpaper policy.
 *
 * The backend owns the folder, validates file names and stores the policy in
 * the single `settings.json`. This module keeps the live wallpaper stores in
 * sync, resolves carousel choices and schedules timed rotation.
 */
import { createRoot, createSignal } from "solid-js";
import { backgroundLibrary } from "../ipc/api";
import type {
  BackgroundCatalogView,
  BackgroundFileView,
  BackgroundImageMode,
  BackgroundSettings,
} from "../ipc/types";
import { applyBackgroundImage } from "./backgroundImage";
import { applyBackgroundVideo } from "./backgroundVideo";

const EMPTY_CATALOG: BackgroundCatalogView = { dir: "", images: [], videos: [] };
const DEFAULT_SETTINGS: BackgroundSettings = {
  image: { file: "", enabled: false, dim: 20 },
  video: { file: "", enabled: false, dim: 20 },
  image_mode: "single",
  carousel_interval_minutes: 30,
  change_on_startup: false,
};

const [settings, setSettingsSignal] = createRoot(() => createSignal<BackgroundSettings>(DEFAULT_SETTINGS));
const [catalog, setCatalogSignal] = createRoot(() => createSignal<BackgroundCatalogView>(EMPTY_CATALOG));
const [ready, setReadySignal] = createRoot(() => createSignal(false));
const [saving, setSavingSignal] = createRoot(() => createSignal(false));

let initialized = false;
let timer: number | undefined;
let liveImage = "";

export function backgroundLibrarySettings(): BackgroundSettings {
  return settings();
}

export function backgroundCatalog(): BackgroundCatalogView {
  return catalog();
}

export function backgroundLibraryReady(): boolean {
  return ready();
}

export function backgroundLibrarySaving(): boolean {
  return saving();
}

function randomFile(files: BackgroundFileView[], exclude?: string): string {
  if (files.length === 0) return "";
  if (files.length === 1) return files[0].name;
  const pool = exclude ? files.filter((file) => file.name !== exclude) : files;
  return pool[Math.floor(Math.random() * pool.length)]?.name ?? files[0].name;
}

function imageFile(
  next: BackgroundSettings,
  startup: boolean,
  current?: string
): BackgroundFileView | undefined {
  const files = catalog().images;
  if (!next.image.enabled || files.length === 0) return undefined;
  if (next.image_mode === "carousel") {
    if (startup && next.change_on_startup) {
      const name = randomFile(files, current);
      return files.find((file) => file.name === name) ?? files[0];
    }
    return files.find((file) => file.name === next.image.file) ?? files[0];
  }
  return files.find((file) => file.name === next.image.file);
}

function apply(snapshot: BackgroundSettings, startup = false, currentImage?: string): void {
  const image = imageFile(snapshot, startup, currentImage);
  liveImage = image?.name ?? "";
  applyBackgroundImage(
    image?.path ?? "",
    snapshot.image.enabled && Boolean(image),
    snapshot.image.dim
  );
  const video = catalog().videos.find((file) => file.name === snapshot.video.file);
  applyBackgroundVideo(
    video?.path ?? "",
    snapshot.video.enabled && Boolean(video),
    snapshot.video.dim
  );
  restartTimer();
}

function clearTimer(): void {
  if (timer !== undefined) {
    window.clearInterval(timer);
    timer = undefined;
  }
}

function advanceCarousel(): void {
  const snapshot = settings();
  if (!snapshot.image.enabled || snapshot.image_mode !== "carousel") return;
  const next = randomFile(catalog().images, liveImage);
  const file = catalog().images.find((candidate) => candidate.name === next);
  if (!file) return;
  liveImage = file.name;
  // The policy keeps the selected file; only the live carousel frame changes.
  applyBackgroundImage(file.path, true, snapshot.image.dim);
}

function restartTimer(): void {
  clearTimer();
  const snapshot = settings();
  if (!snapshot.image.enabled || snapshot.image_mode !== "carousel") return;
  const minutes = Math.max(1, snapshot.carousel_interval_minutes);
  timer = window.setInterval(advanceCarousel, minutes * 60_000);
}

async function load(): Promise<void> {
  const [settingsResult, catalogResult] = await Promise.all([
    backgroundLibrary.getSettings(),
    backgroundLibrary.catalog(),
  ]);
  if (catalogResult.ok) setCatalogSignal(catalogResult.data);
  if (settingsResult.ok) {
    setSettingsSignal(settingsResult.data);
    apply(settingsResult.data, true);
  }
  setReadySignal(true);
}

export async function initBackgroundLibrary(): Promise<void> {
  if (initialized) return;
  initialized = true;
  await load();
}

export async function refreshBackgroundCatalog(): Promise<void> {
  const result = await backgroundLibrary.catalog();
  if (!result.ok) return;
  setCatalogSignal(result.data);
  if (ready()) apply(settings());
}

export async function openBackgroundFolder(): Promise<{ ok: boolean; message?: string }> {
  const result = await backgroundLibrary.openFolder();
  return result.ok ? { ok: true } : { ok: false, message: result.error.message };
}

async function save(next: BackgroundSettings): Promise<{ ok: boolean; message?: string }> {
  setSavingSignal(true);
  const result = await backgroundLibrary.setSettings(next);
  setSavingSignal(false);
  if (!result.ok) return { ok: false, message: result.error.message };
  setSettingsSignal(result.data);
  apply(result.data);
  return { ok: true };
}

function mergeImage(patch: Partial<BackgroundSettings["image"]>): BackgroundSettings {
  const current = settings();
  return { ...current, image: { ...current.image, ...patch } };
}

function mergeVideo(patch: Partial<BackgroundSettings["video"]>): BackgroundSettings {
  const current = settings();
  return { ...current, video: { ...current.video, ...patch } };
}

export function setImageEnabled(enabled: boolean): Promise<{ ok: boolean; message?: string }> {
  const current = settings();
  const next = mergeImage({ enabled });
  if (enabled) next.video = { ...next.video, enabled: false };
  if (current.image_mode === "single" && enabled) next.image.file = current.image.file;
  return save(next);
}

export function setImageFile(file: string): Promise<{ ok: boolean; message?: string }> {
  const next = mergeImage({ file, enabled: Boolean(file) });
  if (file) next.video = { ...next.video, enabled: false };
  return save(next);
}

export function setImageDim(dim: number): Promise<{ ok: boolean; message?: string }> {
  return save(mergeImage({ dim: Math.round(Math.min(80, Math.max(0, dim))) }));
}

export function setImageMode(mode: BackgroundImageMode): Promise<{ ok: boolean; message?: string }> {
  const next = { ...settings(), image_mode: mode };
  return save(next);
}

export function setCarouselInterval(minutes: number): Promise<{ ok: boolean; message?: string }> {
  return save({
    ...settings(),
    carousel_interval_minutes: Math.round(Math.min(24 * 60, Math.max(1, minutes))),
  });
}

export function setChangeOnStartup(value: boolean): Promise<{ ok: boolean; message?: string }> {
  return save({ ...settings(), change_on_startup: value });
}

export function setVideoEnabled(enabled: boolean): Promise<{ ok: boolean; message?: string }> {
  const next = mergeVideo({ enabled });
  if (enabled) next.image = { ...next.image, enabled: false };
  return save(next);
}

export function setVideoFile(file: string): Promise<{ ok: boolean; message?: string }> {
  const next = mergeVideo({ file, enabled: Boolean(file) });
  if (file) next.image = { ...next.image, enabled: false };
  return save(next);
}

export function setVideoDim(dim: number): Promise<{ ok: boolean; message?: string }> {
  return save(mergeVideo({ dim: Math.round(Math.min(80, Math.max(0, dim))) }));
}
