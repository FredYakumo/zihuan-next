#!/usr/bin/env python3
"""Launch the production service with mock connections, then simulate external users."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


def publish(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")
    tmp.replace(path)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=REPO / "target/debug" / ("zihuan_next.exe" if os.name == "nt" else "zihuan_next"))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--color", choices=("always", "auto", "never"), default="always")
    args = parser.parse_args()
    use_color = args.color == "always" or (args.color == "auto" and sys.stdout.isatty())

    def status(text, passed):
        if not use_color:
            return text
        return f"\033[{32 if passed else 31}m{text}\033[0m"

    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"Build the service first; binary missing: {binary}")
    if args.output:
        run = args.output.resolve()
        run.mkdir(parents=True, exist_ok=False)
    else:
        run = Path(tempfile.mkdtemp(prefix="ims-role-behaviour-"))
    print(f"Artifacts: {run}", flush=True)
    config_path = run / "config/zihuan-next_aibot/system_config/system_config.json"
    config_path.parent.mkdir(parents=True)
    template = (HERE / "system_config.json").read_text(encoding="utf-8")
    config = json.loads(template.replace("__RUN_DIR__", run.as_posix()))
    config["python_runtime"] = {"kind": "custom_executable", "executable_path": sys.executable}
    config_path.write_text(json.dumps(config, ensure_ascii=False, indent=2), encoding="utf-8")
    shutil.copytree(HERE / "plugins", run / "plugins")
    shutil.copytree(REPO / "dynamic_script_engine", run / "dynamic_script_engine", ignore=shutil.ignore_patterns("src", "target", "__pycache__"))
    (run / "dag_nodes").mkdir()
    (run / "scheduled_jobs").mkdir()
    env = dict(os.environ, APPDATA=str(run / "config"), LOCALAPPDATA=str(run / "config"), XDG_CONFIG_HOME=str(run / "config"))
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}/api"
    reports = []
    suite_started = time.monotonic()
    with (run / "service.log").open("w", encoding="utf-8") as log:
        options = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {"start_new_session": True}
        process = subprocess.Popen([str(binary), "--host", "127.0.0.1", "--port", str(port)], cwd=run, env=env, stdout=log, stderr=subprocess.STDOUT, **options)
        def wait_for(label, check):
            deadline = time.monotonic() + args.timeout
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"service exited: {process.returncode}")
                value = check()
                if value:
                    return value
                time.sleep(0.05)
            raise TimeoutError(f"Timed out waiting for {label}")

        def get(path):
            try:
                with urllib.request.urlopen(base + path, timeout=2) as response:
                    return json.load(response)
            except (OSError, ValueError):
                return None

        def send(id, user, name, text, group=None, mention=False):
            segments = [{"type": "text", "data": {"text": text}}]
            if mention:
                segments.insert(0, {"type": "at", "data": {"qq": "99999"}})
            event = {"message_id": id, "message_type": "group" if group else "private", "sender": {"user_id": user, "nickname": name}, "message": segments}
            if group:
                event.update(group_id=group, group_name="测试群")
            publish(run / f"bot/inbox/{id:06}.json", event)
            # Wait for real SQL persistence, not merely adapter acceptance.
            wait_for(f"message {id} persistence", lambda: (get(f"/explorer/services/ims-role/messages?message_id={id}") or {}).get("records"))

        def outgoing(target, group=False):
            result = []
            for path in sorted((run / "bot/outbox").glob("*.json")):
                item = json.loads(path.read_text(encoding="utf-8"))
                params = item["params"]
                if str(params.get("group_id" if group else "user_id")) == str(target):
                    result.append(item)
            return result

        def text_of(items):
            return "".join(segment["data"]["text"] for item in items for segment in item["params"]["message"] if segment["type"] == "text").strip()

        def case(name, inputs, expected, execute):
            started = time.monotonic()
            test_name = name.replace(" ", "_")
            print(f"test {test_name} ...", flush=True)
            report = {"case": name, "input": inputs, "expected output": expected, "actual output": "<no reply>", "status": "FAIL"}
            try:
                execute(report)
                if report["actual output"] != expected:
                    raise AssertionError(f"expected {expected!r}, got {report['actual output']!r}")
                report["status"] = "PASS"
            except Exception as error:
                report["error"] = str(error)
                raise
            finally:
                report["time usage"] = f"{time.monotonic() - started:.3f}s"
                reports.append(report)
                print("    input:")
                for message in inputs:
                    print(f"        {message}")
                for field in ("expected output", "actual output", "group window selected", "error", "time usage"):
                    if field in report:
                        print(f"    {field}: {report[field]}")
                passed = report["status"] == "PASS"
                print(f"test {test_name} ... {status('pass' if passed else 'FAILED', passed)}\n", flush=True)
                publish(run / "results.json", reports)

        try:
            wait_for("HTTP server and role startup", lambda: get("/system/services"))
            wait_for("mock adapter readiness", lambda: (run / "bot/ready.json").exists())
            print("\nrunning 2 tests", flush=True)

            def direct(report):
                send(101, 1001, "用户", "世界上最好的二次元游戏是什么")
                reply = wait_for("private bot reply", lambda: outgoing(1001))
                report["actual output"] = text_of(reply)
                assert len(reply) == 1, "Unexpected duplicate replies"
                assert reply[0]["action"] == "send_private_msg"
            case("private reply", ["世界上最好的二次元游戏是什么"], "原神", direct)

            def completed_context():
                context = get("/explorer/services/ims-role/chat-context?sender_id=2004")
                return context if context and any(turn["finished"] for turn in context["turns"]) else None

            def group(report):
                send(201, 2001, "用户A", "我喜欢刻晴", 3001)
                send(202, 2002, "用户B", "啥玩意", 3001)
                send(203, 2003, "用户C", "什么原批", 3001)
                assert not outgoing(3001, True), "Unmentioned messages triggered a reply"
                send(204, 2004, "用户", "用户A喜欢什么角色", 3001, True)
                reply = wait_for("group bot reply", lambda: outgoing(3001, True))
                report["actual output"] = text_of(reply)
                assert len(reply) == 1 and reply[0]["action"] == "send_group_msg"
                traces = [json.loads(p.read_text(encoding="utf-8")) for p in (run / "llm").glob("*.json")]
                selections = [t for t in traces if t["kind"] == "group_window"]
                assert len(selections) == 1, "group window procedure did not run exactly once"
                selection = selections[0]
                window = selection["messages"][-1].split("[Group Window Messages]")[1]
                lines = [line for line in window.splitlines() if line.startswith("[")]
                assert len(lines) == 3, f"Expected three candidates, got {lines}"
                assert [line.split(": ")[-1] for line in lines] == ["什么原批", "啥玩意", "我喜欢刻晴"]
                indexes = json.loads(selection["output"])["related"]
                assert indexes == [3], f"Incorrect related selection: {indexes}"
                brains = [t for t in traces if t["kind"] == "brain" and t["output"] == report["actual output"]]
                history = next(m for m in brains[0]["messages"] if m.startswith("Here are the group messages related"))
                assert "用户A(2001): 我喜欢刻晴" in history
                assert "啥玩意" not in history and "什么原批" not in history
                context = wait_for("completed group context", lambda: completed_context())
                stages = [entry for turn in context["turns"] for entry in turn["entries"] if entry.get("stage") == "group-message-window"]
                assert len(stages) == 1, "Missing group window procedure stage"
                detail = stages[0]["detail"]
                assert detail["candidates"] == 3 and detail["selected_indexes"] == [3]
                assert [message["content"] for message in detail["related"]] == ["我喜欢刻晴"]
                publish(run / "group-context.json", context)
                report["group window selected"] = "我喜欢刻晴"
            case("group window", ["用户A: 我喜欢刻晴", "用户B: 啥玩意", "用户C: 什么原批", "用户 @bot: 用户A喜欢什么角色"], "刻晴", group)
        finally:
            if process.poll() is None:
                if os.name == "nt":
                    subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
                else:
                    os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            if reports:
                passed = sum(report["status"] == "PASS" for report in reports)
                failed = len(reports) - passed
                result = status("pass" if failed == 0 else "FAILED", failed == 0)
                print(f"test result: {result}. {passed} passed; {failed} failed; finished in {time.monotonic() - suite_started:.3f}s", flush=True)


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    main()
