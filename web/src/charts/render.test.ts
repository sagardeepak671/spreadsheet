import { describe, expect, it } from "vitest";

import type { ChartData } from "../bridge";
import { escapeXml, formatTick, niceScale, renderChartSvg } from "./render";

const base: ChartData = {
  kind: "bar",
  title: "Sales <2024>",
  xLabel: "Month",
  yLabel: "k€",
  categories: ["Jan", "Feb", "Mar"],
  x: null,
  series: [
    { name: "A", values: [10, 20, 30] },
    { name: "B", values: [5, null, 15] },
  ],
  warnings: [],
};

describe("niceScale", () => {
  it("produces round ticks that cover the range", () => {
    expect(niceScale(0, 100)).toEqual({ min: 0, max: 100, ticks: [0, 20, 40, 60, 80, 100] });
    const s = niceScale(3, 97);
    expect(s.min).toBeLessThanOrEqual(3);
    expect(s.max).toBeGreaterThanOrEqual(97);
    expect(s.ticks[0]).toBe(s.min);
    expect(s.ticks.at(-1)).toBe(s.max);
    expect(niceScale(5, 5).ticks.length).toBeGreaterThan(1);
    expect(niceScale(0, 0).ticks).toContain(0);
    expect(niceScale(-30, 10).min).toBeLessThanOrEqual(-30);
    expect(niceScale(NaN, 1)).toEqual({ min: 0, max: 1, ticks: [0, 0.5, 1] });
  });

  it("formats ticks compactly", () => {
    expect(formatTick(20)).toBe("20");
    expect(formatTick(0.25)).toBe("0.25");
    expect(formatTick(1234567)).toBe("1.2e+6");
    expect(escapeXml(`<a & "b">`)).toBe("&lt;a &amp; &quot;b&quot;&gt;");
  });
});

describe("renderChartSvg", () => {
  it("renders bars per numeric value, axis labels, title and legend", () => {
    const svg = renderChartSvg(base, { width: 400, height: 300 });
    expect(svg.startsWith("<svg")).toBe(true);
    expect(svg).toContain('class="ss-chart ss-chart-bar"');
    expect((svg.match(/class="ss-chart-bar"/g) ?? []).length).toBe(5); // null is skipped
    expect(svg).toContain("Sales &lt;2024&gt;");
    expect(svg).toContain(">Month<");
    expect(svg).toContain(">k€<");
    expect(svg).toContain(">Jan<");
    expect(svg).toContain(">A<"); // legend entries
    expect(svg).toContain(">B<");
    expect(svg).not.toContain("<2024>");
  });

  it("renders lines with points and scatter with points only", () => {
    const line = renderChartSvg({ ...base, kind: "line" });
    expect((line.match(/<polyline/g) ?? []).length).toBe(2);
    expect((line.match(/class="ss-chart-point"/g) ?? []).length).toBe(5);
    const scatter = renderChartSvg({ ...base, kind: "scatter", x: [1, 2, 10], categories: ["1", "2", "10"] });
    expect(scatter).not.toContain("<polyline");
    expect((scatter.match(/class="ss-chart-point"/g) ?? []).length).toBe(5);
    expect(scatter).toContain(">10<"); // numeric x axis ticks
  });

  it("shows a message instead of axes when there is nothing to plot", () => {
    const empty = renderChartSvg({ ...base, series: [], warnings: ["the range has no data points"] });
    expect(empty).toContain("the range has no data points");
    expect(empty).not.toContain("ss-chart-grid");
    const noPoints = renderChartSvg({ ...base, series: [{ name: "A", values: [] }], warnings: [] });
    expect(noPoints).toContain("no data");
  });

  it("takes its colours from the theme", () => {
    const svg = renderChartSvg(base, {
      theme: { background: "#0b1020", text: "#d6e4ff", muted: "#7f93bd", grid: "#1f2a48", axis: "#34466f", series: ["#22d3ee", "#f472b6"] },
    });
    expect(svg).toContain('fill="#0b1020"');
    expect(svg).toContain('fill="#22d3ee"');
    expect(svg).toContain('fill="#f472b6"');
    expect(svg).toContain('stroke="#1f2a48"');
    expect(svg).not.toContain("#1a73e8");
    // partial themes fall back to the defaults, an empty palette too
    expect(renderChartSvg(base, { theme: { series: [] } })).toContain('fill="#1a73e8"');
  });

  it("uses a nice scale that includes zero for bars", () => {
    const svg = renderChartSvg({ ...base, series: [{ name: "A", values: [50, 60] }] });
    expect(svg).toContain(">0<");
  });
});
