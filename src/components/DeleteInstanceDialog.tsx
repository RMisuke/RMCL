/**
 * Delete-instance dialog (single deletion entry point).
 *
 * ADR-016: an instance *is* `versions/{id}`. `deleteFiles=false` hides the
 * instance (settings `hidden=true`) and keeps every game file; `deleteFiles=true`
 * removes the version directory and its `.rmccc` settings.
 */
import type { JSX } from "solid-js";
import { Show, createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { RmDialog } from "./rm/overlays";
import { RmButton, RmSwitch } from "./rm/controls";
import { instances } from "../ipc/api";

import { instancesStore } from "../stores/instances";
import { toast } from "../stores/ui";
import { t } from "../i18n";

export interface DeleteInstanceTarget {
  key: string;
  name: string;
  external: boolean;
}

export function DeleteInstanceDialog(props: {
  instance: DeleteInstanceTarget;
  open: boolean;
  onClose: () => void;
}): JSX.Element {
  const navigate = useNavigate();
  const [count, setCount] = createSignal(5);
  const [armed, setArmed] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const isExternal = () => props.instance.external;
  const [deleteFiles, setDeleteFiles] = createSignal(!isExternal());

  createEffect(() => {
    if (props.open) {
      setCount(5);
      setArmed(false);
      setBusy(false);
      setDeleteFiles(!isExternal());
    }
  });

  onMount(() => {
    const id = setInterval(() => setCount((c) => Math.max(0, c - 1)), 1000);
    onCleanup(() => clearInterval(id));
  });

  const execute = async () => {
    setBusy(true);
    const key = props.instance.key;
    const res = await instances.remove(key, deleteFiles());
    if (!res.ok) {
      toast({ message: res.error.message, tone: "error" });
      setBusy(false);
      return;
    }
    instancesStore.removeItem(key);
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
          ? t("instanceDetail.deleteAll.externalBody", { name: props.instance.name })
          : t("instanceDetail.deleteAll.body", { name: props.instance.name })}
      </div>
      <div class="path-row">
        <span class="path-row-value">{t("instanceDetail.deleteAll.deleteDir")}</span>
        <RmSwitch
          checked={deleteFiles()}
          onChange={setDeleteFiles}
          label={t("instanceDetail.deleteAll.deleteDir")}
        />
      </div>
      <Show
        when={!deleteFiles()}
        fallback={<div class="rm-confirm-body muted-line">{t("instanceDetail.deleteAll.shared")}</div>}
      >
        <div class="rm-confirm-body muted-line">{t("instanceDetail.deleteAll.external")}</div>
      </Show>
    </RmDialog>
  );
}
