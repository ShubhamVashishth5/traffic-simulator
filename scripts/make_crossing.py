"""Write the four-arm crossing scenarios (P7): the same geometry with a signal,
without one, and as a roundabout.

    uv run python scripts/make_crossing.py
"""

from pathlib import Path

ARMS = [("e", 1, 0), ("n", 0, 1), ("w", -1, 0), ("s", 0, -1)]
COMPOSITION = """        motorcycle: 0.26
        scooter: 0.14
        auto_rickshaw: 0.14
        e_rickshaw: 0.03
        hatchback: 0.24
        sedan: 0.08
        suv: 0.05
        lcv: 0.03
        bus: 0.02
        bicycle: 0.01
"""


def ribbons(arm_length: float, box: float, offset: float, half_width: float) -> str:
    """Each arm has a ribbon in and a ribbon out. Left-hand traffic: driving
    towards the junction, the arriving carriageway is on the left of the
    arm's centre line."""
    out = ""
    for name, ax, ay in ARMS:
        lx, ly = -ay, ax  # left of the outward axis
        far, near = box + arm_length, box
        w = f"[{{s: 0, y_min: {-half_width}, y_max: {half_width}}}]"
        out += (
            f"    - id: {name}_in\n"
            f"      polyline: [[{ax * far - lx * offset}, {ay * far - ly * offset}], "
            f"[{ax * near - lx * offset}, {ay * near - ly * offset}]]\n"
            f"      width: {w}\n"
            f"    - id: {name}_out\n"
            f"      polyline: [[{ax * near + lx * offset}, {ay * near + ly * offset}], "
            f"[{ax * far + lx * offset}, {ay * far + ly * offset}]]\n"
            f"      width: {w}\n"
        )
    return out


def turns(through: float, left: float, right: float) -> str:
    names = [a[0] for a in ARMS]
    out = ""
    for k, name in enumerate(names):
        # Counter-clockwise order e, n, w, s: the next arm is on the right
        # of a vehicle arriving (it turns right to leave by it).
        right_arm, through_arm, left_arm = names[(k + 1) % 4], names[(k + 2) % 4], names[(k + 3) % 4]
        out += f"        {name}: {{{through_arm}: {through}, {left_arm}: {left}, {right_arm}: {right}}}\n"
    return out


def scenario(name: str, description: str, junction: str, flows: dict[str, float], overrides: str,
             duration: int = 1800, extra_outputs: str = "", box: float = 14) -> str:
    entries = ""
    detectors = ""
    for arm, _, _ in ARMS:
        entries += (
            f"    - id: {arm}\n      ribbon: {arm}_in\n"
            f"      flow: [{{from_s: 0, veh_per_h: {flows[arm]}}}]\n      composition:\n{COMPOSITION}"
        )
        detectors += f"  - {{kind: screenline, id: out_{arm}, ribbon: {arm}_out, s: 100, interval_s: 300}}\n"
        detectors += f"  - {{kind: screenline, id: in_{arm}, ribbon: {arm}_in, s: 150, interval_s: 300}}\n"
    return (
        f"name: {name}\ndescription: >\n  {description}\n"
        f"duration_s: {duration}\nwarmup_s: 300\ndt_s: 0.1\nnetwork:\n  ribbons:\n"
        f"{ribbons(300, box, 4.0, 3.5)}  junctions:\n{junction}"
        f"demand:\n  entries:\n{entries}detectors:\n{detectors}{extra_outputs}overrides:\n{overrides}"
    )


ARMS_YAML = "".join(f"        - {{id: {a}, in: {a}_in, out: {a}_out}}\n" for a, _, _ in ARMS)
TURN = (
    "  road.junction.turn_lateral_acceleration: 2.0   # placeholder: not in the guide\n"
    "  road.split.route_weight: 1.0                   # placeholder: G§6.7 gives no number\n"
    "  road.split.urgency_clearance_link: 0.5         # placeholder\n"
)

SIGNAL = (
    "    - id: crossing\n      arms:\n" + ARMS_YAML + "      turns:\n" + turns(0.6, 0.2, 0.2)
    + "      control:\n        kind: signal\n        countdown: working\n        phases:\n"
    "          - {arms: [e, w], green_s: 35, amber_s: 3, all_red_s: 2}\n"
    "          - {arms: [n, s], green_s: 25, amber_s: 3, all_red_s: 2}\n"
)
PRIORITY = (
    "    - id: crossing\n      arms:\n" + ARMS_YAML + "      turns:\n" + turns(0.6, 0.2, 0.2)
    + "      control: {kind: uncontrolled, major: [e, w]}\n"
)
ROUNDABOUT = (
    "    - id: rotary\n      kind: roundabout\n      centre: [0, 0]\n      ring_radius: 16\n      ring_width: 8\n"
    "      arms:\n" + ARMS_YAML + "      turns:\n" + turns(0.6, 0.2, 0.2)
)

if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1] / "scenarios"
    even = {a: 700 for a, _, _ in ARMS}
    (root / "signal_crossing.yaml").write_text(scenario(
        "signal_crossing",
        "A four-arm crossing of two 7 m dual carriageways under a two-stage fixed-time signal "
        "(70 s cycle). Right turns filter through the opposing stream on a gap (P7.3, P7.4).",
        SIGNAL, even, TURN))
    (root / "priority_crossing.yaml").write_text(scenario(
        "priority_crossing",
        "The same crossing without a signal: the east-west road is the major road, and traffic "
        "from north and south waits for gaps in it (G§2.4, P7.4).",
        PRIORITY, {"e": 500, "w": 500, "n": 250, "s": 250}, TURN))
    (root / "roundabout.yaml").write_text(scenario(
        "roundabout",
        "The same four arms round a rotary of 16 m radius and 8 m ring width (ADR-0009, P7.8). "
        "Each arm's stop line is 24 m from the centre, 4 m outside the ring. The entry gap "
        "factor and the share of entrants who wait for a gap are placeholders: the guide has no "
        "roundabout numbers.",
        ROUNDABOUT, {a: 500 for a, _, _ in ARMS},
        TURN
        + "  road.junction.roundabout.entry_critical_gap_factor: 0.6   # placeholder\n"
        + "  road.junction.roundabout.priority_compliance: 0.8         # placeholder\n",
        box=24))
    print("wrote signal_crossing, priority_crossing, roundabout")
