"""Drive kvmd --sim through a writable GPADC file and inspect panel and MCP replies."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import itertools
import json
import os
from pathlib import Path
import socket
import subprocess
from tempfile import TemporaryDirectory
import time

from mcp_http import Client, free_port


LEVELS_MV = {
    "Idle": 1649, "Plus": 1408, "Minus": 1148, "Both": 911, "Func": 681,
    "PlusFunc": 456, "MinusFunc": 221, "Invalid": 0,
}


class Panel:
    def __init__(self, sock):
        self.sock = sock
        self.stream = sock.makefile("rb")
        self.ids = itertools.count(1)

    def until(self, predicate):
        try:
            while line := self.stream.readline():
                message = json.loads(line)
                if predicate(message):
                    return message
        except (TimeoutError, OSError) as error:
            raise AssertionError("expected panel message did not arrive") from error
        raise AssertionError("panel disconnected")

    def request(self, kind, **fields):
        request_id = next(self.ids)
        message = {"t": kind, "id": request_id, **fields}
        self.sock.sendall(json.dumps(message).encode() + b"\n")
        result = self.until(lambda m: m["t"] == "result" and m["id"] == request_id)
        assert result["ok"], result
        return result.get("data")


def eventually(check, *, timeout=4):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if result := check():
            return result
        time.sleep(0.01)
    raise AssertionError("expected simulator state did not arrive")


def listening(port):
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=0.2):
            return True
    except ConnectionRefusedError:
        return False


def simulate(kvmd, gain, offset, *, full):
    with TemporaryDirectory(prefix="kvmd-keys-") as data:
        base = Path(data)
        raw_path = base / "in_voltage0_raw"
        log_path = base / "kvmd.log"
        panel_path = base / "panel.sock"
        port = free_port()

        def raw(state):
            if state == "Clipped":
                return 4095
            mv = LEVELS_MV[state] * gain + offset
            return round(max(0, mv) * 4096 / 1800)

        def sample(state):
            tmp = raw_path.with_suffix(".new")
            tmp.write_text(f"{raw(state)}\n")
            tmp.replace(raw_path)

        def log_after(start=0):
            return log_path.read_text()[start:]

        def set_state(state, *, previous=None, stimulus=None):
            start = len(log_after())
            sample(stimulus or state)
            def confirmed():
                lines = log_after(start)
                return lines if f"state={state}\n" in lines else None

            window = eventually(confirmed)
            if previous is not None:
                # With tracing enabled the first matching raw sample must
                # still report the previous confirmed state.
                readings = [line for line in window.splitlines() if f"raw={raw(stimulus or state)} " in line]
                assert len(readings) >= 2, readings
                assert readings[0].endswith(f"state={previous}"), readings
                assert readings[1].endswith(f"state={state}"), readings

        def press_short(release="Idle"):
            set_state("Func")
            time.sleep(0.08)
            set_state("Idle", stimulus=release)

        sample("Plus" if full else "Func")
        env = {**os.environ, "KVMD_KEY_ADC": str(raw_path), "KVMD_KEY_ADC_TRACE": "0" if full else "1"}
        with log_path.open("wb") as log, subprocess.Popen([
            str(kvmd), "--sim", "--listen", f"127.0.0.1:{port}",
            "--panel", str(panel_path), "--data-dir", data,
        ], env=env, stdout=log,
            stderr=subprocess.STDOUT) as server, socket.socket(socket.AF_UNIX) as sock:
            try:
                eventually(lambda: panel_path.exists())
                sock.settimeout(4)
                sock.connect(str(panel_path))
                panel = Panel(sock)
                panel.until(lambda m: m["t"] == "hello")
                if full:
                    plus = raw("Plus")
                    eventually(lambda: f"raw={plus} baseline={plus:.1f} state=Idle" in log_after())
                else:
                    eventually(lambda: f"raw={raw('Func')} baseline=0.0 state=Invalid" in log_after())
                sample("Idle")
                idle = raw("Idle")
                eventually(lambda: f"raw={idle} baseline={idle:.1f} state=Idle" in log_after())

                previous = "Idle"
                for state in ("Plus", "Minus", "Both", "Func", "PlusFunc", "MinusFunc", "Invalid", "Idle"):
                    set_state(state, previous=None if full else previous)
                    previous = state
                assert "FUNC Short" not in log_after() and "FUNC Long" not in log_after()
                if not full:
                    return

                press_short("Clipped")
                assert f"raw=4095 baseline={idle:.1f} state=Idle" in log_after()
                assert panel.until(lambda m: m["t"] == "function_key") == {
                    "t": "function_key", "action": "toggle_settings", "display_asleep": False,
                }
                set_state("Func")
                msg = panel.until(lambda m: m["t"] == "function_key")
                assert msg == {"t": "function_key", "action": "toggle_screen", "display_asleep": True}, msg
                set_state("Idle")
                panel.until(lambda m: m["t"] == "board" and m["display_asleep"])
                panel.request("action", action="wake_display")
                panel.until(lambda m: m["t"] == "board" and not m["display_asleep"])
                assert log_after().count("FUNC Short") == log_after().count("FUNC Long") == 1

                panel.request("patch_config", patch={"function_key": {"short": "cycle_brightness"}})
                press_short()
                msg = panel.until(lambda m: m["t"] == "function_key")
                assert msg["action"] == "cycle_brightness", msg
                settings = panel.request("get_config")["panel"]
                assert settings["auto_brightness"] is False and settings["brightness"] == 20, settings

                panel.request("patch_config", patch={"function_key": {"short": "stop_mcp"}})
                client = Client(port)
                client.request("POST", "/api/auth/login", {"username": "admin", "password": "admin"})
                client.request("POST", "/api/users/me/password", {"old": "admin", "new": "sim-local-only"}, status=204)
                key = client.key("case-keys", ["input", "observe"])
                client.request("PUT", "/api/mcp", {"enabled": True})
                with ThreadPoolExecutor(max_workers=1) as pool:
                    holding = pool.submit(client.tool, key, "press_keys",
                                          {"keys": ["KeyA"], "hold_ms": 2000}, error="stopped")
                    panel.until(lambda m: m["t"] == "mcp" and m["active"] and m["tool"] == "press_keys")
                    press_short()
                    holding.result(timeout=4)
                eventually(lambda: client.request("GET", "/api/mcp")["enabled"] is False)

                panel.request("patch_config", patch={"function_key": {"short": "pause_kvm"}})
                press_short()
                panel.until(lambda m: m["t"] == "config" and m["config"]["kvm"]["enabled"] is False)
                assert panel.request("get_config")["kvm"]["enabled"] is False
                eventually(lambda: not listening(port))
                restored = panel.request("patch_config", patch={"kvm": {"enabled": True}})
                assert restored["kvm"]["enabled"] is True
                eventually(lambda: listening(port))
            except Exception:
                print(log_after())
                raise
            finally:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kvmd", type=Path)
    args = parser.parse_args()
    simulate(args.kvmd.resolve(), 1.05, 10, full=True)
    simulate(args.kvmd.resolve(), 0.95, -10, full=False)
    print("ADC simulation passed: seven states, invalid input, calibration recovery, gain and offset, clipping, two-sample confirmation, FUNC actions and MCP stop")


if __name__ == "__main__":
    main()
