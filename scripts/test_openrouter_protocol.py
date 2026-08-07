#!/usr/bin/env python3
"""Probe an OpenAI-compatible endpoint and identify its supported API protocol.

The API key is read from the ``Openrouter_KEY`` environment variable and is
never printed.  By default the script:

1. Calls ``GET /v1/models`` to verify the endpoint and choose a model.
2. Sends a tiny request to ``POST /v1/responses``.
3. Sends a tiny request to ``POST /v1/chat/completions``.

Use ``--model`` or ``OPENROUTER_MODEL`` when the first model returned by the
endpoint is not suitable for generation.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from dataclasses import dataclass
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


DEFAULT_BASE_URL = "https://openrouter.zjxqai.com"
API_KEY_ENV = "Openrouter_KEY"
MODEL_ENV = "OPENROUTER_MODEL"
PROBE_PROMPT = "Reply with exactly: OK"


@dataclass
class ApiResult:
    status: int | None
    body: Any
    raw: str
    error: str | None
    elapsed_ms: int

    @property
    def is_http_success(self) -> bool:
        return self.status is not None and 200 <= self.status < 300


def normalize_base_url(value: str) -> str:
    """Return a base URL ending in ``/v1`` exactly once."""

    base = value.strip().rstrip("/")
    if not base:
        raise ValueError("base URL cannot be empty")
    return base if base.endswith("/v1") else f"{base}/v1"


def parse_json(raw: str) -> Any:
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return raw


def request_json(
    base_url: str,
    path: str,
    api_key: str,
    *,
    method: str = "GET",
    payload: dict[str, Any] | None = None,
    timeout: float,
) -> ApiResult:
    """Make one JSON request without exposing the API key in output."""

    url = f"{base_url}{path}"
    body = None
    headers = {
        "Accept": "application/json",
        "Authorization": f"Bearer {api_key}",
        "User-Agent": "openrouter-protocol-probe/1.0",
    }
    if payload is not None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        headers["Content-Type"] = "application/json"

    request = Request(url, data=body, headers=headers, method=method)
    started = time.perf_counter()

    try:
        with urlopen(request, timeout=timeout) as response:
            raw = response.read().decode("utf-8", errors="replace")
            status = response.status
            return ApiResult(
                status=status,
                body=parse_json(raw),
                raw=raw,
                error=None,
                elapsed_ms=elapsed_ms(started),
            )
    except HTTPError as exc:
        raw = exc.read().decode("utf-8", errors="replace")
        return ApiResult(
            status=exc.code,
            body=parse_json(raw),
            raw=raw,
            error=None,
            elapsed_ms=elapsed_ms(started),
        )
    except (TimeoutError, URLError, OSError) as exc:
        return ApiResult(
            status=None,
            body=None,
            raw="",
            error=str(exc),
            elapsed_ms=elapsed_ms(started),
        )


def elapsed_ms(started: float) -> int:
    return round((time.perf_counter() - started) * 1000)


def error_message(result: ApiResult) -> str:
    if result.error:
        return result.error

    if isinstance(result.body, dict):
        error = result.body.get("error")
        if isinstance(error, dict):
            message = error.get("message")
            if message:
                return str(message)
        if error:
            return str(error)

    text = result.raw.strip().replace("\n", " ")
    return text[:300] if text else "empty response"


def response_shape(protocol: str, body: Any) -> str:
    if not isinstance(body, dict):
        return "non-JSON response"
    if protocol == "Responses API" and ("output" in body or body.get("object") == "response"):
        return "Responses response shape"
    if protocol == "Chat Completions API" and (
        "choices" in body or body.get("object") == "chat.completion"
    ):
        return "Chat Completions response shape"
    return "JSON response"


def classify(protocol: str, result: ApiResult) -> str:
    if result.is_http_success:
        return "SUPPORTED"
    if result.status in (404, 405):
        return "NOT_FOUND_OR_NOT_SUPPORTED"
    if result.status in (401, 403):
        return "REACHABLE_BUT_AUTH_REJECTED"
    if result.status in (400, 422):
        return "ROUTE_REACHED_REQUEST_REJECTED"
    if result.status is None:
        return "NETWORK_ERROR"
    return "SERVER_ERROR"


def choose_model(models: list[str], requested: str | None) -> str | None:
    if requested:
        return requested
    return models[0] if models else None


def print_result(protocol: str, path: str, result: ApiResult) -> None:
    state = classify(protocol, result)
    status = str(result.status) if result.status is not None else "-"
    print(f"\n[{protocol}]")
    print(f"  POST {path} -> HTTP {status} ({result.elapsed_ms} ms)")
    print(f"  result: {state}")
    if result.is_http_success:
        print(f"  shape: {response_shape(protocol, result.body)}")
    else:
        print(f"  detail: {error_message(result)}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Test whether an OpenAI-compatible endpoint supports Responses or Chat Completions."
    )
    parser.add_argument(
        "--base-url",
        default=os.getenv("OPENROUTER_BASE_URL", DEFAULT_BASE_URL),
        help=f"API base URL (default: {DEFAULT_BASE_URL})",
    )
    parser.add_argument(
        "--model",
        default=os.getenv(MODEL_ENV),
        help=f"Model ID; defaults to {MODEL_ENV} or the first model from /v1/models",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=20.0,
        help="Per-request timeout in seconds (default: 20)",
    )
    parser.add_argument(
        "--route-only",
        action="store_true",
        help="Do not generate text; send empty validation requests to identify routes",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    api_key = os.getenv(API_KEY_ENV)
    if not api_key:
        print(f"错误：未找到环境变量 {API_KEY_ENV}。", file=sys.stderr)
        print(
            f'PowerShell: $env:{API_KEY_ENV} = "你的 API Key"',
            file=sys.stderr,
        )
        print(
            f'Bash: export {API_KEY_ENV}="你的 API Key"',
            file=sys.stderr,
        )
        return 2

    try:
        base_url = normalize_base_url(args.base_url)
    except ValueError as exc:
        print(f"错误：{exc}", file=sys.stderr)
        return 2

    print(f"Endpoint: {base_url}")
    print(f"Key: loaded from {API_KEY_ENV} (not displayed)")

    models_result = request_json(
        base_url,
        "/models",
        api_key,
        timeout=args.timeout,
    )
    print(f"\n[Models API]")
    status = str(models_result.status) if models_result.status is not None else "-"
    print(f"  GET /v1/models -> HTTP {status} ({models_result.elapsed_ms} ms)")

    model_ids: list[str] = []
    if models_result.is_http_success and isinstance(models_result.body, dict):
        data = models_result.body.get("data", [])
        if isinstance(data, list):
            model_ids = [
                item.get("id")
                for item in data
                if isinstance(item, dict) and isinstance(item.get("id"), str)
            ]
        print(f"  models returned: {len(model_ids)}")
    else:
        print(f"  detail: {error_message(models_result)}")

    if args.route_only:
        model = args.model or "(not used in route-only mode)"
        responses_payload: dict[str, Any] = {}
        chat_payload: dict[str, Any] = {}
    else:
        model = choose_model(model_ids, args.model)
        if not model:
            print(
                "\n错误：无法自动选择模型。请使用 --model MODEL_ID 或设置 OPENROUTER_MODEL。",
                file=sys.stderr,
            )
            return 1
        responses_payload = {
            "model": model,
            "input": PROBE_PROMPT,
            "store": False,
        }
        chat_payload = {
            "model": model,
            "messages": [{"role": "user", "content": PROBE_PROMPT}],
            "stream": False,
        }
    print(f"  model used for probes: {model}")

    responses_result = request_json(
        base_url,
        "/responses",
        api_key,
        method="POST",
        payload=responses_payload,
        timeout=args.timeout,
    )
    chat_result = request_json(
        base_url,
        "/chat/completions",
        api_key,
        method="POST",
        payload=chat_payload,
        timeout=args.timeout,
    )

    print_result("Responses API", "/v1/responses", responses_result)
    print_result("Chat Completions API", "/v1/chat/completions", chat_result)

    responses_ok = responses_result.is_http_success
    chat_ok = chat_result.is_http_success
    print("\n结论:")
    if responses_ok and chat_ok:
        print("  支持 Responses API 和 Chat Completions API。")
    elif responses_ok:
        print("  支持 Responses API；Chat Completions 未通过测试。")
    elif chat_ok:
        print("  支持 Chat Completions API；Responses API 未通过测试。")
    else:
        print("  未能确认可用协议，请根据上面的 HTTP 状态和错误信息排查。")

    return 0 if responses_ok or chat_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
