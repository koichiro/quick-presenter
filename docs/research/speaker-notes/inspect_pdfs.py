"""Inspect authored research samples; requires pypdf and Poppler pdftotext.

Run from the repository root:
  python3 docs/research/speaker-notes/inspect_pdfs.py > /tmp/notes-inspection.json
This is a research aid, not a production note detector.
"""

import hashlib
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

from pypdf import PdfReader


def structure_tags(root):
    tags = Counter()
    seen = set()

    def visit(value):
        value = value.get_object()
        if id(value) in seen:
            return
        seen.add(id(value))
        if isinstance(value, dict):
            if "/S" in value:
                tags[str(value["/S"])] += 1
            # Stay within the structure tree; do not follow page /Pg or /P links.
            if "/K" in value:
                visit(value["/K"])
        elif isinstance(value, list):
            for child in value:
                visit(child)

    if "/StructTreeRoot" in root:
        visit(root["/StructTreeRoot"])
    return dict(sorted(tags.items()))


def inspect(path):
    reader = PdfReader(path)
    root = reader.trailer["/Root"]
    # Poppler decodes the Quartz font mapping correctly in these samples;
    # pypdf 6.10.0 text extraction does not. Keep structural inspection separate.
    text = subprocess.run(
        ["pdftotext", "-layout", str(path), "-"],
        check=True, capture_output=True, text=True,
    ).stdout.split("\f")
    pages = []
    for number, page in enumerate(reader.pages, 1):
        annotations = []
        for ref in page.get("/Annots", []):
            annotation = ref.get_object()
            annotations.append({
                key: str(annotation[key])
                for key in ("/Subtype", "/Contents", "/Name", "/T", "/CA", "/Rect")
                if key in annotation
            })
        pages.append({
            "page": number,
            "size_pt": [float(page.mediabox.width), float(page.mediabox.height)],
            "visible_note_markers": sorted(set(re.findall(
                r"QP168-NOTE-(?:ALPHA|BETA)", text[number - 1]
            ))),
            "annotations": annotations,
            "metadata_stream": "/Metadata" in page,
            "associated_files": "/AF" in page,
        })
    return {
        "file": path.name,
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "pages": pages,
        "metadata": {str(k): str(v) for k, v in (reader.metadata or {}).items()},
        "catalog_metadata_stream": "/Metadata" in root,
        "catalog_associated_files": "/AF" in root,
        "attachments": list(reader.attachments),
        "structure_tags": structure_tags(root),
    }


if __name__ == "__main__":
    folder = Path(__file__).resolve().parent
    print(json.dumps([inspect(p) for p in sorted(folder.glob("*.pdf"))], indent=2))
