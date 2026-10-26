/**
 * 删除实例对话框（唯一的删除入口组件）。
 *
 * 语义：删除 = 删除版本目录（versions/{id}，含版本 JSON / JAR / Natives）
 * 与实例配置；独立游戏目录（mods / 存档等）默认一并删除，可通过开关保留。
 * 后端对目录清理有双守卫：共享游戏根与不像游戏目录的路径一律拒删
 * （rmccc-profile delete 的 is_shared_game_root / looks_like_game_dir）。
 *
 * 三重防误触：删除键在左/取消在右（与全局确认框布局相反，打破肌肉记忆）、
 * 强制 5 秒冷静期（倒计时内禁用）、冷却结束后需**第二次点击**才执行。
 * 共享版本被其他实例引用时自动保留版本目录（ADR-002 决策 3 同思路）。
 */
import type { JSX } from "solid-js";
import { Show, createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { RmDialog } from "./rm/overlays";
import { RmButton, RmSwitch } from "./rm/controls";
import { profiles, instances } from "../ipc/api";
import type { Profile } from "../ipc/api";
import { profilesStore } from "../stores/profiles";
import { toast } from "../stores/ui";
import { t } from "../i18n";

export function DeleteInstanceDialog(props: {
  profile: Profile;
  open: boolean;
  onClose: () => void;
}): JSX.Element {
  const navigate = useNavigate();
  const [count, setCount] = createSignal(5);
  const [armed, setArmed] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  // 外置目录（HMCL）只移除实例记录，游戏文件属于用户既有目录。
  const isExternal = () => props.profile.external_root;
  // Also remove the isolated game dir (mods / saves); backend guards reject
  // shared roots and non-game-looking paths either way.
  const [deleteDir, setDeleteDir] = createSignal(!props.profile.external_root);

  // 每次打开重置冷静期与武装态（关闭后残留状态不得带入下一次）。
  createEffect(() => {
    if (props.open) {
      setCount(5);
      setArmed(false);
      setBusy(false);
      setDeleteDir(!props.profile.external_root);
    }
  });

  // 冷静期倒计时；到 0 后信号值不再变化，Solid 等值不触发更新（可安全续跑）。
  onMount(() => {
    const id = setInterval(() => setCount((c) => Math.max(0, c - 1)), 1000);
    onCleanup(() => clearInterval(id));
  });

  const execute = async () => {
    setBusy(true);
    // Order matters: remove the profile record first. If the later version-dir
    // step fails we keep a clean, retryable state instead of a deleted
    // directory with a dangling profile entry.
    const res = await profiles.delete(props.profile.id, isExternal() ? false : deleteDir());
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      setBusy(false);
      return;
    }
    profilesStore.removeItem(props.profile.id);
    // 共享版本保护：版本目录仍被其他实例引用时保留文件（ADR-002 决策 3 同思路）。
    const shared = (profilesStore.items() ?? []).some(
      (p) => p.id !== props.profile.id && p.game_version === props.profile.game_version
    );
    if (!shared && !isExternal()) {
      const removed = await instances.remove(props.profile.game_version);
      if (!removed.ok) {
        toast({
          message: t("instanceDetail.deleteAll.versionDirFailed", { message: removed.error.message }),
          tone: "error",
        });
        props.onClose();
        navigate("/instances");
        return;
      }
    }
    toast({ message: t("instances.deleted"), tone: "success" });
    props.onClose();
    navigate("/instances");
  };

  const label = () =>
    count() > 0
      ? t("instanceDetail.deleteAll.cooldown", { seconds: count() })
      : armed()
        ? t("instanceDetail.deleteAll.armed")
        : t("instances.delete.confirm");

  const onConfirm = () => {
    if (!armed()) {
      setArmed(true);
      return;
    }
    void execute();
  };

  return (
    <RmDialog
      open={props.open}
      title={t("instances.delete.title")}
      width={460}
      alertRole
      onClose={() => {
        if (!busy()) props.onClose();
      }}
      footer={
        <>
          <RmButton variant="danger" icon="trash" disabled={count() > 0 || busy()} onClick={onConfirm}>
            {label()}
          </RmButton>
          <RmButton variant="ghost" disabled={busy()} onClick={() => props.onClose()}>
            {t("common.cancel")}
          </RmButton>
        </>
      }
    >
      <div class="rm-confirm-body">
        {isExternal()
          ? t("instanceDetail.deleteAll.externalBody", { name: props.profile.name })
          : t("instanceDetail.deleteAll.body", { name: props.profile.name })}
      </div>
      <Show
        when={!isExternal()}
        fallback={<div class="rm-confirm-body muted-line">{t("instanceDetail.deleteAll.external")}</div>}
      >
        <div class="path-row">
          <span class="path-row-value">{t("instanceDetail.deleteAll.deleteDir")}</span>
          <RmSwitch checked={deleteDir()} onChange={setDeleteDir} label={t("instanceDetail.deleteAll.deleteDir")} />
        </div>
      </Show>
      <div class="rm-confirm-body muted-line">{t("instanceDetail.deleteAll.shared")}</div>
    </RmDialog>
  );
}
