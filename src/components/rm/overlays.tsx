/**
 * Rm overlays: dialog / confirm dialog / dropdown menu.
 *
 * The confirm host lives in the ui store (`stores/ui.ts`) so any module can
 * `confirmDanger()` without prop drilling (ui-design 10 §5).
 */
import type { JSX } from "solid-js";
import { Show, For, onMount, onCleanup, createSignal, createEffect } from "solid-js";
import { Portal } from "solid-js/web";
import { Icon, type IconName } from "../icons";
import { RmButton, RmInput } from "./controls";
import { t } from "../../i18n";
import { uiStore } from "../../stores/ui";

// ── Dialog ──

export function RmDialog(props: {
  open: boolean;
  title: string;
  width?: number;
  onClose: () => void;
  children: JSX.Element;
  footer?: JSX.Element;
  alertRole?: boolean;
}): JSX.Element {
  // ESC closes (last-layer rule is handled by mount order: later dialogs
  // stopPropagation on their own keydown, so only the topmost sees it first).
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && props.open) {
      e.stopPropagation();
      props.onClose();
    }
  };
  onMount(() => document.addEventListener("keydown", onKeyDown));
  onCleanup(() => document.removeEventListener("keydown", onKeyDown));

  return (
    <Show when={props.open}>
      <div
        class="rm-overlay"
        onClick={(e) => {
          if (e.target === e.currentTarget) props.onClose();
        }}
      >
        <div
          class="rm-dialog"
          role={props.alertRole ? "alertdialog" : "dialog"}
          aria-modal="true"
          style={{ "--dialog-width": `${props.width ?? 460}px` }}
        >
          <header class="rm-dialog-head">
            <h3>{props.title}</h3>
            <button class="rm-icon-btn" aria-label={t("common.close")} onClick={() => props.onClose()}>
              <Icon name="x" size={16} />
            </button>
          </header>
          <div class="rm-dialog-body">{props.children}</div>
          <Show when={props.footer}>
            <footer class="rm-dialog-footer">{props.footer}</footer>
          </Show>
        </div>
      </div>
    </Show>
  );
}

// ── Confirm dialog ──

export interface ConfirmRequest {
  title: string;
  body: string;
  confirmLabel: string;
  danger?: boolean;
  /** Type-to-confirm the exact string (deletion of named things). */
  typeToConfirm?: string;
  resolve: (allowed: boolean) => void;
}

export function RmConfirmHost(): JSX.Element {
  const [typed, setTyped] = createSignal("");
  const request = () => uiStore.confirmRequest();
  const settle = (allowed: boolean) => {
    uiStore.resolveConfirm(allowed);
    setTyped("");
  };
  return (
    <Show when={request()} keyed>
      {(req) => (
        <RmDialog
          open
          title={req.title}
          width={440}
          alertRole
          onClose={() => settle(false)}
          footer={
            <>
              <RmButton variant="ghost" onClick={() => settle(false)}>
                {t("common.cancel")}
              </RmButton>
              <RmButton
                variant={req.danger ? "danger" : "primary"}
                disabled={req.typeToConfirm !== undefined && typed() !== req.typeToConfirm}
                onClick={() => settle(true)}
              >
                {req.confirmLabel}
              </RmButton>
            </>
          }
        >
          <div class="rm-confirm-body">{req.body}</div>
          <Show when={req.typeToConfirm !== undefined}>
            <input
              class="rm-input"
              placeholder={req.typeToConfirm}
              value={typed()}
              onInput={(e) => setTyped(e.currentTarget.value)}
            />
          </Show>
        </RmDialog>
      )}
    </Show>
  );
}

/** Text-input dialog host (replaces window.prompt — unsupported in WKWebView). */
export function RmPromptHost(): JSX.Element {
  const [value, setValue] = createSignal("");
  const request = () => uiStore.promptRequest();
  // Prefill the input with `initial` for each new request (P1-5): rename
  // flows edit the current value instead of retyping it from scratch.
  createEffect(() => {
    const req = request();
    if (req) setValue(req.initial ?? "");
  });
  const settle = (result: string | null) => {
    uiStore.resolvePrompt(result);
    setValue("");
  };
  return (
    <Show when={request()} keyed>
      {(req) => (
        <RmDialog
          open
          title={req.title}
          width={420}
          onClose={() => settle(null)}
          footer={
            <>
              <RmButton variant="ghost" onClick={() => settle(null)}>
                {t("common.cancel")}
              </RmButton>
              <RmButton variant="primary" onClick={() => settle(value().trim() || null)}>
                {t("common.confirm")}
              </RmButton>
            </>
          }
        >
          <div class="rm-field">
            <div class="rm-field-label">{req.label}</div>
            <RmInput
              value={value()}
              onInput={setValue}
              placeholder={req.placeholder ?? req.initial ?? ""}
              onEnter={() => settle(value().trim() || null)}
            >
              {null}
            </RmInput>
          </div>
        </RmDialog>
      )}
    </Show>
  );
}

// ── Dropdown menu ──

export interface MenuItem {
  label: string;
  icon?: IconName;
  danger?: boolean;
  disabled?: boolean;
  onSelect?: () => void;
}

export function RmMenu(props: {
  items: MenuItem[];
  children: JSX.Element;
  align?: "left" | "right";
  /** `below` (default) | `right`（侧栏/工具栏向右弹出）| `up`（向上弹出，不遮住按钮）。 */
  side?: "below" | "right" | "up";
  /**
   * Render the popup in a body portal with fixed positioning. Needed inside
   * `overflow` containers (the instance sidebar clips a normally-positioned
   * menu). The menu is right-aligned to the trigger and bottom-aligned to it.
   */
  portal?: boolean;
}): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [anchor, setAnchor] = createSignal<{ left: number; bottom: number } | null>(null);
  let root: HTMLDivElement | undefined;
  let menu: HTMLDivElement | undefined;
  const close = () => setOpen(false);
  const toggle = () => {
    if (!open() && props.portal && root) {
      const rect = root.getBoundingClientRect();
      setAnchor({
        left: rect.right + 6,
        bottom: Math.max(8, window.innerHeight - rect.bottom),
      });
    }
    setOpen(!open());
  };
  const onDocClick = (e: MouseEvent) => {
    const target = e.target as Node;
    if (open() && root && !root.contains(target) && !menu?.contains(target)) close();
  };
  onMount(() => document.addEventListener("mousedown", onDocClick));
  onCleanup(() => document.removeEventListener("mousedown", onDocClick));

  const menuNode = () => (
    <div
      ref={menu}
      class={`rm-menu rm-menu-${props.align ?? "left"}${
        props.side && props.side !== "below" ? ` rm-menu-pop-${props.side}` : ""
      }${props.portal ? " rm-menu-portal" : ""}`}
      style={
        props.portal && anchor()
          ? {
              position: "fixed",
              left: `${anchor()!.left}px`,
              bottom: `${anchor()!.bottom}px`,
              top: "auto",
              right: "auto",
            }
          : undefined
      }
      role="menu"
    >
      <For each={props.items}>
        {(item) => (
          <button
            class={`rm-menu-item${item.danger ? " is-danger" : ""}`}
            disabled={item.disabled}
            onClick={() => {
              close();
              item.onSelect?.();
            }}
          >
            <Show when={item.icon}>
              <Icon name={item.icon!} size={15} />
            </Show>
            {item.label}
          </button>
        )}
      </For>
    </div>
  );

  return (
    <div class="rm-menu-root" ref={root}>
      <div
        onClick={toggle}
        onKeyDown={(e) => e.key === "Enter" && toggle()}
        role="button"
        tabindex={0}
      >
        {props.children}
      </div>
      <Show when={open()}>
        {props.portal ? <Portal>{menuNode()}</Portal> : menuNode()}
      </Show>
    </div>
  );
}
