/**
 * Modpack wizard state: inspect a local archive and schedule an install.
 *
 * Progress/completion are owned by the unified task store — this store only
 * holds the archive inspection and the "starting" flag so the dialog can stay
 * open until the backend has created the task.
 */
import { createRoot, createSignal } from "solid-js";
import { modpack } from "../ipc/api";
import type { ModpackInspectView } from "../ipc/types";
import { toast } from "./ui";
import { t } from "../i18n";

export const modpacksStore = createRoot(() => {
  const [inspection, setInspection] = createSignal<ModpackInspectView | null>(null);
  const [inspecting, setInspecting] = createSignal(false);
  const [starting, setStarting] = createSignal(false);
  const [inspectError, setInspectError] = createSignal<string | null>(null);

  async function inspectPath(path: string): Promise<boolean> {
    setInspecting(true);
    setInspectError(null);
    setInspection(null);
    const res = await modpack.inspect(path);
    setInspecting(false);
    if (res.ok) {
      setInspection(res.data);
      return true;
    }
    setInspectError(res.error.message);
    toast({ message: res.error.message, tone: "error" });
    return false;
  }

  async function pickAndInspect(): Promise<boolean> {
    const picked = await modpack.pickFile();
    if (!picked.ok) {
      toast({ message: picked.error.message, tone: "error" });
      return false;
    }
    if (!picked.data) return false;
    return inspectPath(picked.data);
  }

  function dismissInspection(): void {
    setInspection(null);
    setInspectError(null);
  }

  async function startInstall(directoryId: string, instanceName: string): Promise<boolean> {
    const current = inspection();
    if (!current) return false;
    setStarting(true);
    const res = await modpack.install(current.path, directoryId, instanceName.trim() || undefined);
    setStarting(false);
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      return false;
    }
    setInspection(null);
    toast({ message: t("modpack.started", { name: current.name }), tone: "info" });
    return true;
  }

  return {
    inspection,
    inspecting,
    starting,
    inspectError,
    inspectPath,
    pickAndInspect,
    dismissInspection,
    startInstall,
  };
});
