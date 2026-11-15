"""Text files are read and written as UTF-8 on every platform.

Windows reads and writes a text file opened without an encoding as cp1252. The
shipped scenarios and the register hold non-ASCII text (``§``, ``—``), so the
first Windows wheel computed an experiment's run hashes from mis-decoded
scenario text, and ``marg batch``, reading the file as UTF-8, refused every run
("run hash mismatch"). Python flags such calls on any platform with
``-X warn_default_encoding``; here they are errors in marg's own code.
"""

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

CHILD = """
import sys, warnings
warnings.filterwarnings("error", category=EncodingWarning, module="marg")
import marg
exp = marg.Experiment("enc", sys.argv[1], replications=marg.Replications(n=2))
exp.run(marg.backends.Local(workers=1))
assert exp.results("kpis").height == 2
print("ok")
"""


def test_an_experiment_opens_every_text_file_as_utf8(tmp_path):
    scenario = REPO / "scenarios" / "free_flow_early_morning.yaml"
    assert any(ord(c) > 127 for c in scenario.read_text(encoding="utf-8"))
    p = subprocess.run(
        [sys.executable, "-X", "warn_default_encoding", "-c", CHILD, str(scenario)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env={**os.environ, "MARG_PARAMS_DIR": str(REPO / "params" / "base")},
    )
    assert p.returncode == 0 and p.stdout.strip().endswith("ok"), p.stderr[-3000:]
