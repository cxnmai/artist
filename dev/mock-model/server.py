"""Local, test-only mock of a chat-completions API. No third-party packages."""

import argparse
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from responses import reply, stream_frames


class Handler(BaseHTTPRequestHandler):
    requests_seen: list[dict] = []
    lock = threading.Lock()
    default_scenario = "read_then_answer"

    def send_json(self, status: int, value: object) -> None:
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        if self.path == "/health":
            self.send_json(200, {"ok": True})
        elif self.path == "/requests":
            with self.lock:
                self.send_json(200, self.requests_seen.copy())
        else:
            self.send_json(404, {"error": "not found"})

    def do_POST(self) -> None:
        if self.path != "/v1/chat/completions":
            self.send_json(404, {"error": "not found"})
            return
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if size <= 0 or size > 1_000_000:
                self.send_json(413, {"error": "request must be 1..1000000 bytes"})
                return
            request = json.loads(self.rfile.read(size))
            if not isinstance(request, dict):
                raise ValueError("request must be an object")
            scenario = self.headers.get("X-Mock-Scenario", self.default_scenario)
            with self.lock:
                self.requests_seen.append({"scenario": scenario, "body": request})
                del self.requests_seen[:-100]
            if scenario == "http_error":
                self.send_json(429, {"error": {"message": "mock rate limit"}})
                return
            if scenario == "malformed_json":
                body = b'{"choices": ['
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
            if scenario == "slow":
                time.sleep(2)
            response = reply(request, scenario)
            if request.get("stream"):
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                for frame in stream_frames(response):
                    data = frame.encode()
                    # Break transport chunks independently of JSON/SSE frame boundaries.
                    for part in (data[:5], data[5:]):
                        self.wfile.write(part)
                        self.wfile.flush()
                        time.sleep(0.01)
            else:
                self.send_json(200, response)
        except (ValueError, json.JSONDecodeError) as error:
            self.send_json(400, {"error": str(error)})
        except (BrokenPipeError, ConnectionResetError):
            pass


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--scenario", default="read_then_answer")
    args = parser.parse_args()
    Handler.default_scenario = args.scenario
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"Mock model at http://127.0.0.1:{args.port}/v1/chat/completions", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
