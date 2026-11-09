/** A thin ECharts wrapper (modular build: only the charts and components the viewer uses). */
import { HeatmapChart, ScatterChart } from "echarts/charts";
import { DataZoomComponent, GridComponent, LegendComponent, MarkLineComponent, TooltipComponent, VisualMapComponent } from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import { useEffect, useRef } from "react";

echarts.use([ScatterChart, HeatmapChart, GridComponent, TooltipComponent, VisualMapComponent, LegendComponent, MarkLineComponent, DataZoomComponent, CanvasRenderer]);

export type EOption = echarts.EChartsCoreOption;

export function EChart({ option, height = 220, onClick }: { option: EOption; height?: number; onClick?: (p: unknown) => void }) {
  const el = useRef<HTMLDivElement>(null);
  const chart = useRef<echarts.ECharts | null>(null);
  useEffect(() => {
    const c = echarts.init(el.current!, undefined, { renderer: "canvas" });
    chart.current = c;
    const ro = new ResizeObserver(() => c.resize());
    ro.observe(el.current!);
    return () => {
      ro.disconnect();
      c.dispose();
    };
  }, []);
  useEffect(() => {
    chart.current?.setOption(option, { notMerge: true, lazyUpdate: true });
  }, [option]);
  useEffect(() => {
    if (!onClick || !chart.current) return;
    const c = chart.current;
    c.on("click", onClick);
    return () => {
      c.off("click", onClick);
    };
  }, [onClick]);
  return <div ref={el} style={{ width: "100%", height }} />;
}
