"""Rebuild deterministic annotation filtering fixtures using Python's stdlib.

Run: python3 tests/fixtures/generate_note_annotations.py
The existing Marp/Beamer PDFs remain the real-generator compatibility fixtures.
These small PDFs exercise generic comments and mixed annotation dictionaries.
"""

from pathlib import Path


def annotation(contents, fields="", subtype="Text", rect="30 30 50 50"):
    text = (b"\xfe\xff" + contents.encode("utf-16-be")).hex()
    return (
        f"<< /Type /Annot /Subtype /{subtype} /Rect [{rect}] "
        f"/Contents <{text}> {fields} >>"
    )


MARP_FIELDS = "/Name /Note /Subj (Note) /C [1 0.92 0.42] /CA 0.25"
MARP_RECT = "0 20 20 20"
GENERIC = [
    annotation("Ordinary Comment icon", "/Name /Comment"),
    annotation("Ordinary Note icon", "/Name /Note"),
    annotation("Comment with no icon or identifier"),
    annotation("Identifier is not an icon", "/Name /Comment /NM (Note)"),
    annotation("Opaque comment by Quick Presenter", "/Name /Note /T (Quick Presenter) /C [0 0 1] /CA 1"),
    annotation("Transparent comment by a reviewer", "/Name /Note /T (Reviewer) /C [0 0 1] /CA 0"),
    annotation("Marp color at an ordinary position", MARP_FIELDS),
    annotation("Marp position with another color", "/Name /Note /C [1 1 0] /CA 0.25", rect=MARP_RECT),
    annotation("Marp position and color without opacity", "/Name /Note /C [1 0.92 0.42]", rect=MARP_RECT),
    annotation("FreeText is not speaker notes", MARP_FIELDS, subtype="FreeText", rect=MARP_RECT),
    annotation("   \n", MARP_FIELDS, rect=MARP_RECT),
]


def write_pdf(path, pages):
    objects = []

    def add(value):
        objects.append(value)
        return len(objects)

    add("<< /Type /Catalog /Pages 2 0 R >>")
    add("")
    page_refs = []
    for annotations in pages:
        refs = [f"{add(value)} 0 R" for value in annotations]
        page_refs.append(f"{add('<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Annots [' + ' '.join(refs) + '] >>')} 0 R")
    objects[1] = f"<< /Type /Pages /Kids [{' '.join(page_refs)}] /Count {len(pages)} >>"

    data = bytearray(b"%PDF-1.7\n")
    offsets = [0]
    for i, value in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(f"{i} 0 obj\n{value}\nendobj\n".encode("ascii"))
    startxref = len(data)
    data.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
    for offset in offsets[1:]:
        data.extend(f"{offset:010} 00000 n \n".encode())
    data.extend(f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{startxref}\n%%EOF\n".encode())
    path.write_bytes(data)


if __name__ == "__main__":
    root = Path(__file__).resolve().parent
    write_pdf(root / "generic-comments.pdf", [GENERIC])
    write_pdf(root / "mixed-speaker-notes.pdf", [
        [*GENERIC,
         annotation("Marp note in a commented PDF", MARP_FIELDS + " /NM (unique-note-id)", rect=MARP_RECT),
         annotation("日本語のノート", MARP_FIELDS, rect=MARP_RECT)],
        [*GENERIC,
         annotation("Beamer note in a commented PDF", "/Name /Note /T (Quick Presenter) /C [0 0 1] /CA 0", rect="142.226 125.624 155.776 139.173")],
    ])
