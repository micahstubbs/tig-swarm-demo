import hashlib
import re


def normalize(text: str) -> str:
    return re.sub(r"[^a-z0-9 ]", "", text.lower()).strip()


def fingerprint(title: str, tag: str) -> str:
    raw = f"{normalize(title)}|{tag}"
    return hashlib.sha256(raw.encode()).hexdigest()[:16]


def jaccard_tokens(a: str, b: str) -> float:
    ta = set(normalize(a).split())
    tb = set(normalize(b).split())
    if not ta or not tb:
        return 0.0
    return len(ta & tb) / len(ta | tb)


def check_duplicate(
    new_title: str,
    new_tag: str,
    existing: list[dict],
) -> dict | None:
    """Check if a hypothesis is a duplicate. Returns the similar hypothesis dict or None."""
    fp = fingerprint(new_title, new_tag)
    for h in existing:
        if h["fingerprint"] == fp:
            return h
        if h["strategy_tag"] == new_tag and jaccard_tokens(new_title, h["title"]) > 0.6:
            return h
    return None


def check_saturation(tag: str, existing: list[dict], max_per_tag: int = 3) -> bool:
    """Check if a strategy tag has too many active hypotheses."""
    active_statuses = {"proposed", "claimed", "testing"}
    count = sum(
        1 for h in existing
        if h["strategy_tag"] == tag and h["status"] in active_statuses
    )
    return count >= max_per_tag
