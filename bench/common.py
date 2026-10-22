"""Shared code for the gains-study tools: read Claude Code transcripts and price tokens.

Never writes outside the paths a caller gives it. Reads transcripts only.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
PRICES_PATH = os.path.join(HERE, "prices.json")
# The tool-visible token types, in the order the CSV and reports print them.
TYPES = ("input", "output", "cache_read", "cache_write_5m", "cache_write_1h", "cache_creation_unsplit")


class PriceError(Exception):
    pass


def load_prices(path=PRICES_PATH):
    with open(path) as f:
        return json.load(f)


def base_model(prices, model):
    """Resolve a model id to a price-table id, or raise PriceError naming the id."""
    model = prices.get("aliases", {}).get(model, model)
    if model not in prices["models"]:
        raise PriceError("no price for model id %r (add it to prices.json)" % model)
    return model


def usd(prices, model, tokens):
    """Price one {type: count} dict for one model, in USD."""
    p = prices["models"][base_model(prices, model)]
    rate = {"input": p["input"], "output": p["output"], "cache_read": p["cache_read"],
            "cache_write_5m": p["cache_write_5m"], "cache_write_1h": p["cache_write_1h"],
            "cache_creation_unsplit": p["cache_write_1h"]}
    return sum(tokens.get(t, 0) * rate[t] for t in TYPES) / 1e6


def _split(usage, model):
    """One usage dict -> {type: count}. An unsplit cache write is counted apart, priced at 1h."""
    out = dict.fromkeys(TYPES, 0)
    out["input"] = usage.get("input_tokens") or 0
    out["output"] = usage.get("output_tokens") or 0
    out["cache_read"] = usage.get("cache_read_input_tokens") or 0
    total = usage.get("cache_creation_input_tokens") or 0
    cc = usage.get("cache_creation")
    if isinstance(cc, dict) and (cc.get("ephemeral_5m_input_tokens") is not None
                                 or cc.get("ephemeral_1h_input_tokens") is not None):
        out["cache_write_5m"] = cc.get("ephemeral_5m_input_tokens") or 0
        out["cache_write_1h"] = cc.get("ephemeral_1h_input_tokens") or 0
        out["cache_creation_unsplit"] = max(total - out["cache_write_5m"] - out["cache_write_1h"], 0)
    else:
        out["cache_creation_unsplit"] = total
    return out


def assistant_records(path):
    """Yield (key, model, timestamp, {model: tokens}, cwd) per assistant line with billable usage.

    key is message.id, else requestId, else file:line (never deduped).
    Messages from <synthetic> carry no billable usage and are skipped.
    shortcut: an `iterations` entry whose type is not "message" (an advisor call) is priced under
    its own model; none exist in the transcripts seen so far, so that path is untested on real data.
    """
    with open(path, errors="replace") as f:
        for n, line in enumerate(f):
            if '"assistant"' not in line:
                continue
            try:
                o = json.loads(line)
            except ValueError:
                continue
            m = o.get("message")
            if o.get("type") != "assistant" or not isinstance(m, dict):
                continue
            model, usage = m.get("model"), m.get("usage")
            if not isinstance(usage, dict) or model == "<synthetic>":
                continue
            if not model:
                raise PriceError("%s line %d: usage without a model id" % (path, n + 1))
            key = m.get("id") or o.get("requestId") or "%s:%d" % (path, n)
            by_model = {model: _split(usage, model)}
            for it in usage.get("iterations") or []:
                if it.get("type", "message") != "message":
                    im = it.get("model")
                    if not im:
                        raise PriceError("%s line %d: iteration without a model id" % (path, n + 1))
                    t = by_model.setdefault(im, dict.fromkeys(TYPES, 0))
                    for k, v in _split(it, im).items():
                        t[k] += v
            yield key, model, o.get("timestamp"), by_model, o.get("cwd")


def transcript_cwd(path):
    """The first `cwd` a transcript's lines carry, or None."""
    with open(path, errors="replace") as f:
        for line in f:
            if '"cwd"' in line:
                try:
                    c = json.loads(line).get("cwd")
                except ValueError:
                    continue
                if c:
                    return c
    return None


def add(into, more):
    for k, v in more.items():
        into[k] = into.get(k, 0) + v


def collect(paths, start=None, end=None):
    """Sum tokens over transcript files, counting each message id once across all of them.

    Returns {path: {model: {type: n}}}. A message goes to the first file that carries it.
    start and end are ISO timestamps (inclusive); a message outside them is not counted,
    and a message without a timestamp is not counted when either bound is set.
    """
    seen = set()
    result = {}
    for p in paths:
        per = result.setdefault(p, {})
        for key, _model, ts, by_model, _cwd in assistant_records(p):
            if key in seen:
                continue
            if (start or end) and (not ts or (start and ts < start) or (end and ts > end)):
                continue
            seen.add(key)
            for m, toks in by_model.items():
                add(per.setdefault(m, {}), toks)
    return result


def merge_models(per_file):
    """{path: {model: tokens}} -> {model: tokens}."""
    out = {}
    for per in per_file.values():
        for m, toks in per.items():
            add(out.setdefault(m, {}), toks)
    return out


def total_usd(prices, by_model):
    return sum(usd(prices, m, t) for m, t in by_model.items())
