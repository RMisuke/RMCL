/**
 * Rm control components (ui-design 01 §5).
 *
 * Every component consumes `--rmccc-*` tokens only; pages must not restyle
 * them with hardcoded colors. All interactive elements are keyboard-focusable
 * with a visible focus ring and labelled for a11y.
 */
import type { JSX } from "solid-js";
import { Show, createEffect } from "solid-js";
import { Icon, type IconName } from "../icons";

// ── Button ──

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

interface ButtonProps {
  variant?: ButtonVariant;
  size?: "sm" | "md" | "lg";
  icon?: IconName;
  loading?: boolean;
  disabled?: boolean;
  onClick?: (e: MouseEvent) => void;
  title?: string;
  children?: JSX.Element;
}

export function RmButton(props: ButtonProps): JSX.Element {
  return (
    <button
      class={`rm-btn rm-btn-${props.variant ?? "secondary"} rm-btn-${props.size ?? "md"}${props.loading ? " is-loading" : ""}`}
      disabled={props.disabled || props.loading}
      title={props.title}
      onClick={(e) => {
        if (!props.disabled && !props.loading) props.onClick?.(e);
      }}
    >
      <Show when={props.icon}>
        <Icon name={props.icon!} size={props.size === "sm" ? 14 : 16} />
      </Show>
      {props.children}
    </button>
  );
}

interface IconButtonProps {
  icon: IconName;
  label: string;
  title?: string;
  size?: number;
  onClick?: (e: MouseEvent) => void;
  disabled?: boolean;
  danger?: boolean;
  /** 常驻高亮态（如视图切换的当前视图）。 */
  active?: boolean;
  class?: string;
}

export function RmIconButton(props: IconButtonProps): JSX.Element {
  return (
    <button
      class={`rm-icon-btn${props.active ? " is-active" : ""}${props.danger ? " is-danger" : ""}${props.class ? ` ${props.class}` : ""}`}
      style={{ width: `${props.size ?? 30}px`, height: `${props.size ?? 30}px` }}
      aria-label={props.label}
      title={props.title ?? props.label}
      disabled={props.disabled}
      onClick={(e) => {
        if (!props.disabled) props.onClick?.(e);
      }}
    >
      <Icon name={props.icon} size={16} />
    </button>
  );
}

// ── Inputs ──

interface InputProps {
  value: string;
  onInput: (value: string) => void;
  placeholder?: string;
  type?: "text" | "password";
  disabled?: boolean;
  error?: string | null;
  mono?: boolean;
  onEnter?: () => void;
  children?: JSX.Element;
}

export function RmInput(props: InputProps): JSX.Element {
  return (
    <div class="rm-input-wrap">
      <input
        class={`rm-input${props.mono ? " rm-input-mono" : ""}`}
        type={props.type ?? "text"}
        value={props.value}
        placeholder={props.placeholder}
        disabled={props.disabled}
        onInput={(e) => props.onInput(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") props.onEnter?.();
        }}
      />
      {props.children}
      <Show when={props.error}>
        <div class="rm-field-error">{props.error}</div>
      </Show>
    </div>
  );
}

interface SelectProps {
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string; disabled?: boolean }[];
  disabled?: boolean;
  class?: string;
}

/** Native select wrapper (dropdown stays native — zero JS, themeable enough). */
export function RmSelect(props: SelectProps): JSX.Element {
  let el: HTMLSelectElement | undefined;
  // options 重建（如探测刷新模型列表）时原生 <select> 会丢选中项；
  // Solid 只在 value 变化时写 DOM，这里显式跟随 options 重设一次。
  createEffect(() => {
    // Track the option values, not only the count: dynamic lists (e.g. the
    // game-version filter rebuilt from the manifest) can replace their options
    // without changing length, leaving the native select on the placeholder.
    const values = props.options.map((o) => o.value).join("|");
    void values;
    const value = props.value;
    if (!el) return;
    if (el.value !== value) el.value = value;
    // The <option> children may commit after this effect during a context
    // change; re-assert once the DOM has settled.
    queueMicrotask(() => {
      if (el && el.value !== value) el.value = value;
    });
  });
  return (
    <select
      ref={el}
      class={`rm-select${props.class ? ` ${props.class}` : ""}`}
      value={props.value}
      disabled={props.disabled}
      onChange={(e) => props.onChange(e.currentTarget.value)}
    >
      {props.options.map((o) => (
        <option value={o.value} disabled={o.disabled}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function RmCheckbox(props: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
}): JSX.Element {
  return (
    <label class="rm-checkbox">
      <input
        type="checkbox"
        checked={props.checked}
        disabled={props.disabled}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
      />
      <span class="rm-checkbox-box">
        <Icon name="check" size={12} />
      </span>
      <Show when={props.label}>
        <span>{props.label}</span>
      </Show>
    </label>
  );
}

export function RmSwitch(props: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
}): JSX.Element {
  return (
    <button
      class={`rm-switch${props.checked ? " is-on" : ""}`}
      role="switch"
      aria-checked={props.checked}
      aria-label={props.label}
      disabled={props.disabled}
      onClick={() => props.onChange(!props.checked)}
    >
      <span class="rm-switch-knob" />
    </button>
  );
}

// ── Tags / progress ──

type TagTone = "neutral" | "accent" | "success" | "warning" | "danger";

// ── Slider / segmented choice ──

interface SliderProps {
  value: number;
  min: number;
  max: number;
  step?: number;
  onInput: (value: number) => void;
  disabled?: boolean;
  label: string;
}

/** Native range mapped onto the design tokens (memory allocation). */
export function RmSlider(props: SliderProps): JSX.Element {
  const percent = () => {
    const span = props.max - props.min;
    if (span <= 0) return 0;
    return Math.min(100, Math.max(0, ((props.value - props.min) / span) * 100));
  };
  return (
    <input
      class="rm-slider"
      type="range"
      min={props.min}
      max={props.max}
      step={props.step ?? 1}
      value={props.value}
      disabled={props.disabled}
      aria-label={props.label}
      style={{ "--rm-slider-fill": `${percent()}%` }}
      onInput={(e) => props.onInput(Number(e.currentTarget.value))}
    />
  );
}

interface SegmentedProps {
  value: string;
  options: { value: string; label: string; disabled?: boolean }[];
  onChange: (value: string) => void;
  disabled?: boolean;
  label: string;
  class?: string;
}

/**
 * Compact mutually-exclusive choice. Used where HMCL uses a radio list
 * (Java mode / window mode / quick-play target) but the option count is small.
 */
export function RmSegmented(props: SegmentedProps): JSX.Element {
  return (
    <div
      class={`rm-segmented${props.class ? ` ${props.class}` : ""}`}
      role="group"
      aria-label={props.label}
    >
      {props.options.map((option) => (
        <button
          type="button"
          class={`rm-segmented-item${props.value === option.value ? " is-active" : ""}`}
          aria-pressed={props.value === option.value}
          disabled={props.disabled || option.disabled}
          onClick={() => props.onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

// ── Tags / progress ──

export function RmTag(props: {
  tone?: TagTone;
  children: JSX.Element;
  title?: string;
}): JSX.Element {
  return (
    <span class={`rm-tag rm-tag-${props.tone ?? "neutral"}`} title={props.title}>
      {props.children}
    </span>
  );
}

export function RmProgress(props: {
  value: number;
  determinate?: boolean;
}): JSX.Element {
  return (
    <div
      class={`rm-progress${props.determinate === false ? " is-indeterminate" : ""}`}
      role="progressbar"
      aria-valuenow={props.determinate === false ? undefined : Math.round(props.value)}
    >
      <div
        class="rm-progress-fill"
        style={{ width: `${Math.min(100, Math.max(0, props.value))}%` }}
      />
    </div>
  );
}
