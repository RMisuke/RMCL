/**
 * 零依赖虚拟滚动列表（固定行高窗口化）。
 *
 * 只渲染视口内 + 上下 overscan 的行，DOM 数量恒定——版本列表 900+ 条
 * 全量可滚而不掉帧。滚动事件经 rAF 节流；行高必须与 itemHeight 一致
 * （由 CSS 固定），否则窗口定位会错位。
 */
import { createEffect, createSignal, For, onCleanup, onMount } from "solid-js";
import type { JSX } from "solid-js";

export function VirtualList<T>(props: {
  items: readonly T[];
  /** 固定行高（px），须与行元素的实际高度一致。 */
  itemHeight: number;
  /** 视口高度（px）；未传 height 时使用。 */
  viewportHeight?: number;
  /** "fill" = 不定高，由父级 flex 撑满（用 ResizeObserver 实测视口高度）。 */
  height?: "fill" | string;
  /** 视口上下各多渲染的行数，滚动时避免白边。 */
  overscan?: number;
  class?: string;
  renderItem: (item: T, index: number) => JSX.Element;
}): JSX.Element {
  const fallbackH = () => props.viewportHeight ?? 440;
  const overscan = () => props.overscan ?? 6;
  const [viewportPx, setViewportPx] = createSignal(fallbackH());
  const [scrollTop, setScrollTop] = createSignal(0);
  let el: HTMLDivElement | undefined;
  let raf = 0;
  let resizeObserver: ResizeObserver | null = null;

  onMount(() => {
    if (props.height !== "fill") return;
    resizeObserver = new ResizeObserver(() => {
      if (el && el.clientHeight > 0) setViewportPx(el.clientHeight);
    });
    if (el) resizeObserver.observe(el);
  });
  onCleanup(() => {
    resizeObserver?.disconnect();
    resizeObserver = null;
  });

  const onScroll = (e: Event) => {
    const top = (e.currentTarget as HTMLElement).scrollTop;
    if (!raf) {
      raf = requestAnimationFrame(() => {
        raf = 0;
        setScrollTop(top);
      });
    }
  };
  onCleanup(() => {
    if (raf) cancelAnimationFrame(raf);
  });

  // 数据集变化（搜索过滤/换 loader）时回到顶部，避免停在超出范围的旧位置。
  createEffect(() => {
    void props.items.length;
    if (el) {
      el.scrollTop = 0;
      setScrollTop(0);
    }
  });

  const start = () =>
    Math.max(0, Math.floor(scrollTop() / props.itemHeight) - overscan());
  const end = () =>
    Math.min(
      props.items.length,
      Math.ceil((scrollTop() + viewportPx()) / props.itemHeight) + overscan()
    );

  return (
    <div
      ref={el}
      class={`vlist${props.height === "fill" ? " vlist-fill" : ""}${props.class ? ` ${props.class}` : ""}`}
      style={{ height: props.height === "fill" ? undefined : (props.height ?? `${fallbackH()}px`) }}
      onScroll={onScroll}
    >
      <div
        class="vlist-spacer"
        style={{ height: `${props.items.length * props.itemHeight}px` }}
      >
        <div
          class="vlist-window"
          style={{ transform: `translateY(${start() * props.itemHeight}px)` }}
        >
          <For each={props.items.slice(start(), end())}>
            {(item, i) => props.renderItem(item, start() + i())}
          </For>
        </div>
      </div>
    </div>
  );
}
