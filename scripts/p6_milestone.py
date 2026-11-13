"""P6 milestone (plan/phases/P06-road-objects-side-friction.md) and phase report.

Runs, with the shipped register and nothing tuned towards any target:

1. P6.8: capacity against the width parked vehicles take (the Varanasi
   relation of about 1:1, validation/targets/patterns.yaml).
2. P6.4: capacity and speed with and without a kerbside bus stop.
3. P6.7: capacity against the number of pedestrians crossing per hour.
4. P6.5: free-flow speed on a potholed stretch against a sound one.
5. P6.6: speeds over a hump, and e-rickshaws on a 4% ramp.

The guide has no numbers for most of the inputs these mechanisms need (the
register leaves them unset). The values in PLACEHOLDERS below exercise the
mechanisms; they are not survey results, so every comparison with a published
figure is an order-of-magnitude check.

Capacity is the flow past a screenline downstream of the feature while a
queue stands at the entry (demand is set well above capacity).

Writes reports/phases/P6.md and reports/phases/P6_milestone.json.

Usage: uv run python scripts/p6_milestone.py [--seeds 3] [--jobs 4]
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import polars as pl

ROOT = Path(__file__).resolve().parent.parent
MARG = ROOT / "target" / "release" / "marg"

# Inputs the guide gives no number for. Placeholders, not survey results.
PLACEHOLDERS = {
    "road.parking.manoeuvre_duration": "{dist: log_normal_median, median: 20, sigma: 0.5}",
    "road.bus.dwell_time": "{dist: log_normal_median, median: 20, sigma: 0.5}",
    "road.bus.stop_offset_longitudinal": "{dist: normal, mean: 0, sd: 5}",
    "road.bus.stop_offset_lateral": "{dist: uniform, min: 0.3, max: 1.5}",
    "road.pedestrian.accepted_gap": "{dist: log_normal_median, median: 4, sigma: 0.3}",
    "road.pedestrian.group_size": "{dist: uniform, min: 0.5, max: 3.5}",
    "road.pedestrian.walking_speed": "1.2",
    "road.pedestrian.body_size": "0.6",
    "road.pothole.detection_distance": "30",
    "road.pothole.aggressiveness_link": "0.5",
    "road.pothole.crossing_speed.two_wheeler": "10",
    "road.pothole.crossing_speed.car": "15",
    "road.pothole.crossing_speed.heavy": "20",
    "road.pothole.response.two_wheeler.swerve": "0.7",
    "road.pothole.response.two_wheeler.slow": "0.2",
    "road.pothole.response.two_wheeler.straddle": "0.0",
    "road.pothole.response.car.swerve": "0.4",
    "road.pothole.response.car.slow": "0.35",
    "road.pothole.response.car.straddle": "0.15",
    "road.pothole.response.heavy.swerve": "0.1",
    "road.pothole.response.heavy.slow": "0.6",
    "road.pothole.response.heavy.straddle": "0.2",
    "vehicles.e_rickshaw.loaded_mass": "700",
    "vehicles.gradient_speed_loss.auto_rickshaw": "2.5",
    "vehicles.gradient_speed_loss.tractor": "2.06",
}

MIXED = "{hatchback: 0.22, sedan: 0.10, suv: 0.06, motorcycle: 0.25, scooter: 0.20, auto_rickshaw: 0.12, bus: 0.03, lcv: 0.02}"


def scenario(width: float, flow: float, network: str, composition: str = MIXED, duration: int = 1500) -> str:
    """A 1.2 km road with the feature in the middle and detectors round it."""
    half = width / 2
    return f"""
name: p6
duration_s: {duration}
warmup_s: 300
network:
  ribbons: [{{id: main, polyline: [[0, 0], [1200, 0]], width: [{{s: 0, y_min: {-half}, y_max: {half}}}]}}]
{network}
demand: {{entries: [{{id: west, ribbon: main, flow: [{{from_s: 0, veh_per_h: {flow}}}], composition: {composition}}}]}}
detectors:
  - {{kind: screenline, id: out, ribbon: main, s: 1100, interval_s: 300}}
  - {{kind: screenline, id: at, ribbon: main, s: 600, interval_s: 300}}
  - {{kind: area, id: before, ribbon: main, s_from: 100, s_to: 350, interval_s: 300}}
  - {{kind: area, id: feature, ribbon: main, s_from: 400, s_to: 800, interval_s: 300}}
kpi: {{throughput_detector: out}}
outputs: {{effective_width: [{{id: w, ribbon: main, s_from: 400, s_to: 800, bin_m: 400}}]}}
"""


def run(args: tuple[str, str, int, dict[str, str]]) -> dict:
    """One run; returns its headline numbers."""
    name, yaml, seed, extra = args
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "s.yaml"
        path.write_text(yaml)
        cmd = [str(MARG), "run", str(path), "--seed", str(seed), "--out", tmp + "/run"]
        for key, value in {**PLACEHOLDERS, **extra}.items():
            cmd += ["--set", f"{key}={value}"]
        done = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT)
        if done.returncode != 0:
            raise RuntimeError(f"{name} seed {seed}: {done.stderr[-800:]}")
        out = Path(tmp) / "run"
        kpis = pl.read_parquet(out / "kpis.parquet").row(0, named=True)
        det = pl.read_parquet(out / "detectors.parquet").filter(~pl.col("in_warmup"))
        area = det.filter(pl.col("kind") == "area").group_by("detector_id").agg(
            (pl.col("ttd").sum() / pl.col("tts").sum() * 3.6).alias("kmh")
        )
        speed = dict(zip(area["detector_id"], area["kmh"], strict=True))
        at = det.filter((pl.col("detector_id") == "at") & (pl.col("count") > 0))
        by_class = at.group_by("class").agg(
            ((pl.col("time_mean_speed") * pl.col("count")).sum() / pl.col("count").sum() * 3.6).alias("kmh")
        )
        width = pl.read_parquet(out / "effective_width.parquet").row(0, named=True)
        events = pl.read_parquet(out / "events.parquet").group_by("kind").len()
        return {
            "name": name,
            "seed": seed,
            "throughput": kpis["throughput_veh_h"],
            "waiting": kpis["waiting_to_enter_at_end"],
            "guard": kpis["collision_guard_events"],
            "delay": kpis["delay_mean_s"],
            "speed_before": speed.get("before"),
            "speed_feature": speed.get("feature"),
            "speed_at": dict(zip(by_class["class"], by_class["kmh"], strict=True)),
            "blocked_width": width["blocked_width"],
            "effective_width": width["effective_width_observed"],
            "shy": width["shy_observed"],
            "road_width": width["road_width"],
            "events": dict(zip(events["kind"], events["len"], strict=True)),
        }


def mean_sd(xs: list[float]) -> tuple[float, float]:
    return statistics.fmean(xs), (statistics.stdev(xs) if len(xs) > 1 else 0.0)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=3)
    ap.add_argument("--jobs", type=int, default=4)
    a = ap.parse_args()
    seeds = range(1, a.seeds + 1)
    jobs: list[tuple[str, str, int, dict[str, str]]] = []
    static = "double_parking_probability: {d}, manoeuvres_per_100m_h: 0, cruising_share: 0"

    # 1. Width against capacity: a 10.5 m carriageway, 400 m of kerb.
    def parking(composition: str, offset: float, double: float, occupancy: float = 0.9) -> str:
        return (
            "  parking:\n    - {id: kerb, ribbon: main, s_from: 400, s_to: 800, occupancy: "
            f"{occupancy}, composition: {composition}, offset_from_kerb: {{dist: constant, value: {offset}}}, "
            + static.format(d=double)
            + "}"
        )

    cars = "{hatchback: 0.6, sedan: 0.3, suv: 0.1}"
    arms = {
        "none": "",
        "motorcycles": parking("{motorcycle: 0.5, scooter: 0.5}", 0.2, 0.0),
        "cars at the kerb": parking(cars, 0.2, 0.0),
        "cars 0.8 m out": parking(cars, 0.8, 0.0),
        "cars, half double parked": parking(cars, 0.3, 0.5),
        "cars, all double parked": parking(cars, 0.3, 1.0),
    }
    for arm, net in arms.items():
        jobs += [(f"width|{arm}", scenario(10.5, 9000, net), s, {}) for s in seeds]
    # Diagnostic, not part of the fit: the same kerb of cars with every
    # class able to creep sideways at 1.5 m/s when stopped.
    classes = ["bicycle", "cycle_rickshaw", "two_wheeler", "e_rickshaw", "auto_rickshaw",
               "car", "lcv", "bus", "truck", "tractor"]
    fast = {f"interaction.lateral.creep_speed.{c}": "1.5" for c in classes}
    jobs += [("diagnostic|fast creep", scenario(10.5, 9000, arms["cars at the kerb"]), s, fast) for s in seeds]

    # The model's own relation between width and capacity: plain roads.
    plain_widths = [9.0, 7.5, 6.0]
    for w in plain_widths:
        jobs += [(f"plain|{w}", scenario(w, 9000, ""), s, {}) for s in seeds]

    # 2. A bus stop on a 7 m carriageway (two lanes each way, as at Warangal).
    def bus_scenario(flow: float, per_hour: float, passengers: float | None) -> str:
        """Buses enter at their own rate, apart from the rest of the stream."""
        net = "" if passengers is None else (
            "  bus_stops: [{id: stop, ribbon: main, s: 600, length: 30, serving_share: 1, "
            f"waiting_passenger_width: {passengers}}}]"
        )
        text = scenario(7.0, flow, net, MIXED.replace("bus: 0.03, ", ""))
        return text.replace(
            "demand: {entries: [",
            f"demand: {{entries: [{{id: buses, ribbon: main, flow: [{{from_s: 0, veh_per_h: {per_hour}}}], composition: {{bus: 1.0}}}}, ",
        )

    at_kerb = {"road.bus.stop_offset_lateral": "{dist: uniform, min: 0.2, max: 0.5}"}
    bus_arms = {
        "no stop, 30 buses/h": (30, None, {}),
        "at the kerb, 30 buses/h": (30, 0.0, at_kerb),
        "out in the road, 30 buses/h": (30, 0.6, {}),
        "out in the road, 70 buses/h": (70, 0.6, {}),
    }
    for flow, label in ((6000, "capacity"), (1800, "moderate")):
        for arm, (per_hour, passengers, extra) in bus_arms.items():
            jobs += [(f"bus|{label}|{arm}", bus_scenario(flow, per_hour, passengers), s, extra) for s in seeds]

    # 3. Pedestrians crossing along 100 m of a 10.5 m carriageway.
    ped_flows = [0, 100, 200, 400, 800, 1600]
    for n in ped_flows:
        net = f"  pedestrian_crossings: [{{id: p, ribbon: main, s_from: 550, s_to: 650, per_hour: {n}, from_left_share: 0.5}}]" if n else ""
        jobs += [(f"ped|{n}", scenario(10.5, 9000, net), s, {}) for s in seeds]

    # 4. Potholes at low flow (free-flow speed).
    for per_km in (0, 40, 120):
        net = f"  pothole_fields: [{{id: f, ribbon: main, s_from: 400, s_to: 800, per_km: {per_km}, kerb_bias: 0.5}}]" if per_km else ""
        jobs += [(f"pothole|{per_km}", scenario(7.0, 400, net), s, {}) for s in seeds]

    # 5. A hump, and a 4% ramp with e-rickshaws.
    jobs += [("hump", scenario(7.0, 600, "  humps: [{id: h, ribbon: main, s: 600, length: 3.7}]"), s, {}) for s in seeds]
    erick = "{hatchback: 0.4, motorcycle: 0.4, e_rickshaw: 0.2}"
    ramp = "  surfaces: [{{id: ramp, ribbon: main, s_from: 400, s_to: 800, gradient_percent: {g}}}]"
    for grade in (0, 4, 8):
        net = ramp.format(g=grade) if grade else ""
        jobs += [(f"ramp|{grade}", scenario(7.0, 1200, net, erick), s, {}) for s in seeds]

    print(f"{len(jobs)} runs on {a.jobs} workers")
    with ThreadPoolExecutor(a.jobs) as pool:
        results = list(pool.map(run, jobs))
    by: dict[str, list[dict]] = {}
    for r in results:
        by.setdefault(r["name"], []).append(r)
    guard = sum(r["guard"] for r in results)

    def stat(name: str, key: str) -> tuple[float, float]:
        return mean_sd([r[key] for r in by[name]])

    md: list[str] = []
    md += [
        "# P6 milestone: road objects and side friction",
        "",
        "Generated by `scripts/p6_milestone.py`. Nothing here was tuned towards a target.",
        "",
        "**Read this first.** The guide has no numbers for most of what these mechanisms need",
        "(kerb offsets, dwell times, pedestrian gaps, pothole responses and more), and the register",
        "leaves them unset. The runs below use the placeholder values listed at the end. Every",
        "comparison with a published figure is therefore an order-of-magnitude check of the",
        "mechanism, not a validation.",
        "",
        f"Runs: {len(results)} ({a.seeds} seeds each), **{guard:.0f} collision-guard events** in all of them.",
        "Capacity is the flow past a screenline 300 m downstream of the feature with a queue at the entry.",
        "",
    ]

    # ---- 1. width
    base, _ = stat("width|none", "throughput")
    md += ["## 1. Width taken by parking against capacity (P6.8)", "",
           "10.5 m carriageway, 400 m of kerb at 90% occupancy, mixed traffic, demand above capacity.",
           "Two measures of the width lost, both from `effective_width.parquet`: the **blocked** width",
           "(footprints plus a strip to the kerb too narrow for any vehicle), and the loss of",
           "**effective** width, which adds the shy distance traffic was observed to keep (G§6.2:",
           "`W_eff = W − Σw − c_shy`).", "",
           "| Arm | Blocked | Effective-width loss | Capacity (veh/h) | Capacity loss |", "| --- | --- | --- | --- | --- |"]
    pts, pts_eff = [], []
    for arm in arms:
        rs = by[f"width|{arm}"]
        if arm != "none":
            for r in rs:
                loss = 100 * (1 - r["throughput"] / base)
                pts.append((100 * r["blocked_width"] / r["road_width"], loss))
                pts_eff.append((100 * (1 - r["effective_width"] / r["road_width"]), loss))
        w, _ = mean_sd([100 * r["blocked_width"] / r["road_width"] for r in rs])
        e = "" if arm == "none" else f"{statistics.fmean(100 * (1 - r['effective_width'] / r['road_width']) for r in rs):.1f}%"
        c, sd = stat(f"width|{arm}", "throughput")
        md.append(f"| {arm} | {w:.1f}% | {e} | {c:.0f} ± {sd:.0f} | {100 * (1 - c / base):.1f}% |")

    def slopes(points: list[tuple[float, float]]) -> dict:
        x, y = np.array([q[0] for q in points]), np.array([q[1] for q in points])
        slope = float((x * y).sum() / (x * x).sum())
        se = float(np.sqrt(((y - slope * x) ** 2).sum() / (len(x) - 1) / (x * x).sum()))
        fit = np.polyfit(x, y, 1)
        return {"slope": slope, "ci95": 1.96 * se, "slope_free": float(fit[0]), "intercept": float(fit[1]), "n": len(x)}

    s_blocked, s_eff = slopes(pts), slopes(pts_eff)
    md += ["", "Plain roads of other widths, no parking (the model's own width–capacity relation):", "",
           "| Road width | Width loss | Capacity (veh/h) | Capacity loss |", "| --- | --- | --- | --- |"]
    pts_plain = []
    for w in plain_widths:
        c, sd = stat(f"plain|{w}", "throughput")
        pts_plain += [(100 * (1 - w / 10.5), 100 * (1 - r["throughput"] / base)) for r in by[f"plain|{w}"]]
        md.append(f"| {w} m | {100 * (1 - w / 10.5):.1f}% | {c:.0f} ± {sd:.0f} | {100 * (1 - c / base):.1f}% |")
    s_plain = slopes(pts_plain)
    md += ["",
           "| Capacity loss per point of width loss | Slope through the origin (95% CI) | With an intercept |", "| --- | --- | --- |",
           f"| Plain narrower road | {s_plain['slope']:.2f} ± {s_plain['ci95']:.2f} | {s_plain['slope_free']:.2f}, intercept {s_plain['intercept']:.1f} |",
           f"| Parking, against effective-width loss | {s_eff['slope']:.2f} ± {s_eff['ci95']:.2f} | {s_eff['slope_free']:.2f}, intercept {s_eff['intercept']:.1f} |",
           f"| Parking, against blocked width | {s_blocked['slope']:.2f} ± {s_blocked['ci95']:.2f} | {s_blocked['slope_free']:.2f}, intercept {s_blocked['intercept']:.1f} |",
           "",
           "The Varanasi series is about 1.0 against effective-width loss (tolerance ±0.2 in",
           "`validation/targets/patterns.yaml`).", ""]
    width_result = {"blocked": s_blocked, "effective": s_eff, "plain": s_plain, "points_effective": pts_eff}
    dc, _ = stat("diagnostic|fast creep", "throughput")
    de = statistics.fmean(100 * (1 - r["effective_width"] / r["road_width"]) for r in by["diagnostic|fast creep"])
    kc, _ = stat("width|cars at the kerb", "throughput")
    vb, _ = stat("width|cars at the kerb", "speed_before")
    vf, _ = stat("width|cars at the kerb", "speed_feature")
    verdict = "within" if abs(s_eff["slope"] - 1.0) <= 0.2 else "outside"
    md += [f"**The target is {'met' if verdict == 'within' else 'missed'}.** A plain narrower road loses capacity one for one with",
           f"its width, so the lateral model itself gives the Varanasi relation. Parking loses {s_eff['slope']:.2f} times its",
           f"effective-width loss, {verdict} the target band (0.8–1.2). The loss is made at the start of the parked",
           f"row: with cars at the kerb the stream runs at {vf:.0f} km/h beside them and queues at {vb:.0f} km/h before",
           "them. Vehicles arriving in the kerb strip must merge into the stream beside it there.",
           "",
           "Since 2026-11-13 vehicles keep room to swing out from behind a standing body and pull out",
           "moving forward (ADR-0012 part 2, option B). Before that the slope was 1.63 ± 0.11 (creep alone).",
           "Letting every class also slide sideways at 1.5 m/s when stopped (a diagnostic, not a model) brings",
           f"the same kerb to a capacity loss of {100 * (1 - dc / base):.1f}% (against {100 * (1 - kc / base):.1f}%) for an",
           f"effective-width loss of {de:.1f}%. Sliding lets a stopped vehicle take a gap of its own length in the",
           "stream beside it; steering out needs a longer one. So what remains is how readily vehicles merge",
           "and are let in at the head of the row (the safety and politeness terms of the lateral choice,",
           "stage 3, and forced merging, P8), not width or desired speeds. That is a hypothesis from this",
           "diagnostic, not yet tested. Nothing was tuned.", ""]
    width_result["diagnostic_fast_creep"] = {"capacity_loss_pct": 100 * (1 - dc / base), "effective_width_loss_pct": de}

    # ---- 2. bus stop
    md += ["## 2. Kerbside bus stop (P6.4)", "",
           "7 m carriageway (two lanes each way). Every bus stops. \"At the kerb\": 0.2–0.5 m out, no",
           "passengers on the road. \"Out in the road\": 0.3–1.5 m out, beyond 0.6 m of waiting passengers.", "",
           "| Arm | Capacity (veh/h) | Loss | Speed past the stop at 1,800 veh/h (km/h) | Loss |", "| --- | --- | --- | --- | --- |"]
    bus = {}
    ref = "no stop, 30 buses/h"
    for arm in bus_arms:
        t_, sd = stat(f"bus|capacity|{arm}", "throughput")
        v, _ = stat(f"bus|moderate|{arm}", "speed_feature")
        bus[arm] = (t_, v)
    for arm in bus_arms:
        t_, v = bus[arm]
        _, sd = stat(f"bus|capacity|{arm}", "throughput")
        md.append(f"| {arm} | {t_:.0f} ± {sd:.0f} | {100 * (1 - t_ / bus[ref][0]):.1f}% | {v:.1f} | {100 * (1 - v / bus[ref][1]):.1f}% |")
    md += ["",
           "Published: 7.92% capacity drop on a Warangal four-lane section (tolerance ±4 points) and speed",
           "down by up to 22%. The bus frequency, dwell and stopping position of that section are not in",
           "the guide, so the arms bracket them: a bus that stands at the kerb costs less than the",
           "published figure, one that stands out in the road, beyond passengers, far more. The second is",
           "the same effect as in section 1: a standing body that vehicles in the kerb strip must merge",
           "out from behind.", ""]
    bus_result = {arm: {"capacity": t_, "speed": v, "capacity_loss_pct": 100 * (1 - t_ / bus[ref][0]), "speed_loss_pct": 100 * (1 - v / bus[ref][1])} for arm, (t_, v) in bus.items()}

    # ---- 3. pedestrians
    md += ["## 3. Pedestrians crossing midblock (P6.7)", "",
           "10.5 m carriageway (three lanes), crossings spread over 100 m, groups of 1–3.", "",
           "| Crossings per hour | Capacity (veh/h) | Loss | Crossed | Guard events |", "| --- | --- | --- | --- | --- |"]
    ped0, _ = stat("ped|0", "throughput")
    ped = {}
    for n in ped_flows:
        c, sd = stat(f"ped|{n}", "throughput")
        crossed = statistics.fmean(r["events"].get("pedestrian_crossing", 0) for r in by[f"ped|{n}"])
        ped[n] = 100 * (1 - c / ped0)
        hits = sum(r["guard"] for r in by[f"ped|{n}"])
        md.append(f"| {n} | {c:.0f} ± {sd:.0f} | {ped[n]:.1f}% | {crossed:.0f} | {hits:.0f} |")
    noise = 100 * stat("ped|0", "throughput")[1] / ped0
    md += ["",
           f"Run-to-run spread of capacity without pedestrians: ±{noise:.1f}%. Published: practically no",
           "effect below about 200 pedestrians per hour (Dhamaniya & Chandra 2014), and 19–56% at busy",
           "six-lane midblocks in Hyderabad.", "",
           "The guard events are vehicles that drove into a pedestrian and were put back: the gap rule",
           "is the pedestrian's, and a vehicle that accelerates or changes line after the pedestrian has",
           "stepped out can still reach it. They are counted, not hidden.", ""]

    # ---- 4. potholes
    md += ["## 4. Potholes at free flow (P6.5)", "",
           "7 m carriageway, 400 veh/h, potholes along 400 m.", "",
           "| Potholes per km | Speed on the stretch (km/h) | Loss |", "| --- | --- | --- |"]
    v0, _ = stat("pothole|0", "speed_feature")
    pot = {}
    for per_km in (0, 40, 120):
        v, sd = stat(f"pothole|{per_km}", "speed_feature")
        pot[per_km] = 100 * (1 - v / v0)
        md.append(f"| {per_km} | {v:.1f} ± {sd:.1f} | {pot[per_km]:.1f}% |")
    md += ["",
           "Published: 36% lower free-flow speed on a potholed Indonesian road (24.2 against 38.1 km/h;",
           "tolerance ±8 points). The pothole density of that road is not reported, so the density here",
           "is a free input and the comparison says only that a loss of that size is within reach.", ""]

    # ---- 5. hump and ramp
    hump = {}
    for r in by["hump"]:
        for cls, v in r["speed_at"].items():
            hump.setdefault(cls, []).append(v)
    md += ["## 5. Hump and ramp (P6.6)", "",
           "Speed at a screenline on a 3.7 m hump across the whole road:", "",
           "| Class | Speed on the hump (km/h) |", "| --- | --- |"]
    for cls, vs in sorted(hump.items()):
        md.append(f"| {cls} | {statistics.fmean(vs):.1f} |")
    md += ["", "The register's hump speed is uniform 14–23 km/h; the published band is 14–23 km/h.", "",
           "E-rickshaws (20% of the stream) on a 400 m ramp, speed at mid-ramp:", "",
           "| Upgrade | E-rickshaw (km/h) | Car (km/h) | Two-wheeler (km/h) | Stream on the ramp (km/h) |", "| --- | --- | --- | --- | --- |"]
    ramp_out = {}
    for grade in (0, 4, 8):
        rs = by[f"ramp|{grade}"]
        def cls(c: str, rs: list[dict] = rs) -> float:
            return statistics.fmean(r["speed_at"].get(c, float("nan")) for r in rs)

        v, _ = stat(f"ramp|{grade}", "speed_feature")
        ramp_out[grade] = {"e_rickshaw": cls("e_rickshaw"), "car": cls("car"), "stream": v}
        md.append(f"| {grade}% | {cls('e_rickshaw'):.1f} | {cls('car'):.1f} | {cls('two_wheeler'):.1f} | {v:.1f} |")
    cap4 = 2000 / (700 * 9.80665 * 0.04) * 3.6
    cap8 = 2000 / (700 * 9.80665 * 0.08) * 3.6
    md += ["",
           f"With the legal 2 kW and a placeholder 700 kg, power allows {cap4:.0f} km/h on 4% and {cap8:.0f} km/h on 8%",
           "(rolling resistance and drag neglected). An e-rickshaw's free speed is 21 km/h, so **on a 4% ramp",
           "the power cap does not bind**: by this derivation it is no slower there than on the level, where it",
           "is already the slowest vehicle. It binds on 8%.", ""]

    md += ["## Placeholder inputs", "", "| Register entry | Value used |", "| --- | --- |"]
    md += [f"| `{k}` | `{v}` |" for k, v in PLACEHOLDERS.items()]
    md += ["", "Parking arms: 90% occupancy, constant kerb offsets as named, no manoeuvres or cruising.",
           "Bus stop: every bus serves it. Pedestrians: half start from each edge. Potholes: kerb bias 0.5.", ""]

    out = ROOT / "reports" / "phases"
    (out / "P6.md").write_text("\n".join(md))
    (out / "P6_milestone.json").write_text(json.dumps({
        "width": width_result, "bus": bus_result,
        "pedestrians": ped, "pedestrian_noise_pct": noise, "potholes": pot, "hump": {k: statistics.fmean(v) for k, v in hump.items()},
        "ramp": ramp_out, "guard_events": guard, "runs": results,
    }, indent=1, default=str))
    print("\n".join(md))


if __name__ == "__main__":
    main()
