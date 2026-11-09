"""House style for every figure in reports and papers (plan P3.9).

Static, light-surface figures (PNG for HTML reports, PDF for papers). Colours
follow a validated categorical order (adjacent pairs separable under colour-
vision deficiency); categorical hues are assigned in this fixed order and never
cycled. Marks are thin (2 px lines, ≥ 8 px markers), gridlines are recessive
hairlines, and text is always in ink colours, never in a series colour.
"""

from __future__ import annotations

from typing import Any

#: Categorical series colours, in assignment order.
SERIES = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"]
SURFACE = "#fcfcfb"
INK = "#0b0b0b"
INK_2 = "#52514e"
MUTED = "#898781"
GRID = "#e1e0d9"
AXIS = "#c3c2b7"
#: Neutral reference lines (zero, targets).
REFERENCE = "#898781"
#: Target bands (validation tolerance), as a wash.
BAND = "#f0efec"

RC: dict[str, Any] = {
    "figure.facecolor": SURFACE,
    "axes.facecolor": SURFACE,
    "savefig.facecolor": SURFACE,
    "figure.dpi": 110,
    "savefig.dpi": 200,
    "savefig.bbox": "tight",
    "font.family": "sans-serif",
    "font.sans-serif": ["DejaVu Sans", "Helvetica", "Arial", "sans-serif"],
    "font.size": 9,
    "axes.titlesize": 10,
    "axes.titleweight": "bold",
    "axes.titlelocation": "left",
    "axes.labelsize": 9,
    "axes.labelcolor": INK_2,
    "axes.edgecolor": AXIS,
    "axes.linewidth": 0.8,
    "axes.spines.top": False,
    "axes.spines.right": False,
    "axes.grid": True,
    "axes.axisbelow": True,
    "grid.color": GRID,
    "grid.linewidth": 0.6,
    "grid.linestyle": "-",
    "xtick.color": MUTED,
    "ytick.color": MUTED,
    "xtick.labelcolor": INK_2,
    "ytick.labelcolor": INK_2,
    "xtick.labelsize": 8,
    "ytick.labelsize": 8,
    "lines.linewidth": 1.6,
    "lines.solid_capstyle": "round",
    "lines.solid_joinstyle": "round",
    "lines.markersize": 6,
    "legend.frameon": False,
    "legend.fontsize": 8,
    "legend.labelcolor": INK_2,
    "text.color": INK,
    "pdf.fonttype": 42,
}


def use() -> None:
    """Apply the house style to matplotlib (Agg backend if none is set)."""
    import matplotlib

    if matplotlib.get_backend().lower() not in ("agg", "pdf", "svg"):
        try:
            matplotlib.use("Agg")
        except Exception:  # noqa: BLE001 - interactive sessions keep their backend
            pass
    from cycler import cycler  # a matplotlib dependency

    matplotlib.rcParams.update(RC)
    matplotlib.rcParams["axes.prop_cycle"] = cycler(color=SERIES)


def color(i: int) -> str:
    """The ``i``-th categorical colour (series beyond eight must be folded or faceted)."""
    if i >= len(SERIES):
        raise ValueError("more than 8 categorical series: fold into 'Other' or use small multiples")
    return SERIES[i]


def save(fig: Any, path: str, formats: tuple[str, ...] = ("png",)) -> list[str]:
    """Save ``fig`` as ``path.<fmt>`` for each format; returns the paths."""
    import matplotlib.pyplot as plt

    out = []
    base = str(path).removesuffix(".png").removesuffix(".pdf")
    for f in formats:
        p = f"{base}.{f}"
        fig.savefig(p)
        out.append(p)
    plt.close(fig)
    return out
