import { useLayoutEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";

import type { Strings } from "../i18n";

import "./AdjustableDataRegion.css";


const MIN_REGION_HEIGHT = 140;
const DEFAULT_REGION_HEIGHT = 320;
const MIN_COLUMN_WIDTH = 48;
const MAX_COLUMN_WIDTH = 720;
const COLUMN_STEP = 16;
export function clampColumnWidth(width: number, minWidth = MIN_COLUMN_WIDTH) {
  return Math.min(MAX_COLUMN_WIDTH, Math.max(minWidth, width));
}

function maxRegionHeight(region: HTMLElement | null) {
  const scrollport = region?.closest<HTMLElement>(".flow-scroll");
  const controlsHeight =
    region?.querySelector<HTMLElement>(".adjustable-data-region__controls")
      ?.getBoundingClientRect().height ?? 0;
  const available = (scrollport?.clientHeight ?? window.innerHeight) - controlsHeight;
  return Math.max(MIN_REGION_HEIGHT, Math.floor(available));
}

function clampRegionHeight(height: number, region: HTMLElement | null) {
  return Math.min(Math.max(height, MIN_REGION_HEIGHT), maxRegionHeight(region));
}

export function AdjustableDataRegion({
  children,
  s,
  className,
  label,
}: {
  children: ReactNode;
  s: Strings;
  className: string;
  label: string;
}) {
  const regionRef = useRef<HTMLElement | null>(null);
  const [height, setHeight] = useState(DEFAULT_REGION_HEIGHT);
  const [maxHeight, setMaxHeight] = useState(DEFAULT_REGION_HEIGHT);
  const dragStart = useRef<{ y: number; height: number } | null>(null);
  const handleRef = useRef<HTMLDivElement | null>(null);
  const keyboardResizePending = useRef(false);
  const fitPending = useRef(false);
  const [fitRequest, setFitRequest] = useState(0);

  useLayoutEffect(() => {
    const region = regionRef.current;
    if (!region) return;
    const scrollport = region.closest<HTMLElement>(".flow-scroll");
    let mounted = true;
    const updateBounds = () => {
      if (!mounted) return;
      const max = maxRegionHeight(region);
      setMaxHeight(max);
      setHeight((current) => Math.min(current, max));
    };
    const observer = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(updateBounds);
    if (scrollport) observer?.observe(scrollport);
    const controls = region.querySelector(".adjustable-data-region__controls");
    if (controls) observer?.observe(controls);
    window.addEventListener("resize", updateBounds);
    document.fonts?.ready.then(updateBounds);
    updateBounds();
    return () => {
      mounted = false;
      observer?.disconnect();
      window.removeEventListener("resize", updateBounds);
    };
  }, []);
  useLayoutEffect(() => {
    const region = regionRef.current;
    if (fitPending.current && region) {
      fitPending.current = false;
      const scrollport = region.closest<HTMLElement>(".flow-scroll");
      if (scrollport) {
        scrollport.scrollTop +=
          region.getBoundingClientRect().top -
          scrollport.getBoundingClientRect().top;
      }
    }
    if (keyboardResizePending.current) {
      keyboardResizePending.current = false;
      handleRef.current?.scrollIntoView?.({
        block: "nearest",
        inline: "nearest",
      });
    }
  }, [height, fitRequest]);

  const beginResize = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    dragStart.current = { y: event.clientY, height };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const updateResize = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragStart.current) return;
    setHeight(
      clampRegionHeight(
        dragStart.current.height + event.clientY - dragStart.current.y,
        regionRef.current,
      ),
    );
  };

  return (
    <section ref={regionRef} className="adjustable-data-region" aria-label={label}>
      <div
        className="adjustable-data-region__content"
        style={{ height: `${height}px`, maxHeight: `${maxHeight}px` }}
      >
        <div className={className}>{children}</div>
      </div>
      <div className="adjustable-data-region__controls">
        <button
          type="button"
          onClick={() =>
            setHeight(clampRegionHeight(DEFAULT_REGION_HEIGHT, regionRef.current))
          }
        >
          {s.t("layout.data.resetHeight")}
        </button>
        <button
          type="button"
          onClick={() => {
            fitPending.current = true;
            setHeight(maxRegionHeight(regionRef.current));
            setFitRequest((request) => request + 1);
          }}
        >
          {s.t("layout.data.fit")}
        </button>
        <div ref={handleRef}
          className="adjustable-data-region__handle"
          role="separator"
          tabIndex={0}
          aria-label={s.t("layout.data.resizeHeight", { label })}
          aria-orientation="horizontal"
          aria-valuemin={MIN_REGION_HEIGHT}
          aria-valuemax={maxHeight}
          aria-valuenow={Math.round(height)}
          onPointerDown={beginResize}
          onPointerMove={updateResize}
          onPointerUp={() => (dragStart.current = null)}
          onLostPointerCapture={() => (dragStart.current = null)}
          onPointerCancel={() => (dragStart.current = null)}
          onKeyDown={(event) => {
            if (event.key === "ArrowUp" || event.key === "ArrowDown") {
              event.preventDefault();
              const delta = event.key === "ArrowDown" ? COLUMN_STEP : -COLUMN_STEP;
              keyboardResizePending.current = true;
              setHeight((current) =>
                clampRegionHeight(current + delta, regionRef.current),
              );
            } else if (event.key === "Home") {
              event.preventDefault();
              keyboardResizePending.current = true;
              setHeight(clampRegionHeight(MIN_REGION_HEIGHT, regionRef.current));
            } else if (event.key === "End") {
              event.preventDefault();
              keyboardResizePending.current = true;
              setHeight(maxRegionHeight(regionRef.current));
            }
          }}
        />
      </div>
    </section>
  );
}

export function ColumnResizeHandle({
  width,
  label,
  s,
  onWidthChange,
  minWidth = MIN_COLUMN_WIDTH,
}: {
  width: number;
  label: string;
  s: Strings;
  onWidthChange: (width: number) => void;
  minWidth?: number;
}) {
  const dragStart = useRef<{ x: number; width: number } | null>(null);
  const updatePreview = (handle: HTMLDivElement, nextWidth: number) => {
    const next = Math.round(clampColumnWidth(nextWidth, minWidth));
    const header = handle.parentElement;
    const table = header?.closest("table");
    const cellIndex =
      header instanceof HTMLTableCellElement ? header.cellIndex : -1;
    const column = cellIndex >= 0 ? table?.querySelectorAll("col")[cellIndex] : null;
    const previousWidth = Number.parseFloat(column?.style.width || "") ||
      Number.parseFloat(header?.style.width || "");
    const currentTableWidth = Number.parseFloat(table?.style.width || "");
    if (table && Number.isFinite(currentTableWidth) && previousWidth > 0) {
      table.style.width = `${Math.max(0, currentTableWidth + next - previousWidth)}px`;
    }
    if (header) header.style.width = `${next}px`;
    if (column) column.style.width = `${next}px`;
    handle.setAttribute("aria-valuenow", String(next));
    handle.setAttribute(
      "aria-valuetext",
      s.t("layout.column.width", { width: next }),
    );
  };

  return (
    <div
      className="adjustable-column-handle"
      role="separator"
      tabIndex={0}
      aria-label={s.t("layout.column.resize", { column: label })}
      aria-orientation="vertical"
      aria-valuemin={minWidth}
      aria-valuemax={MAX_COLUMN_WIDTH}
      aria-valuenow={Math.round(width)}
      aria-valuetext={s.t("layout.column.width", { width: Math.round(width) })}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.stopPropagation();
        dragStart.current = { x: event.clientX, width };
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (!dragStart.current) return;
        updatePreview(
          event.currentTarget,
          dragStart.current.width + event.clientX - dragStart.current.x,
        );
      }}
      onPointerUp={(event) => {
        if (!dragStart.current) return;
        const next = Number(event.currentTarget.getAttribute("aria-valuenow"));
        dragStart.current = null;
        onWidthChange(next);
      }}
      onPointerCancel={(event) => {
        if (!dragStart.current) return;
        updatePreview(event.currentTarget, dragStart.current.width);
        dragStart.current = null;
      }}
      onLostPointerCapture={(event) => {
        if (!dragStart.current) return;
        updatePreview(event.currentTarget, dragStart.current.width);
        dragStart.current = null;
      }}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
          event.preventDefault();
          const delta = event.key === "ArrowRight" ? COLUMN_STEP : -COLUMN_STEP;
          onWidthChange(clampColumnWidth(width + delta, minWidth));
        } else if (event.key === "Home") {
          event.preventDefault();
          onWidthChange(minWidth);
        } else if (event.key === "End") {
          event.preventDefault();
          onWidthChange(MAX_COLUMN_WIDTH);
        }
      }}
    />
  );
}
