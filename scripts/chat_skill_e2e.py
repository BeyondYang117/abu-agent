#!/usr/bin/env python3
"""Live Chat + Skill tool E2E probe (mirrors Kivio agent loop against configured provider)."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import time
import uuid
from pathlib import Path
from typing import Any
from urllib import error, request

SETTINGS_PATH = Path.home() / "Library/Application Support/com.zmair.kivio/settings.json"
SKILLS_DIR = Path.home() / "Library/Application Support/com.zmair.kivio/skills"
REPORT_PATH = Path("/tmp/kivio-chat-skill-e2e-report.json")

SKILL_TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "skill_activate",
            "description": "Activate an Agent Skill by name.",
            "parameters": {
                "type": "object",
                "properties": {"name": {"type": "string"}},
                "required": ["name"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "skill_read_file",
            "description": "Read a file relative to the skill root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "relative_path": {"type": "string"},
                },
                "required": ["name", "relative_path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "skill_run_script",
            "description": "Run a script under scripts/.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "relative_path": {"type": "string"},
                    "args": {"type": "array", "items": {"type": "string"}},
                },
                "required": ["name", "relative_path"],
            },
        },
    },
]


def load_settings() -> dict[str, Any]:
    outer = json.loads(SETTINGS_PATH.read_text(encoding="utf-8"))
    return outer.get("settings", outer)


def load_provider(settings: dict[str, Any]) -> tuple[str, str, list[str]]:
    pid = settings.get("chatProviderId") or settings.get("chat_provider_id")
    model = settings.get("chatModel") or settings.get("chat_model")
    provider = next((p for p in settings.get("providers", []) if p.get("id") == pid), None)
    if not provider:
        raise RuntimeError(f"chat provider not found: {pid}")
    keys = provider.get("apiKeys") or provider.get("api_keys") or []
    if not keys:
        raise RuntimeError("no API keys configured for chat provider")
    base = (provider.get("baseUrl") or provider.get("base_url") or "").rstrip("/")
    return base, model, keys


def contains_dsml_markup(text: str) -> bool:
    lower = text.lower()
    return "dsml" in lower and ("invoke" in lower or "tool_calls" in lower)


def extract_openai_tool_calls(message: dict[str, Any]) -> list[dict[str, Any]]:
    calls = []
    for call in message.get("tool_calls") or []:
        fn = call.get("function") or {}
        name = fn.get("name")
        if not name:
            continue
        raw = fn.get("arguments") or "{}"
        try:
            args = json.loads(raw)
        except json.JSONDecodeError:
            args = {}
        calls.append(
            {
                "id": call.get("id") or f"tool_{uuid.uuid4()}",
                "name": name,
                "arguments": args,
            }
        )
    return calls


def extract_dsml_tool_calls(content: str) -> list[dict[str, Any]]:
    if not contains_dsml_markup(content):
        return []
    calls: list[dict[str, Any]] = []
    lower = content.lower()
    search_from = 0
    while True:
        rel = lower[search_from:].find("invoke")
        if rel < 0:
            break
        invoke_start = search_from + rel
        if invoke_start > 0 and content[invoke_start - 1] == "/":
            search_from = invoke_start + 6
            continue
        slice_ = content[invoke_start:]
        after = slice_[6:]
        m = re.search(r'name="([^"]+)"', after, re.I)
        if not m:
            search_from = invoke_start + 6
            continue
        tool_name = m.group(1)
        block_end = len(slice_)
        close = re.search(r"</[^>]*invoke[^>]*>", slice_, re.I)
        if close:
            block_end = close.end()
        block = slice_[:block_end]
        args: dict[str, Any] = {}
        for pm in re.finditer(
            r'parameter name="([^"]+)"[^>]*>(.*?)</', block, re.I | re.S
        ):
            key = pm.group(1)
            val = pm.group(2).strip()
            if key == "args":
                try:
                    args[key] = json.loads(val)
                except json.JSONDecodeError:
                    args[key] = val
            else:
                args[key] = val
        calls.append(
            {
                "id": f"tool_{uuid.uuid4()}",
                "name": tool_name,
                "arguments": args,
            }
        )
        search_from = invoke_start + block_end
    return calls


def skill_record(name: str) -> Path:
    direct = SKILLS_DIR / name
    if (direct / "SKILL.md").is_file():
        return direct
    for child in SKILLS_DIR.iterdir():
        if child.is_dir() and (child / "SKILL.md").is_file():
            if child.name == name:
                return child
    raise RuntimeError(f"skill not found: {name}")


def run_skill_tool(name: str, arguments: dict[str, Any]) -> str:
    record = skill_record(arguments.get("name") or name)
    if name == "skill_activate":
        body = (record / "SKILL.md").read_text(encoding="utf-8")
        return (
            f'<skill_content name="{arguments.get("name", record.name)}">\n'
            f"{body}\n</skill_content>"
        )[:4000]
    if name == "skill_read_file":
        rel = arguments["relative_path"]
        path = (record / rel).resolve()
        if not str(path).startswith(str(record.resolve())):
            raise RuntimeError("path traversal blocked")
        return path.read_text(encoding="utf-8")[:4000]
    if name == "skill_run_script":
        rel = arguments["relative_path"]
        if not rel.replace("\\", "/").startswith("scripts/"):
            raise RuntimeError("only scripts/ allowed")
        script = record / rel
        args = arguments.get("args") or []
        cmd = ["python3", str(script), *args]
        proc = subprocess.run(
            cmd,
            cwd=record,
            capture_output=True,
            text=True,
            timeout=60,
        )
        out = ""
        if proc.stdout.strip():
            out += "stdout:\n" + proc.stdout.strip()
        if proc.stderr.strip():
            out += ("\n" if out else "") + "stderr:\n" + proc.stderr.strip()
        if proc.returncode != 0:
            raise RuntimeError(out or f"exit {proc.returncode}")
        return out[:6000]
    raise RuntimeError(f"unknown tool {name}")


def chat_completion(
    base_url: str,
    api_key: str,
    model: str,
    messages: list[dict[str, Any]],
    tools: list[dict[str, Any]] | None,
) -> dict[str, Any]:
    body: dict[str, Any] = {
        "model": model,
        "messages": messages,
        "temperature": 0.7,
        "max_tokens": 2000,
    }
    if tools:
        body["tools"] = tools
    req = request.Request(
        f"{base_url}/chat/completions",
        data=json.dumps(body).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with request.urlopen(req, timeout=120) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
    except error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")[:800]
        raise RuntimeError(f"HTTP {exc.code}: {detail}") from exc
    choice = (payload.get("choices") or [{}])[0]
    return choice.get("message") or {}


def agent_turn(
    base_url: str,
    api_key: str,
    model: str,
    user_prompt: str,
    *,
    active_skill: str | None,
    max_rounds: int,
) -> dict[str, Any]:
    system = (
        "You are Kivio Chat E2E assistant. When skills are needed, call tools instead of describing commands. "
        "Available skill: tavily-multi-key (web search via scripts/tavily_cli.py)."
    )
    if active_skill:
        system += f"\nUser selected skill: {active_skill}. Call skill_activate first."
    messages: list[dict[str, Any]] = [
        {"role": "system", "content": system},
        {"role": "user", "content": user_prompt},
    ]
    tool_trace: list[dict[str, Any]] = []
    for round_idx in range(max_rounds):
        message = chat_completion(base_url, api_key, model, messages, SKILL_TOOLS)
        content = (message.get("content") or "").strip()
        tool_calls = extract_openai_tool_calls(message)
        if not tool_calls and content:
            tool_calls = extract_dsml_tool_calls(content)
        if not tool_calls:
            leaked_dsml = contains_dsml_markup(content)
            return {
                "rounds": round_idx,
                "final_content": content,
                "tool_trace": tool_trace,
                "dsml_leak": leaked_dsml,
                "ok": not leaked_dsml and bool(content),
            }
        assistant_msg: dict[str, Any] = {
            "role": "assistant",
            "content": message.get("content"),
            "tool_calls": [
                {
                    "id": c["id"],
                    "type": "function",
                    "function": {
                        "name": c["name"],
                        "arguments": json.dumps(c["arguments"], ensure_ascii=False),
                    },
                }
                for c in tool_calls
            ],
        }
        messages.append(assistant_msg)
        for call in tool_calls:
            started = time.time()
            try:
                result = run_skill_tool(call["name"], call["arguments"])
                status = "success"
                err = None
            except Exception as exc:  # noqa: BLE001
                result = str(exc)
                status = "error"
                err = str(exc)
            duration_ms = int((time.time() - started) * 1000)
            entry = {
                "round": round_idx + 1,
                "name": call["name"],
                "arguments": call["arguments"],
                "status": status,
                "duration_ms": duration_ms,
                "result_preview": result[:500],
                "error": err,
            }
            tool_trace.append(entry)
            messages.append(
                {
                    "role": "tool",
                    "tool_call_id": call["id"],
                    "content": result,
                }
            )
    return {
        "rounds": max_rounds,
        "final_content": "",
        "tool_trace": tool_trace,
        "dsml_leak": False,
        "ok": False,
        "error": "max tool rounds exhausted",
    }


def main() -> int:
    settings = load_settings()
    base_url, model, keys = load_provider(settings)
    chat_tools = settings.get("chatTools") or {}
    max_rounds = int(chat_tools.get("maxToolRounds") or 5)

    cases = [
        {
            "id": "math",
            "prompt": "1+1 等于几？只回答数字，不要调用任何工具。",
            "active_skill": None,
            "expect_tools": False,
        },
        {
            "id": "skill_list_tools",
            "prompt": "请用 tavily-multi-key：先 skill_activate，再 skill_run_script 运行 scripts/tavily_cli.py，参数为 list-tools --format text。把工具列表简要告诉我。",
            "active_skill": "tavily-multi-key",
            "expect_tools": True,
        },
        {
            "id": "weather_jilin",
            "prompt": "吉林市明天天气怎么样？用 tavily-multi-key 搜索后简短回答。",
            "active_skill": "tavily-multi-key",
            "expect_tools": True,
        },
    ]

    report: dict[str, Any] = {
        "provider_base": base_url,
        "model": model,
        "max_tool_rounds": max_rounds,
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "cases": [],
    }

    # Direct skill script smoke test
    skill_dir = SKILLS_DIR / "tavily-multi-key"
    direct = subprocess.run(
        [
            "python3",
            str(skill_dir / "scripts/tavily_cli.py"),
            "list-tools",
            "--format",
            "text",
        ],
        cwd=skill_dir,
        capture_output=True,
        text=True,
        timeout=30,
    )
    report["direct_skill_script"] = {
        "ok": direct.returncode == 0,
        "stdout_preview": (direct.stdout or "")[:300],
        "stderr_preview": (direct.stderr or "")[:200],
    }

    api_key = keys[0]
    for case in cases:
        print(f"\n=== Case: {case['id']} ===", flush=True)
        try:
            result = agent_turn(
                base_url,
                api_key,
                model,
                case["prompt"],
                active_skill=case.get("active_skill"),
                max_rounds=max_rounds,
            )
        except Exception as exc:  # noqa: BLE001
            result = {"ok": False, "error": str(exc), "tool_trace": []}
        used_tools = len(result.get("tool_trace") or [])
        if case.get("expect_tools"):
            result["expect_tools_met"] = used_tools > 0
        else:
            result["expect_tools_met"] = used_tools == 0
        result["case_ok"] = bool(
            result.get("ok")
            and result.get("expect_tools_met")
            and not result.get("dsml_leak")
        )
        print(json.dumps(result, ensure_ascii=False, indent=2)[:4000], flush=True)
        report["cases"].append({"id": case["id"], **result})

    report["all_ok"] = all(c.get("case_ok") for c in report["cases"]) and report[
        "direct_skill_script"
    ]["ok"]
    REPORT_PATH.write_text(
        json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(f"\nReport written: {REPORT_PATH}", flush=True)
    return 0 if report["all_ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
