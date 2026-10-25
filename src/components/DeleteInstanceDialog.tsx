/**
 * 删除实例对话框（唯一的删除入口组件）。
 *
 * 语义：删除 = 删除**全部游戏资源**（版本目录 + 实例配置，独立游戏目录一并
 * 删除）——不存在"只删记录"的轻量删除。
 *
 * 三重防误触：删除键在左/取消在右（与全局确认框布局相反，打破肌肉记忆）、
 * 强制 5 秒冷静期（倒计时内禁用）、冷却结束后需**第二次点击**才执行。
 * 共享版本被其他实例引用时自动保留版本目录（ADR-002 决策 3 同思路）。
 */
import type { JSX } from "solid-js";
import { createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { RmDialog } from "./rm/overlays";
import { RmButton } from "./rm/controls";
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

  // 每次打开重置冷静期与武装态（关闭后残留状态不得带入下一次）。
  createEffect(() => {
    if (props.open) {
      setCount(5);
      setArmed(false);
      setBusy(false);
    }
  });

  // 冷静期倒计时；到 0 后信号值不再变化，Solid 等值不触发更新（可安全续跑）。
  onMount(() => {
    const id = setInterval(() => setCount((c) => Math.max(0, c - 1)), 1000);
    onCleanup(() => clearInterval(id));
  });

  const execute = async () => {
    setBusy(true);
    // 共享版本保护：版本目录仍被其他实例引用时保留文件（ADR-002 决策 3 同思路）。
    const shared = (profilesStore.items() ?? []).some(
      (p) => p.id !== props.profile.id && p.game_version === props.profile.game_version
    );
    if (!shared) {
      const removed = await instances.remove(props.profile.game_version);
      if (!removed.ok) {
        toast({ message: removed.error.message, tone: "error" });
        setBusy(false);
        return;
      }
    }
    const res = await profiles.delete(props.profile.id);
    if (res.ok) {
      profilesStore.removeItem(props.profile.id);
      toast({ message: t("instances.deleted"), tone: "success" });
      props.onClose();
      navigate("/instances");
    } else {
      toast({ message: res.error.message, tone: "error" });
      setBusy(false);
    }
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
      <div class="rm-confirm-body">{t("instanceDetail.deleteAll.body", { name: props.profile.name })}</div>
      <div class="rm-confirm-body muted-line">{t("instanceDetail.deleteAll.shared")}</div>
    </RmDialog>
  );
}
