"""A disposable Unix control peer for qp startup integration tests."""
import json
import os
from pathlib import Path
import socket
import struct
import time

root = Path(os.environ["XDG_RUNTIME_DIR"])
(root / "gui.pid").write_text(str(os.getpid()))
with (root / "launches").open("a") as log:
    log.write("started\n")
mode = os.environ.get("QP_LAUNCH_MODE", "ready")
if mode == "exit":
    raise SystemExit(5)
if mode == "timeout":
    time.sleep(60)
time.sleep(0.3)
endpoint = root / "quick-presenter/control.sock"
server = socket.socket(socket.AF_UNIX)
server.bind(str(endpoint))
endpoint.chmod(0o600)
server.listen(8)
state = json.loads(Path(os.environ["QP_CONTRACT_FIXTURE"]).read_text())["responses"][0]["result"]

def read_exact(peer, count):
    data = b""
    while len(data) < count:
        part = peer.recv(count - len(data))
        if not part:
            raise EOFError()
        data += part
    return data

while True:
    peer, _ = server.accept()
    with peer:
        try:
            size = struct.unpack(">I", read_exact(peer, 4))[0]
            request = json.loads(read_exact(peer, size))
            with (root / "requests").open("a") as log:
                log.write(json.dumps(request) + "\n")
            if request["method"] == "presentation.open":
                state["document"] = request["params"]["file"]
                result = {"kind": "mutation", "changed": True, "state": state}
            else:
                result = state
            response = {"protocol_version": 1, "id": request["id"], "result": result}
            if mode == "reject_open" and request["method"] == "presentation.open":
                response.pop("result")
                response["error"] = {"code": "OPEN_FAILED", "message": "Fixture rejection"}
            data = json.dumps(response).encode()
            peer.sendall(struct.pack(">I", len(data)) + data)
        except (EOFError, BrokenPipeError):
            pass
