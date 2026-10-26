/**
 * PageHeader: breadcrumb + title on the left, up to 3 page-level actions on
 * the right (ui-design 00 §3.1).
 */
import type { JSX } from "solid-js";
import { Show, For } from "solid-js";

export interface Crumb {
  label: string;
  href?: string;
}

export function PageHeader(props: {
  crumbs?: Crumb[];
  title: string;
  actions?: JSX.Element;
}): JSX.Element {
  return (
    <header class="page-header">
      <div class="page-header-crumbs">
        <For each={props.crumbs ?? []}>
          {(crumb, index) => (
            <>
              <Show when={index() > 0}>
                <span class="page-crumb-sep">/</span>
              </Show>
              {crumb.href ? (
                <a class="page-crumb-link" href={crumb.href}>
                  {crumb.label}
                </a>
              ) : (
                <span class="page-crumb">{crumb.label}</span>
              )}
            </>
          )}
        </For>
        <h1 class="page-title">{props.title}</h1>
      </div>
      <div class="page-header-actions">{props.actions}</div>
    </header>
  );
}
