"""Rebuild the five Beamer research PDFs with Tectonic in a temporary folder."""

import subprocess
import tempfile
from pathlib import Path

folder = Path(__file__).resolve().parent
modes = {
    "hidden": "hide notes",
    "interleaved": "show notes",
    "only": "show only notes",
    "second-screen": "show notes on second screen=right",
    "annotations": "hide notes",
}
with tempfile.TemporaryDirectory(prefix="qp168-beamer-") as temporary:
    work = Path(temporary)
    (work / "beamer.tex").write_bytes((folder / "beamer.tex").read_bytes())
    for name, mode in modes.items():
        wrapper = work / f"beamer-{name}.tex"
        wrapper.write_text(
            "\\def\\researchmode{" + mode + "}\n"
            + ("\\def\\annotationnotes{1}\n" if name == "annotations" else "")
            + "\\input{beamer.tex}\n"
        )
        subprocess.run(
            ["tectonic", wrapper.name, "--outdir", str(folder)],
            cwd=work, check=True,
        )
