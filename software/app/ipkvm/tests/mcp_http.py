"""Exercise every MCP tool against a separate kvmd --sim process using real HTTP."""

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
from http.cookiejar import CookieJar
import itertools
import json
from pathlib import Path
import socket
import subprocess
from tempfile import TemporaryDirectory
import time
from urllib.error import HTTPError
from urllib.request import HTTPCookieProcessor, Request, build_opener


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


class Client:
    def __init__(self, port):
        self.base = f"http://127.0.0.1:{port}"
        self.ids = itertools.count(1)
        self.opener = build_opener(HTTPCookieProcessor(CookieJar()))
        self.exercised = set()

    def request(self, method, path, body=None, *, key=None, status=200, headers=None):
        headers = {**(headers or {})}
        if body is not None:
            headers["Content-Type"] = "application/json"
        if key:
            headers["Authorization"] = "Bearer " + key
        data = json.dumps(body).encode() if body is not None else None
        request = Request(self.base + path, data=data, headers=headers, method=method)
        try:
            response = self.opener.open(request, timeout=20)
        except HTTPError as error:
            response = error
        with response:
            raw = response.read()
            assert response.code == status, (method, path, response.code, raw[:500])
            assert "mcp-session-id" not in response.headers
            if not raw:
                return None
            return json.loads(raw) if response.headers.get_content_type() == "application/json" else raw

    def rpc(self, key, method, params=None, *, error=None, headers=None):
        request_id = next(self.ids)
        req = {"jsonrpc": "2.0", "id": request_id, "method": method}
        if params is not None:
            req["params"] = params
        result = self.request("POST", "/api/mcp/v1", req, key=key, headers={
            "Accept": "application/json, text/event-stream",
            "MCP-Protocol-Version": "2025-11-25",
            **(headers or {}),
        })
        assert result.get("id") == request_id and result.get("jsonrpc") == "2.0", result
        if error is not None:
            assert result.get("error", {}).get("code") == error, result
            return None
        assert "result" in result, (method, result)
        return result["result"]

    def tool(self, key, name, args=None, *, error=None):
        reply = self.rpc(key, "tools/call", {"name": name, "arguments": args or {}})
        fields = reply.get("structuredContent", {})
        if error:
            assert reply.get("isError") and fields.get("error") == error, (name, reply)
        else:
            assert not reply.get("isError"), (name, reply)
        self.exercised.add(name)
        return reply, fields

    def key(self, name, scopes):
        key = self.request("POST", "/api/mcp/keys", {"name": name, "scopes": scopes}, status=201)["key"]
        assert key.startswith("kvp_")
        return key


def has_jpeg(reply):
    images = [item for item in reply["content"] if item["type"] == "image"]
    return bool(images) and all(
        item["mimeType"] == "image/jpeg" and base64.b64decode(item["data"]).startswith(b"\xff\xd8")
        for item in images
    )


def check(client):
    client.request("POST", "/api/auth/login", {"username": "admin", "password": "admin"})
    client.request("POST", "/api/users/me/password", {"old": "admin", "new": "sim-local-only"}, status=204)
    observe = client.key("observer", ["observe"])
    all_scopes = client.key("integration", ["observe", "input", "power", "media", "host_private"])
    client.request("PUT", "/api/mcp", {"enabled": True})
    client.request("GET", "/api/mcp/v1", key=all_scopes, status=405)
    client.request("POST", "/api/mcp/v1", {"jsonrpc": "2.0", "method": "ping", "id": 1},
                   key=all_scopes, headers={"Origin": "http://rebind.invalid"}, status=403)
    client.request("POST", "/api/mcp/v1", {"jsonrpc": "2.0", "method": "ping", "id": 1},
                   key=all_scopes, headers={"Host": "rebind.invalid"}, status=403)
    client.request("POST", "/api/mcp/v1", {"jsonrpc": "2.0", "method": "ping", "id": 1},
                   key="invalid", status=401)

    for requested, expected in (("2025-06-18", "2025-06-18"), ("2025-11-25", "2025-11-25"), ("1900-01-01", "2025-11-25")):
        init = client.rpc(all_scopes, "initialize", {
            "protocolVersion": requested, "capabilities": {},
            "clientInfo": {"name": "kvmpanel-http-smoke", "version": "1"},
        })
        assert init["protocolVersion"] == expected and init["serverInfo"]["name"] == "kvmpanel", init
    client.request("POST", "/api/mcp/v1", {"jsonrpc": "2.0", "method": "notifications/initialized"},
                   key=all_scopes, status=202)
    client.rpc(all_scopes, "ping")

    schema_path = Path(__file__).resolve().parents[2] / "api/schema/mcp.schema.json"
    contract = json.loads(schema_path.read_text(encoding="utf-8"))["tools"]
    published = {tool["name"]: tool for tool in client.rpc(all_scopes, "tools/list")["tools"]}
    assert set(published) == set(contract), published.keys()
    for name, tool in published.items():
        for schema in ("inputSchema", "outputSchema"):
            assert tool[schema] == contract[name][schema], (name, schema)
    visible = {tool["name"] for tool in client.rpc(observe, "tools/list")["tools"]}
    assert visible and visible < set(published) and "screenshot" in visible and "power" not in visible
    client.tool(observe, "power", {"action": "press"}, error="forbidden_scope")
    client.rpc(all_scopes, "tools/call", {"name": "boot_watch", "arguments": {"start": "none", "retries": 1}}, error=-32602)

    client.tool(all_scopes, "host_status")
    screenshot, fields = client.tool(all_scopes, "screenshot", {
        "region": {"x": 0.1, "y": 0.1, "w": 0.8, "h": 0.8}, "max_width": 320, "quality": 50,
    })
    assert fields["width"] <= 320 and has_jpeg(screenshot), fields
    waited, fields = client.tool(all_scopes, "screen_wait", {"until": "stable", "stable_ms": 100,
                                                               "timeout_ms": 2500, "threshold": 1})
    assert fields["met"] and has_jpeg(waited), fields
    client.tool(all_scopes, "telemetry_read", {"groups": ["cpu"]})
    client.tool(all_scopes, "telemetry_watch", {"seconds": 2, "groups": ["cpu"]})
    client.tool(all_scopes, "telemetry_catalogue")
    client.tool(all_scopes, "host_info")
    history, _ = client.tool(all_scopes, "screen_history", {"seconds": 10, "max_frames": 3})
    assert has_jpeg(history)
    client.tool(all_scopes, "host_clipboard")

    client.tool(all_scopes, "type_text", {"text": "KVM smoke"})
    client.tool(all_scopes, "press_keys", {"keys": ["ControlLeft", "KeyA"], "hold_ms": 10})
    client.tool(all_scopes, "move_mouse", {"mode": "relative", "dx": 255, "dy": -256})
    client.tool(all_scopes, "click_mouse", {"button": "left", "x": 0.25, "y": 0.25})
    client.tool(all_scopes, "drag_mouse", {"from": {"x": 0.2, "y": 0.2}, "to": {"x": 0.4, "y": 0.5},
                                         "button": "left", "steps": 3, "duration_ms": 50})
    client.tool(all_scopes, "scroll_mouse", {"dy": 2})
    client.tool(all_scopes, "wake_on_lan", {"mac": "02:00:00:00:00:01"})
    _, fields = client.tool(all_scopes, "boot_watch", {"start": "none", "until": "os", "timeout_s": 4})
    assert fields["result"] == "ok", fields
    boot = {"start": "reset", "until": "os", "timeout_s": 5}
    _, confirmation = client.tool(all_scopes, "boot_watch", boot)
    _, fields = client.tool(all_scopes, "boot_watch", {**boot, "confirm_token": confirmation["confirm_token"]})
    assert fields["result"] == "ok" and len(fields["rounds"]) == 1, fields
    timeline = fields["rounds"][0]
    assert timeline["signal_ms"] is not None and (timeline["telemetry_ms"] or timeline["host_agent_ms"]) >= 300, timeline
    client.request("PATCH", "/api/config", {"host": {"tap_duration_s": 3}})
    action = {"target": "setup", "start": "reset"}
    _, fields = client.tool(all_scopes, "enter_firmware", action)
    assert fields["needs_confirm"] and fields["expires_in"] == 30
    entered, _ = client.tool(all_scopes, "enter_firmware", {**action, "confirm_token": fields["confirm_token"]})
    assert has_jpeg(entered)
    _, fields = client.tool(all_scopes, "power", {"action": "reset"})
    token = fields["confirm_token"]
    client.tool(all_scopes, "power", {"action": "force_off", "confirm_token": token}, error="invalid_confirmation")
    client.tool(all_scopes, "power", {"action": "reset", "confirm_token": token})
    client.tool(all_scopes, "power", {"action": "reset", "confirm_token": token}, error="invalid_confirmation")

    for _ in range(25):
        _, fields = client.tool(all_scopes, "host_status")
        if fields["host_agent"]["online"]:
            break
        time.sleep(0.1)
    else:
        raise AssertionError("simulated host did not reconnect after reset")
    client.tool(all_scopes, "virtual_media", {"action": "list"})
    client.tool(all_scopes, "virtual_media", {"action": "mount", "path": "/backup/pc-2026-10.img", "mode": "disk", "readonly": False})
    _, fields = client.tool(all_scopes, "virtual_media", {"action": "eject"})
    assert fields["needs_confirm"]
    client.tool(all_scopes, "virtual_media", {"action": "eject", "confirm_token": fields["confirm_token"]})
    client.tool(all_scopes, "host_drop_file", {"name": "smoke.txt", "content": "S1ZN", "encoding": "base64", "export": True})
    client.tool(all_scopes, "share", {"action": "status"})
    client.tool(all_scopes, "share", {"action": "reclaim"})

    assert client.rpc(observe, "resources/list")["resources"]
    client.rpc(observe, "resources/read", {"uri": "kvmpanel://host/status"})
    assert client.rpc(all_scopes, "prompts/list")["prompts"]
    client.rpc(all_scopes, "prompts/get", {"name": "boot_failure_triage"})
    with ThreadPoolExecutor(max_workers=1) as pool:
        waiting = pool.submit(client.tool, all_scopes, "screen_wait",
                              {"until": "changed", "threshold": 1, "timeout_ms": 10000}, error="stopped")
        time.sleep(0.4)
        client.request("POST", "/api/mcp/stop", status=204)
        waiting.result(timeout=4)
        client.request("PUT", "/api/mcp", {"enabled": True})
        _, confirm = client.tool(all_scopes, "power", {"action": "force_off"})
        holding = pool.submit(client.tool, all_scopes, "power",
                              {"action": "force_off", "confirm_token": confirm["confirm_token"]}, error="stopped")
        time.sleep(0.4)
        client.request("POST", "/api/mcp/stop", status=204)
        holding.result(timeout=4)
    state = client.request("GET", "/api/mcp")
    assert state["enabled"] is False
    client.request("POST", "/api/mcp/v1", {"jsonrpc": "2.0", "method": "ping", "id": 1}, key=all_scopes, status=403)
    client.request("PUT", "/api/mcp", {"enabled": True})
    _, fields = client.tool(all_scopes, "host_status")
    assert fields["power"]["state"] == "on", fields
    audit = client.request("GET", "/api/mcp/audit?limit=100")["entries"]
    serialized_audit = json.dumps(audit)
    assert audit and all(secret not in serialized_audit for secret in ("KVM smoke", "S1ZN", token))
    assert client.exercised == set(published), set(published) - client.exercised
    print(f"HTTP passed for {len(published)} MCP tools, both protocol versions, scope filtering, confirmation, and emergency stop")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kvmd", type=Path, help="path to a locally built kvmd executable")
    options = parser.parse_args()
    port, panel_port = free_port(), free_port()
    while port == panel_port:
        panel_port = free_port()
    with TemporaryDirectory(prefix="kvmd-mcp-") as data:
        log_path = Path(data) / "kvmd.log"
        with log_path.open("wb") as log, subprocess.Popen([
            str(options.kvmd.resolve()), "--sim", "--listen", f"127.0.0.1:{port}",
            "--panel", f"127.0.0.1:{panel_port}", "--data-dir", data,
        ], stdout=log, stderr=subprocess.STDOUT) as server:
            client = Client(port)
            try:
                for _ in range(100):
                    try:
                        client.request("GET", "/api/health")
                        break
                    except OSError:
                        if server.poll() is not None:
                            raise RuntimeError(log_path.read_text()) from None
                        time.sleep(0.05)
                else:
                    raise RuntimeError("kvmd never became ready")
                check(client)
            finally:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()


if __name__ == "__main__":
    main()
