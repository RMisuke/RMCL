/**
 * Structural pieces for the instance settings page.
 *
 * The layout intentionally follows HMCL's GameSettingsPage: a titled settings
 * section contains one compact card whose rows align a label on the left with
 * its editor on the right. Unlike the old form, inherited rows are never
 * rendered as dead grey controls — editing one freezes the whole card group
 * into per-instance overrides (the lock in the card header undoes that).
 */
import type { JSX } from "solid-js";
import { createEffect, createSignal, Show } from "solid-js";
import { Icon, type IconName } from "../icons";
import { RmInheritLock } from "../rm/surfaces";
import { formatBytes } from "../../lib/format";
import { t } from "../../i18n";

export function SettingsSection(props: {
  icon: IconName;
  title: string;
  hint?: string;
  children: JSX.Element;
}): JSX.Element {
  return (
    <section class="is-section">
      <header class="is-section-head">
        <span class="is-section-icon" aria-hidden="true">
          <Icon name={props.icon} size={15} />
        </span>
        <div class="is-section-heading">
          <h3 class="is-section-title">{props.title}</h3>
          <Show when={props.hint}>
            <p class="is-section-hint">{props.hint}</p>
          </Show>
        </div>
      </header>
      <div class="is-section-body">{props.children}</div>
    </section>
  );
}

export function SettingsCard(props: {
  title: string;
  hint?: string;
  /** Present when the whole card is one inherit/override group. */
  group?: {
    overridden: boolean;
    onToggle: () => void;
    label: string;
    disabled?: boolean;
  };
  actions?: JSX.Element;
  class?: string;
  children: JSX.Element;
}): JSX.Element {
  return (
    <section class={`is-card${props.class ? ` ${props.class}` : ""}`}>
      <header class="is-card-head">
        <div class="is-card-heading">
          <h4 class="is-card-title">{props.title}</h4>
          <Show when={props.hint}>
            <p class="is-card-hint">{props.hint}</p>
          </Show>
        </div>
        <div class="is-card-head-actions">
          {props.actions}
          <Show when={props.group}>
            {(group) => (
              <RmInheritLock
                overridden={group().overridden}
                onToggle={group().onToggle}
                label={group().label}
                disabled={group().disabled}
              />
            )}
          </Show>
        </div>
      </header>
      <div class="is-card-body">{props.children}</div>
    </section>
  );
}

export function SettingsRow(props: {
  title: JSX.Element;
  hint?: JSX.Element;
  /** Full-width editor below the label (textareas, path fields). */
  stacked?: boolean;
  class?: string;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div class={`is-row${props.stacked ? " is-stacked" : ""}${props.class ? ` ${props.class}` : ""}`}>
      <div class="is-row-label">
        <div class="is-row-title">{props.title}</div>
        <Show when={props.hint}>
          <div class="is-row-hint">{props.hint}</div>
        </Show>
      </div>
      <div class="is-row-control">{props.children}</div>
    </div>
  );
}

/**
 * Integer editor that keeps a local text buffer: clearing the field does not
 * immediately write `0`, and blur/Enter clamps to the configured grid.
 */
export function NumberField(props: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  ariaLabel: string;
  suffix?: string;
  disabled?: boolean;
}): JSX.Element {
  const [text, setText] = createSignal(String(props.value));
  let focused = false;

  createEffect(() => {
    const next = String(props.value);
    if (!focused) setText(next);
  });

  const clamp = (raw: number): number => {
    if (!Number.isFinite(raw)) return props.value;
    const step = props.step ?? 1;
    const snapped = Math.round(raw / step) * step;
    return Math.max(props.min, Math.min(props.max, snapped));
  };

  const commit = (): void => {
    const parsed = Number(text());
    const next = clamp(parsed);
    setText(String(next));
    if (next !== props.value) props.onChange(next);
  };

  return (
    <div class="is-number-wrap">
      <input
        class="rm-input is-number"
        type="number"
        inputmode="numeric"
        min={props.min}
        max={props.max}
        step={props.step ?? 1}
        value={text()}
        disabled={props.disabled}
        aria-label={props.ariaLabel}
        onFocus={() => {
          focused = true;
        }}
        onInput={(e) => {
          const raw = e.currentTarget.value;
          setText(raw);
          if (raw === "") return;
          const parsed = Number(raw);
          if (Number.isFinite(parsed) && parsed >= props.min && parsed <= props.max) {
            props.onChange(parsed);
          }
        }}
        onBlur={() => {
          focused = false;
          commit();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            commit();
            e.currentTarget.blur();
          }
        }}
      />
      <Show when={props.suffix}>
        <span class="is-number-suffix">{props.suffix}</span>
      </Show>
    </div>
  );
}

export function MemoryMeter(props: {
  totalBytes: number;
  availableBytes: number;
  allocatedMb: number;
}): JSX.Element {
  const total = () => Math.max(0, props.totalBytes);
  const used = () => Math.max(0, total() - Math.max(0, props.availableBytes));
  const allocated = () => Math.max(0, props.allocatedMb) * 1024 * 1024;
  const usedPercent = () => (total() > 0 ? Math.min(100, (used() / total()) * 100) : 0);
  const allocatedPercent = () =>
    total() > 0
      ? Math.min(100 - usedPercent(), (allocated() / total()) * 100)
      : 0;

  return (
    <div class="is-memory" role="group" aria-label={t("instanceDetail.settings.memory.meter")}>
      <div class="is-memory-track">
        <Show when={total() > 0}>
          <span class="is-memory-used" style={{ width: `${usedPercent()}%` }} />
          <span class="is-memory-allocated" style={{ width: `${allocatedPercent()}%` }} />
        </Show>
        <Show when={total() === 0}>
          <span class="is-memory-allocated is-unknown" style={{ width: "100%" }} />
        </Show>
      </div>
      <div class="is-memory-legend">
        <Show
          when={total() > 0}
          fallback={
            <span>
              {t("instanceDetail.settings.memory.allocated", {
                value: formatBytes(allocated()),
              })}
            </span>
          }
        >
          <span>
            {t("instanceDetail.settings.memory.systemUsed", {
              used: formatBytes(used()),
              total: formatBytes(total()),
            })}
          </span>
          <span class="is-memory-legend-alloc">
            {t("instanceDetail.settings.memory.allocated", {
              value: formatBytes(allocated()),
            })}
          </span>
        </Show>
      </div>
    </div>
  );
}
