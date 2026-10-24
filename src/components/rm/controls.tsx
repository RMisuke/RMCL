/**
 * Rm control components (ui-design 01 §5).
 *
 * Every component consumes `--rmccc-*` tokens only; pages must not restyle
 * them with hardcoded colors. All interactive elements are keyboard-focusable
 * with a visible focus ring and labelled for a11y.
 */
import type { JSX } from "solid-js";
import { Show } from "solid-js";
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
}

export function RmIconButton(props: IconButtonProps): JSX.Element {
  return (
    <button
      class={`rm-icon-btn${props.danger ? " is-danger" : ""}`}
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

interface TextareaProps {
  value: string;
  onInput: (value: string) => void;
  rows?: number;
  placeholder?: string;
  mono?: boolean;
}

export function RmTextarea(props: TextareaProps): JSX.Element {
  return (
    <textarea
      class={`rm-textarea${props.mono ? " rm-input-mono" : ""}`}
      rows={props.rows ?? 3}
      value={props.value}
      placeholder={props.placeholder}
      onInput={(e) => props.onInput(e.currentTarget.value)}
    />
  );
}

interface SelectProps {
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string; disabled?: boolean }[];
  disabled?: boolean;
}

/** Native select wrapper (dropdown stays native — zero JS, themeable enough). */
export function RmSelect(props: SelectProps): JSX.Element {
  return (
    <select
      class="rm-select"
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

// ── Tags / badges / progress ──

type TagTone = "neutral" | "accent" | "success" | "warning" | "danger";

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

export function RmBadge(props: {
  count?: number;
  tone?: "accent" | "danger";
}): JSX.Element {
  return (
    <span class={`rm-badge rm-badge-${props.tone ?? "accent"}`}>
      <Show when={props.count} fallback={<span class="rm-badge-dot" />}>
        {props.count}
      </Show>
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
