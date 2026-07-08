#!/usr/bin/env python3
"""Take a screenshot through xdg-desktop-portal and copy it to a target path."""

from __future__ import annotations

import argparse
import shutil
import sys
import traceback
from pathlib import Path
from urllib.parse import urlparse, unquote

import gi

gi.require_version("Gio", "2.0")
gi.require_version("GLib", "2.0")
from gi.repository import Gio, GLib  # noqa: E402


PORTAL_BUS_NAME = "org.freedesktop.portal.Desktop"
PORTAL_OBJECT_PATH = "/org/freedesktop/portal/desktop"
PORTAL_SCREENSHOT_INTERFACE = "org.freedesktop.portal.Screenshot"
PORTAL_REQUEST_INTERFACE = "org.freedesktop.portal.Request"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", help="Destination PNG path")
    parser.add_argument(
        "--interactive",
        action="store_true",
        help="Let the portal show its interactive screenshot UI",
    )
    parser.add_argument(
        "--timeout-ms",
        type=int,
        default=30000,
        help="Maximum time to wait for the portal response",
    )
    return parser.parse_args()


def uri_to_path(uri: str) -> Path:
    parsed = urlparse(uri)
    if parsed.scheme != "file":
        raise RuntimeError(f"portal returned unsupported URI: {uri}")
    return Path(unquote(parsed.path))


def unpack_variant(value: object) -> object:
    if isinstance(value, GLib.Variant):
        return value.unpack()
    return value


def take_screenshot(output: Path, interactive: bool, timeout_ms: int) -> None:
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    loop = GLib.MainLoop()
    result: dict[str, object] = {}

    options = {
        "interactive": GLib.Variant("b", interactive),
    }
    handle_variant = bus.call_sync(
        PORTAL_BUS_NAME,
        PORTAL_OBJECT_PATH,
        PORTAL_SCREENSHOT_INTERFACE,
        "Screenshot",
        GLib.Variant("(sa{sv})", ("", options)),
        GLib.VariantType.new("(o)"),
        Gio.DBusCallFlags.NONE,
        timeout_ms,
        None,
    )
    handle = handle_variant.unpack()[0]

    def on_response(
        _connection: Gio.DBusConnection,
        _sender: str,
        _object_path: str,
        _interface_name: str,
        _signal_name: str,
        parameters: GLib.Variant,
    ) -> None:
        response, response_data = parameters.unpack()
        result["response"] = response
        result["data"] = response_data
        loop.quit()

    subscription_id = bus.signal_subscribe(
        PORTAL_BUS_NAME,
        PORTAL_REQUEST_INTERFACE,
        "Response",
        handle,
        None,
        Gio.DBusSignalFlags.NONE,
        on_response,
    )

    def on_timeout() -> bool:
        result["timeout"] = True
        loop.quit()
        return GLib.SOURCE_REMOVE

    timeout_id = GLib.timeout_add(timeout_ms, on_timeout)
    loop.run()
    if not result.get("timeout"):
        GLib.source_remove(timeout_id)
    bus.signal_unsubscribe(subscription_id)

    if result.get("timeout"):
        raise TimeoutError("timed out waiting for portal screenshot response")

    response = result.get("response")
    if response != 0:
        raise RuntimeError(f"portal screenshot failed or was cancelled: response={response}")

    data = result.get("data")
    if not isinstance(data, dict) or "uri" not in data:
        raise RuntimeError(f"portal screenshot response did not include a URI: {data!r}")

    uri = unpack_variant(data["uri"])
    source = uri_to_path(str(uri))
    output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, output)


def main() -> int:
    args = parse_args()
    try:
        take_screenshot(Path(args.output), args.interactive, args.timeout_ms)
    except Exception as exc:
        print(f"portal screenshot failed: {exc}", file=sys.stderr)
        if args.timeout_ms > 0:
            traceback.print_exc(file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
