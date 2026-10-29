import type { JSX } from "solid-js";

/** Soft-voxel grass cube used by the home hero, instance rows and the launch dock. */
export function GrassBlock(props: { small?: boolean }): JSX.Element {
  return (
    <div class={`grass-block${props.small ? " is-small" : ""}`} aria-hidden="true">
      <div class="grass-block-top" />
      <div class="grass-block-left" />
      <div class="grass-block-right" />
      <i class="grass-pixel p1" /><i class="grass-pixel p2" /><i class="grass-pixel p3" />
    </div>
  );
}
