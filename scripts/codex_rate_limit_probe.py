#!/usr/bin/env python3
import json
import subprocess
import sys
import time


def main() -> int:
    process = subprocess.Popen(
        ["codex", "app-server", "--listen", "stdio://"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        requests = [
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": 2,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "ceodex-rate-limit-probe",
                        "version": "0.1",
                    },
                },
            },
            {
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {},
            },
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "account/rateLimits/read",
                "params": {},
            },
        ]
        assert process.stdin is not None
        for message in requests:
            process.stdin.write(json.dumps(message) + "\n")
        process.stdin.flush()

        response = None
        deadline = time.time() + 10
        assert process.stdout is not None
        while time.time() < deadline:
            line = process.stdout.readline()
            if not line:
                break
            payload = json.loads(line)
            if payload.get("id") != 2:
                continue
            response = payload
            break
        if response is None:
            stderr = process.stderr.read() if process.stderr is not None else ""
            print(stderr.strip() or "No account/rateLimits/read response received", file=sys.stderr)
            return 1

        captured_at = int(time.time())
        result = response.get("result") or {}
        output = {
            "capturedAt": captured_at,
            "capturedAtLocal": time.strftime("%Y-%m-%d %H:%M:%S %z", time.localtime(captured_at)),
            "rateLimits": result.get("rateLimits"),
            "rateLimitsByLimitId": result.get("rateLimitsByLimitId"),
        }
        print(json.dumps(output, sort_keys=True))
        return 0
    finally:
        process.terminate()
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=2)


if __name__ == "__main__":
    raise SystemExit(main())
