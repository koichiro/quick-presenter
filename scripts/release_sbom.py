#!/usr/bin/env python3
"""Generate and verify artifact-bound CycloneDX 1.5 release sidecars."""
from __future__ import annotations

import argparse
import base64
import copy
from contextlib import contextmanager
import hashlib
import json
import plistlib
from pathlib import Path, PurePosixPath
import subprocess
import tempfile
import tomllib
import urllib.request
import uuid
import zipfile
import xml.etree.ElementTree as ET

REPO = Path(__file__).resolve().parent.parent
SOURCE_URL = "https://github.com/koichiro/quick-presenter"
SCHEMA_LOCK = Path(__file__).with_name("sbom_schemas.json")
TARGET_ASSETS = {
    "aarch64-apple-darwin": "pdfium-mac-arm64.tgz",
    "x86_64-apple-darwin": "pdfium-mac-x64.tgz",
    "x86_64-unknown-linux-gnu": "pdfium-linux-x64.tgz",
    "x86_64-pc-windows-msvc": "pdfium-win-x64.tgz",
}


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def hashes(path: Path) -> list[dict]:
    return [{"alg": "SHA-256", "content": digest(path)}]


def properties(**values: str) -> list[dict]:
    return [{"name": f"quick-presenter:{key.replace('_', '-')}", "value": value}
            for key, value in values.items()]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


@contextmanager
def unpack(artifact: Path):
    """Inspect the actual package, rather than an adjacent staging directory."""
    with tempfile.TemporaryDirectory(prefix="quick-presenter-sbom-") as directory:
        payload = Path(directory) / "payload"
        payload.mkdir()
        if artifact.suffix == ".dmg":
            subprocess.run(["hdiutil", "attach", str(artifact), "-readonly", "-nobrowse",
                            "-mountpoint", str(payload)], check=True)
            try:
                app = payload / "Quick Presenter.app"
                require(app.is_dir(), "DMG does not contain Quick Presenter.app")
                yield app
            finally:
                subprocess.run(["hdiutil", "detach", str(payload)], check=True)
        elif artifact.suffix == ".deb":
            subprocess.run(["dpkg-deb", "-x", str(artifact), str(payload)], check=True)
            actual_version = subprocess.check_output(["dpkg-deb", "--field", str(artifact), "Version"], text=True).strip()
            expected_version = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
            require(actual_version == expected_version, "Debian package version differs from Cargo")
            yield payload
        elif artifact.suffix == ".msix":
            with zipfile.ZipFile(artifact) as archive:
                for entry in archive.infolist():
                    path = PurePosixPath(entry.filename.replace("\\", "/"))
                    require(not path.is_absolute() and ".." not in path.parts
                            and ":" not in entry.filename,
                            "Unsafe MSIX archive path")
                archive.extractall(payload)
            yield payload
        else:
            raise ValueError("Supported SBOM packages are .dmg, .deb, and .msix")


def make_bom(cargo: dict, artifact: Path, payload: Path, target: str,
             revision: str, repo: Path = REPO) -> dict:
    require(target in TARGET_ASSETS, f"Unsupported release target: {target}")
    require(cargo.get("bomFormat") == "CycloneDX" and cargo.get("specVersion") == "1.5",
            "Cargo input must be CycloneDX 1.5")
    target_properties = cargo.get("metadata", {}).get("properties", [])
    require({"name": "cdx:rustc:sbom:target:triple", "value": target} in target_properties,
            "Cargo SBOM target does not match the package target")
    require(len(revision) == 40 and all(c in "0123456789abcdef" for c in revision),
            "Source revision must be a full Git commit SHA")
    package = tomllib.loads((repo / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    root = copy.deepcopy(cargo["metadata"]["component"])
    require(root["name"] == package["name"] and root["version"] == package["version"],
            "Cargo SBOM does not match the application version")
    old_root = root["bom-ref"]
    root.pop("components", None)
    root["bom-ref"] = "quick-presenter:application"
    root["purl"] = f"pkg:github/koichiro/quick-presenter@{revision}"
    root["hashes"] = hashes(artifact)
    root["externalReferences"] = [
        {"type": "vcs", "url": f"{SOURCE_URL}/tree/{revision}"},
        {"type": "distribution", "url": f"urn:quick-presenter:artifact:{artifact.name}",
         "hashes": hashes(artifact)},
    ]
    identity = {}
    if artifact.suffix == ".msix":
        manifest_path = payload / "AppxManifest.xml"
        node = ET.parse(manifest_path).getroot().find("{http://schemas.microsoft.com/appx/manifest/foundation/windows10}Identity")
        require(node is not None, "MSIX is missing package identity")
        require(node.attrib["Version"] == package["version"] + ".0", "MSIX package version differs from Cargo")
        require(node.attrib["ProcessorArchitecture"] == "x64", "MSIX architecture differs from target")
        identity = {"package_identity": node.attrib["Name"], "package_publisher": node.attrib["Publisher"],
                    "package_version": node.attrib["Version"], "package_scope": "submission input; Store may re-sign this package"}
    elif artifact.suffix == ".dmg":
        with (payload / "Contents/Info.plist").open("rb") as stream:
            info = plistlib.load(stream)
        require(info["CFBundleShortVersionString"] == package["version"], "App bundle version differs from Cargo")
        identity = {"package_identity": info["CFBundleIdentifier"]}
    root["properties"] = properties(artifact=artifact.name, target=target,
                                    source_revision=revision,
                                    cargo_lock_sha256=digest(repo / "Cargo.lock"), **identity)
    bom = copy.deepcopy(cargo)
    bom["metadata"]["component"] = root
    components = bom.setdefault("components", [])
    dependencies = bom.setdefault("dependencies", [])
    ref_map = {old_root: root["bom-ref"]}
    # Local path dependencies are modified source, not pristine registry releases.
    for component in components:
        ref = component["bom-ref"]
        if ref.startswith("path+file:"):
            local = repo / "vendor" / component["name"]
            require(local.is_dir(), f"Unrecognized local dependency: {component['name']}")
            stable_ref = f"quick-presenter:vendored:{component['name']}@{revision}"
            ref_map[ref] = stable_ref
            component["bom-ref"] = stable_ref
            component.pop("purl", None)
            component.setdefault("properties", []).extend(properties(
                modified="true", source_revision=revision,
                source_path=local.relative_to(repo).as_posix()))
            component.setdefault("externalReferences", []).append({
                "type": "vcs", "url": f"{SOURCE_URL}/tree/{revision}/vendor/{component['name']}"})
    for dependency in dependencies:
        dependency["ref"] = ref_map.get(dependency["ref"], dependency["ref"])
        dependency["dependsOn"] = [ref_map.get(ref, ref) for ref in dependency.get("dependsOn", [])]

    files = []
    for path in sorted(payload.rglob("*")):
        relative = path.relative_to(payload).as_posix()
        if path.is_symlink():
            files.append({"type": "file", "bom-ref": f"payload:{relative}", "name": relative,
                          "properties": properties(symlink_target=path.readlink().as_posix())})
        elif path.is_file():
            require(path.stat().st_size > 0 or path.suffix not in {".exe", ".dll", ".dylib", ".so"},
                    f"Empty native binary: {relative}")
            files.append({"type": "file", "bom-ref": f"payload:{relative}",
                          "name": relative, "hashes": hashes(path)})
    require(files, "Package payload is empty")
    require(any(Path(file["name"]).name in {"quick-presenter", "quick-presenter.exe"}
                and "hashes" in file and (payload / file["name"]).stat().st_size > 0
                for file in files), "Missing application executable")
    components.extend(files)
    by_name = {file["name"]: file for file in files}
    for required in ("QuickPresenter-LICENSE.txt", "QuickPresenter-SOURCE-OFFER.txt", "PDFium-LICENSE.txt"):
        require(any(Path(name).name == required and (payload / name).stat().st_size > 0
                    for name in by_name), f"Missing or empty notice: {required}")
    pdfium_dirs = sorted(path for path in payload.rglob("pdfium") if path.is_dir())
    require(pdfium_dirs, "Missing bundled PDFium")
    manifest = json.loads((repo / "scripts/pdfium_manifest.json").read_text(encoding="utf-8"))
    asset = TARGET_ASSETS[target]
    pdfium_ref = "quick-presenter:pdfium"
    pdfium_dependencies = []
    version = None
    declared_licenses = {}
    main_license = None
    for directory in pdfium_dirs:
        version_file = directory / "VERSION"
        require(version_file.is_file(), "Missing PDFium VERSION")
        fields = dict(line.split("=", 1) for line in version_file.read_text(encoding="utf-8").splitlines() if "=" in line)
        current = ".".join(fields[key] for key in ("MAJOR", "MINOR", "BUILD", "PATCH"))
        require(manifest["version"] == f"chromium/{fields['BUILD']}", "PDFium version differs from the pin")
        require(version is None or version == current, "Bundled PDFium copies have different versions")
        version = current
        require((directory / "LICENSE").is_file(), "Missing PDFium LICENSE")
        text = (directory / "LICENSE").read_bytes()
        require(bool(text.strip()), "Empty PDFium LICENSE")
        require(main_license is None or main_license == text, "PDFium LICENSE copies differ")
        main_license = text
        require((directory / "licenses").is_dir(), "Missing PDFium component licenses")
        binary_names = {"pdfium-mac-arm64.tgz": "libpdfium.dylib", "pdfium-mac-x64.tgz": "libpdfium.dylib",
                        "pdfium-linux-x64.tgz": "libpdfium.so", "pdfium-win-x64.tgz": "pdfium.dll"}
        require(any(path.name == binary_names[asset] and path.stat().st_size > 0
                    for path in directory.rglob("*") if path.is_file()), "Missing PDFium native library")
        for path in directory.rglob("*"):
            if path.is_file() and not path.is_symlink():
                pdfium_dependencies.append(f"payload:{path.relative_to(payload).as_posix()}")
        licenses = sorted(path for path in (directory / "licenses").iterdir() if path.is_file())
        require(licenses, "PDFium component license inventory is empty")
        for path in licenses:
            require(path.stat().st_size > 0, f"Empty PDFium license: {path.name}")
            if path.name in declared_licenses:
                require(declared_licenses[path.name] == path.read_bytes(), "PDFium license copies differ")
            declared_licenses[path.name] = path.read_bytes()
    components.append({
        "type": "library", "bom-ref": pdfium_ref, "name": "PDFium", "version": version,
        "supplier": {"name": "bblanchon/pdfium-binaries"},
        "licenses": [{"license": {"name": "PDFium bundled LICENSE", "text": {
            "contentType": "text/plain", "encoding": "base64",
            "content": base64.b64encode(main_license).decode("ascii")}}}],
        "externalReferences": [{"type": "distribution", "url": f"{manifest['base_url']}/{asset}",
                                "hashes": [{"alg": "SHA-256", "content": manifest["assets"][asset]["sha256"]}]}],
        "properties": properties(upstream_release=manifest["version"],
                                  native_inventory="incomplete: internal component versions are not supplied"),
    })
    # License evidence is recorded without inventing versions or claiming binary detection.
    for name, text in sorted(declared_licenses.items()):
        license_ref = f"quick-presenter:pdfium-license:{name}"
        components.append({"type": "file", "bom-ref": license_ref, "name": f"PDFium license evidence: {name}",
                           "licenses": [{"license": {"name": name, "text": {
                               "contentType": "text/plain", "encoding": "base64",
                               "content": base64.b64encode(text).decode("ascii")}}}],
                           "properties": properties(evidence="upstream distributed license file; not a version inventory")})
        pdfium_dependencies.append(license_ref)
    dependencies.append({"ref": pdfium_ref, "dependsOn": sorted(set(pdfium_dependencies))})
    root_dependency = next((item for item in dependencies if item["ref"] == root["bom-ref"]), None)
    require(root_dependency is not None, "Cargo SBOM is missing application dependency relationships")
    root_dependency["dependsOn"] = sorted(set(root_dependency["dependsOn"] + [pdfium_ref] + [file["bom-ref"] for file in files]))
    bom["compositions"] = [{"aggregate": "incomplete", "assemblies": [root["bom-ref"], pdfium_ref]}]
    bom["metadata"].setdefault("tools", []).append({"name": "quick-presenter-release-sbom", "version": "1"})
    bom["metadata"].setdefault("properties", []).extend(properties(
        inventory_scope="target-specific Cargo normal dependency graph plus extracted payload; not proof of linked crate contents",
        native_limitations="PDFium internal versions, Rust standard library and other statically linked native dependencies are not fully inventoried"))
    bom["serialNumber"] = f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, json.dumps(bom, sort_keys=True))}"
    validate_refs(bom)
    return bom


def validate_refs(bom: dict) -> None:
    refs = [bom["metadata"]["component"]["bom-ref"]] + [c["bom-ref"] for c in bom["components"]]
    require(len(refs) == len(set(refs)), "Duplicate component references")
    for dependency in bom["dependencies"]:
        require(dependency["ref"] in refs and all(ref in refs for ref in dependency.get("dependsOn", [])),
                "Dependency relationship refers to a missing component")


def validate_schema(bom: dict, cache: Path) -> None:
    from jsonschema import Draft7Validator
    from referencing import Registry, Resource
    lock = json.loads(SCHEMA_LOCK.read_text(encoding="utf-8"))
    cache.mkdir(parents=True, exist_ok=True)
    schemas = {}
    for name, entry in lock.items():
        path = cache / name
        if not path.is_file():
            with urllib.request.urlopen(entry["url"], timeout=30) as response:
                data = response.read()
            require(hashlib.sha256(data).hexdigest() == entry["sha256"], f"Schema checksum mismatch: {name}")
            path.write_bytes(data)
        require(digest(path) == entry["sha256"], f"Schema checksum mismatch: {name}")
        schemas[name] = json.loads(path.read_text(encoding="utf-8"))
    registry = Registry()
    for name, schema in schemas.items():
        resource = Resource.from_contents(schema)
        registry = registry.with_resource(f"http://cyclonedx.org/schema/{name}", resource)
    Draft7Validator(schemas["bom-1.5.schema.json"], registry=registry).validate(bom)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo-bom", required=True, type=Path)
    parser.add_argument("--artifact", required=True, type=Path)
    parser.add_argument("--target", required=True, choices=TARGET_ASSETS)
    parser.add_argument("--schema-cache", required=True, type=Path)
    parser.add_argument("--verify", action="store_true", help="Verify an existing sidecar without rewriting it")
    args = parser.parse_args()
    artifact = args.artifact.resolve()
    require(artifact.is_file() and artifact.stat().st_size > 0, "Missing or empty package")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()
    cargo = json.loads(args.cargo_bom.read_text(encoding="utf-8"))
    output = artifact.with_name(artifact.name + ".cdx.json")
    with unpack(artifact) as payload:
        expected = make_bom(cargo, artifact, payload, args.target, revision)
    if args.verify:
        actual = json.loads(output.read_text(encoding="utf-8"))
        validate_schema(actual, args.schema_cache)
        require(actual == expected, "SBOM differs from the source inputs or final package; regenerate it")
    else:
        validate_schema(expected, args.schema_cache)
        output.write_text(json.dumps(expected, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")
    print(f"{'Verified' if args.verify else 'Generated'} {output}")


if __name__ == "__main__":
    main()
