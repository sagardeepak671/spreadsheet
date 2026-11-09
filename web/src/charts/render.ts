/**
 * Minimal dependency-free SVG chart renderer for the `ChartData` the Rust
 * core produces: line, bar and scatter, with title, axis labels, ticks and a
 * legend. Returns an SVG *string* so it is trivially unit-testable and can be
 * dropped into `innerHTML`.
 */

import type { ChartData } from "../bridge";
import type { ChartTheme } from "../theme";

export interface RenderOptions {
  width?: number;
  height?: number;
  /** Colours from the active UI theme (defaults to the light palette). */
  theme?: Partial<ChartTheme>;
}

export const SERIES_COLORS = ["#1a73e8", "#e8710a", "#188038", "#d93025", "#8430ce", "#007b83", "#b06000", "#5f6368"];

export const DEFAULT_CHART_THEME: ChartTheme = {
  background: "#ffffff",
  text: "#1f2328",
  muted: "#6b7280",
  grid: "#e5e7eb",
  axis: "#9aa0a6",
  series: SERIES_COLORS,
};

export interface Scale {
  min: number;
  max: number;
  ticks: number[];
}

/** "Nice" axis scale covering [lo, hi] with ~`count` ticks. */
export function niceScale(lo: number, hi: number, count = 5): Scale {
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) return { min: 0, max: 1, ticks: [0, 0.5, 1] };
  if (lo === hi) {
    lo = lo === 0 ? -1 : lo - Math.abs(lo) * 0.5;
    hi = hi === 0 ? 1 : hi + Math.abs(hi) * 0.5;
  }
  const span = hi - lo;
  const rawStep = span / Math.max(1, count);
  const magnitude = 10 ** Math.floor(Math.log10(rawStep));
  const residual = rawStep / magnitude;
  // Heckbert's "nice numbers": round the step to 1, 2, 5 or 10 x 10^k.
  const step = (residual < 1.5 ? 1 : residual < 3 ? 2 : residual < 7 ? 5 : 10) * magnitude;
  const min = Math.floor(lo / step) * step;
  const max = Math.ceil(hi / step) * step;
  const ticks: number[] = [];
  for (let v = min; v <= max + step / 2; v += step) ticks.push(Number(v.toFixed(10)));
  return { min, max, ticks };
}

export function formatTick(v: number): string {
  if (Math.abs(v) >= 1e6 || (Math.abs(v) < 1e-3 && v !== 0)) return v.toExponential(1);
  return Number.isInteger(v) ? String(v) : String(Number(v.toFixed(3)));
}

export function escapeXml(text: string): string {
  return text.replace(/[<>&"']/g, (c) => ({ "<": "&lt;", ">": "&gt;", "&": "&amp;", '"': "&quot;", "'": "&#39;" })[c] ?? c);
}

/** Renders `data` as an SVG document string. */
export function renderChartSvg(data: ChartData, options: RenderOptions = {}): string {
  const width = options.width ?? 520;
  const height = options.height ?? 320;
  const theme: ChartTheme = { ...DEFAULT_CHART_THEME, ...options.theme };
  if (!theme.series.length) theme.series = SERIES_COLORS;
  const margin = { top: data.title ? 38 : 18, right: 16, bottom: data.xLabel ? 58 : 42, left: data.yLabel ? 66 : 52 };
  const legendHeight = data.series.length > 1 ? 20 : 0;
  const plotW = Math.max(10, width - margin.left - margin.right);
  const plotH = Math.max(10, height - margin.top - margin.bottom - legendHeight);
  const parts: string[] = [];
  parts.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" class="ss-chart ss-chart-${data.kind}" role="img" aria-label="${escapeXml(data.title || "chart")}">`);
  parts.push(`<rect x="0" y="0" width="${width}" height="${height}" fill="${theme.background}"/>`);
  if (data.title) parts.push(`<text x="${width / 2}" y="22" text-anchor="middle" font-size="15" font-weight="600" fill="${theme.text}">${escapeXml(data.title)}</text>`);

  const pointCount = Math.max(0, ...data.series.map((s) => s.values.length));
  const hasData = data.series.length > 0 && pointCount > 0;
  if (!hasData) {
    parts.push(`<text x="${width / 2}" y="${height / 2}" text-anchor="middle" font-size="13" fill="${theme.muted}">${escapeXml(data.warnings[0] ?? "no data")}</text></svg>`);
    return parts.join("");
  }

  // ---- Y scale over all numeric values (bars start at 0)
  const ys = data.series.flatMap((s) => s.values.filter((v): v is number => v !== null));
  let yLo = ys.length ? Math.min(...ys) : 0;
  let yHi = ys.length ? Math.max(...ys) : 1;
  if (data.kind === "bar") {
    yLo = Math.min(0, yLo);
    yHi = Math.max(0, yHi);
  }
  const yScale = niceScale(yLo, yHi);
  const yPos = (v: number) => margin.top + plotH - ((v - yScale.min) / (yScale.max - yScale.min || 1)) * plotH;

  // ---- X positions: numeric x for scatter/line-with-x, else categories
  const numericX = data.x && data.kind !== "bar" && data.x.some((v) => v !== null);
  let xScale: Scale | null = null;
  let xPos: (i: number) => number | null;
  if (numericX) {
    const xs = (data.x ?? []).filter((v): v is number => v !== null);
    xScale = niceScale(Math.min(...xs), Math.max(...xs));
    const sc = xScale;
    xPos = (i) => {
      const v = data.x?.[i] ?? null;
      return v === null ? null : margin.left + ((v - sc.min) / (sc.max - sc.min || 1)) * plotW;
    };
  } else {
    const slot = plotW / Math.max(1, pointCount);
    xPos = (i) => margin.left + slot * (i + 0.5);
  }

  // ---- axes + grid
  const axisColor = theme.axis;
  parts.push(`<g class="ss-chart-grid">`);
  for (const t of yScale.ticks) {
    const y = yPos(t);
    parts.push(`<line x1="${margin.left}" y1="${y.toFixed(1)}" x2="${margin.left + plotW}" y2="${y.toFixed(1)}" stroke="${theme.grid}"/>`);
    parts.push(`<text x="${margin.left - 6}" y="${(y + 4).toFixed(1)}" text-anchor="end" font-size="11" fill="${theme.muted}">${formatTick(t)}</text>`);
  }
  parts.push(`</g>`);
  parts.push(`<line x1="${margin.left}" y1="${margin.top}" x2="${margin.left}" y2="${margin.top + plotH}" stroke="${axisColor}"/>`);
  parts.push(`<line x1="${margin.left}" y1="${margin.top + plotH}" x2="${margin.left + plotW}" y2="${margin.top + plotH}" stroke="${axisColor}"/>`);
  if (xScale) {
    const sc = xScale;
    for (const t of sc.ticks) {
      const x = margin.left + ((t - sc.min) / (sc.max - sc.min || 1)) * plotW;
      parts.push(`<text x="${x.toFixed(1)}" y="${margin.top + plotH + 16}" text-anchor="middle" font-size="11" fill="${theme.muted}">${formatTick(t)}</text>`);
    }
  } else {
    const every = Math.max(1, Math.ceil(pointCount / Math.max(1, Math.floor(plotW / 48))));
    for (let i = 0; i < pointCount; i += every) {
      const x = xPos(i);
      if (x === null) continue;
      const label = data.categories[i] ?? String(i + 1);
      parts.push(`<text x="${x.toFixed(1)}" y="${margin.top + plotH + 16}" text-anchor="middle" font-size="11" fill="${theme.muted}">${escapeXml(label.length > 12 ? `${label.slice(0, 11)}…` : label)}</text>`);
    }
  }
  if (data.xLabel) parts.push(`<text x="${margin.left + plotW / 2}" y="${height - legendHeight - 10}" text-anchor="middle" font-size="12" fill="${theme.text}">${escapeXml(data.xLabel)}</text>`);
  if (data.yLabel) parts.push(`<text transform="translate(14 ${margin.top + plotH / 2}) rotate(-90)" text-anchor="middle" font-size="12" fill="${theme.text}">${escapeXml(data.yLabel)}</text>`);

  // ---- series
  data.series.forEach((series, si) => {
    const color = theme.series[si % theme.series.length];
    if (data.kind === "bar") {
      const slot = plotW / Math.max(1, pointCount);
      const barW = Math.max(2, (slot * 0.7) / data.series.length);
      const zero = yPos(Math.max(yScale.min, Math.min(yScale.max, 0)));
      series.values.forEach((v, i) => {
        if (v === null) return;
        const x = margin.left + slot * i + slot * 0.15 + barW * si;
        const y = yPos(v);
        parts.push(`<rect class="ss-chart-bar" x="${x.toFixed(1)}" y="${Math.min(y, zero).toFixed(1)}" width="${barW.toFixed(1)}" height="${Math.abs(zero - y).toFixed(1)}" fill="${color}"><title>${escapeXml(series.name)}: ${v}</title></rect>`);
      });
    } else {
      const points: Array<[number, number, number]> = [];
      series.values.forEach((v, i) => {
        const x = xPos(i);
        if (v === null || x === null) return;
        points.push([x, yPos(v), v]);
      });
      if (data.kind === "line" && points.length > 1) {
        const sorted = numericX ? [...points].sort((a, b) => a[0] - b[0]) : points;
        parts.push(`<polyline class="ss-chart-line" fill="none" stroke="${color}" stroke-width="2" points="${sorted.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(" ")}"/>`);
      }
      for (const [x, y, v] of points) {
        parts.push(`<circle class="ss-chart-point" cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="${data.kind === "scatter" ? 4 : 3}" fill="${color}"><title>${escapeXml(series.name)}: ${v}</title></circle>`);
      }
    }
  });

  // ---- legend
  if (data.series.length > 1) {
    let x = margin.left;
    const y = height - 6;
    data.series.forEach((series, si) => {
      const color = theme.series[si % theme.series.length];
      parts.push(`<rect x="${x}" y="${y - 9}" width="10" height="10" fill="${color}"/>`);
      parts.push(`<text x="${x + 14}" y="${y}" font-size="11" fill="${theme.text}">${escapeXml(series.name)}</text>`);
      x += 14 + Math.min(120, series.name.length * 6.5 + 14);
    });
  }
  parts.push("</svg>");
  return parts.join("");
}
