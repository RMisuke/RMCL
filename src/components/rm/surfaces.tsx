/**
 * Rm structural components: cards, list rows, side tabs, three-state content,
 * advanced disclosure (ui-design 01 §5, 01 §11).
 */
import type { JSX } from "solid-js";
import { Show, For, createSignal, Switch, Match } from "solid-js";
import { Icon, type IconName } from "../icons";
import { t } from "../../i18n";

// ── Card ──

export function RmCard(props: {
  title?: JSX.Element;
  actions?: JSX.Element;
  danger?: boolean;
  children: JSX.Element;
}): JSX.Element {
  return (
    <section class={`rm-card${props.danger ? " rm-card-danger" : ""}`}>
      <Show when={props.title || props.actions}>
        <header class="rm-card-head">
          <h3 class="rm-card-title">{props.title}</h3>
          <div class="rm-card-actions">{props.actions}</div>
        </header>
      </Show>
      <div class="rm-card-body">{props.children}</div>
    </section>
  );
}

// ── List row ──

export function RmListRow(props: {
  icon?: JSX.Element;
  title: JSX.Element;
  subtitle?: JSX.Element;
  trailing?: JSX.Element;
  onClick?: () => void;
  onDblClick?: () => void;
  selected?: boolean;
  danger?: boolean;
}): JSX.Element {
  return (
    <div
      class={`rm-row${props.selected ? " is-selected" : ""}${props.danger ? " is-danger" : ""}${props.onClick ? " is-clickable" : ""}`}
      onClick={() => props.onClick?.()}
      onDblClick={() => props.onDblClick?.()}
    >
      <Show when={props.icon}>
        <div class="rm-row-icon">{props.icon}</div>
      </Show>
      <div class="rm-row-main">
        <div class="rm-row-title">{props.title}</div>
        <Show when={props.subtitle}>
          <div class="rm-row-subtitle">{props.subtitle}</div>
        </Show>
      </div>
      <Show when={props.trailing}>
        <div class="rm-row-trailing">{props.trailing}</div>
      </Show>
    </div>
  );
}

// ── Side tabs (page-internal sub navigation, hash-based) ──

export interface SideTabItem {
  id: string;
  icon?: IconName;
  label: string;
}

export function RmSideTabs(props: {
  items: SideTabItem[];
  active: string;
  onSelect: (id: string) => void;
  footer?: JSX.Element;
}): JSX.Element {
  return (
    <nav class="rm-sidetabs">
      <For each={props.items}>
        {(item) => (
          <button
            class={`rm-sidetab${props.active === item.id ? " is-active" : ""}`}
            onClick={() => props.onSelect(item.id)}
          >
            <Show when={item.icon}>
              <Icon name={item.icon!} size={16} />
            </Show>
            <span>{item.label}</span>
          </button>
        )}
      </For>
      <div class="rm-sidetabs-footer">{props.footer}</div>
    </nav>
  );
}

// ── Top tabs (underline style) ──

export function RmTabs(props: {
  items: SideTabItem[];
  active: string;
  onSelect: (id: string) => void;
}): JSX.Element {
  return (
    <div class="rm-tabs" role="tablist">
      <For each={props.items}>
        {(item) => (
          <button
            role="tab"
            aria-selected={props.active === item.id}
            class={`rm-tab${props.active === item.id ? " is-active" : ""}`}
            onClick={() => props.onSelect(item.id)}
          >
            {item.label}
          </button>
        )}
      </For>
    </div>
  );
}

// ── Three-state content ──

export function RmSkeleton(props: { rows?: number; cards?: boolean }): JSX.Element {
  return (
    <div class={props.cards ? "rm-skeleton-cards" : "rm-skeleton-rows"} aria-hidden="true">
      <For each={Array.from({ length: props.cards ? 3 : (props.rows ?? 5) })}>
        {() => <div class={props.cards ? "rm-skeleton-card" : "rm-skeleton-row"} />}
      </For>
    </div>
  );
}

export function RmEmptyState(props: {
  icon?: IconName;
  title: string;
  hint?: string;
  action?: JSX.Element;
}): JSX.Element {
  return (
    <div class="rm-empty">
      <Icon name={props.icon ?? "package"} size={44} class="rm-empty-icon" />
      <div class="rm-empty-title">{props.title}</div>
      <Show when={props.hint}>
        <div class="rm-empty-hint">{props.hint}</div>
      </Show>
      <Show when={props.action}>
        <div class="rm-empty-action">{props.action}</div>
      </Show>
    </div>
  );
}

export function RmErrorState(props: {
  message: string;
  onRetry?: () => void;
  retryLabel?: string;
}): JSX.Element {
  return (
    <div class="rm-error" onClick={() => props.onRetry?.()}>
      <Icon name="alert" size={28} class="rm-error-icon" />
      <div class="rm-error-message">{props.message}</div>
      <Show when={props.onRetry}>
        <button class="rm-btn rm-btn-secondary rm-btn-sm">{props.retryLabel ?? t("common.retry")}</button>
      </Show>
    </div>
  );
}

/** The single container every async data area must go through (ui-design 10 §6). */
export function RmContentState<T>(props: {
  data: T | undefined;
  error?: string | null;
  loading: boolean;
  isEmpty?: (data: T) => boolean;
  skeleton?: JSX.Element;
  empty?: JSX.Element;
  children: (data: T) => JSX.Element;
}): JSX.Element {
  return (
    <Switch
      fallback={<Show when={props.data !== undefined}>{props.children(props.data as T)}</Show>}
    >
      <Match when={props.loading}>
        <Show when={props.skeleton} fallback={<RmSkeleton />}>
          {props.skeleton}
        </Show>
      </Match>
      <Match when={props.error}>
        <RmErrorState message={props.error ?? ""} />
      </Match>
      <Match when={props.data !== undefined && props.isEmpty?.(props.data as T)}>
        <Show when={props.empty} fallback={<RmEmptyState title="" />}>
          {props.empty}
        </Show>
      </Match>
    </Switch>
  );
}

// ── Advanced disclosure (level-3 progressive disclosure) ──

export function RmAdvancedSection(props: {
  title: string;
  summary?: string;
  children: JSX.Element;
}): JSX.Element {
  const [open, setOpen] = createSignal(false);
  return (
    <div class="rm-advanced">
      <button class="rm-advanced-head" onClick={() => setOpen(!open())}>
        <Icon name={open() ? "chevron-down" : "chevron-right"} size={14} />
        <span class="rm-advanced-title">{props.title}</span>
        <Show when={props.summary && !open()}>
          <span class="rm-advanced-summary">{props.summary}</span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="rm-advanced-body">{props.children}</div>
      </Show>
    </div>
  );
}

/** Field-level inherit/override lock (ui-design 04 §6.1). */
export function RmInheritLock(props: {
  overridden: boolean;
  onToggle: () => void;
  disabled?: boolean;
}): JSX.Element {
  return (
    <button
      class={`rm-inherit-lock${props.overridden ? " is-overridden" : ""}`}
      aria-label={props.overridden ? t("instanceDetail.settings.lockOverride") : t("instanceDetail.settings.lockInherit")}
      title={props.overridden ? t("instanceDetail.settings.lockOverride") : t("instanceDetail.settings.lockInherit")}
      disabled={props.disabled}
      onClick={() => props.onToggle()}
    >
      <Icon name={props.overridden ? "unlock" : "lock"} size={14} />
    </button>
  );
}

/** Section header inside a settings-like form. */
export function RmFieldRow(props: {
  label: string;
  hint?: JSX.Element;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div class="rm-field">
      <div class="rm-field-label">{props.label}</div>
      <div class="rm-field-control">{props.children}</div>
      <Show when={props.hint}>
        <div class="rm-field-hint">{props.hint}</div>
      </Show>
    </div>
  );
}
