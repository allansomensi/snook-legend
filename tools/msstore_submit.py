#!/usr/bin/env python3
"""Sends a version to the Microsoft Store: a new submission in Partner Center
with the new MSIX (in place of the previous one) and "What's new in this
version" in every language of the Store listing, written from the game's
own "What's new" (crates/client/whatsnew/<language>.md). Everything else
in the listing (descriptions, screenshots, prices, age rating) is kept as
it was in the last published submission. See docs/MSSTORE.md.

The release notes, as they will be sent (nothing is sent):

    python3 tools/msstore_submit.py notes 0.12.3
    python3 tools/msstore_submit.py notes 0.12.3 --lang pt-BR

The submission (what the Microsoft Store workflow runs on a release tag):

    MSSTORE_TENANT_ID=… MSSTORE_CLIENT_ID=… MSSTORE_CLIENT_SECRET=… \\
    python3 tools/msstore_submit.py submit --app-id 9NZ15BNMZPTG \\
        --version 0.12.3 --package target/msix/SnookLegend_0.12.3.0_x64.msix

The credentials are those of a Microsoft Entra application added to the
Partner Center account (Account settings → User management → Microsoft
Entra applications) with the Manager role. It uses the Microsoft Store
submission API (manage.devcenter.microsoft.com) and Python's standard
library only.
"""

import argparse
import base64
import http.client
import io
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_metainfo import WHATSNEW, notes as whatsnew_notes  # noqa: E402

API = "https://manage.devcenter.microsoft.com"
TOKEN_URL = "https://login.microsoftonline.com/{tenant}/oauth2/token"
USER_AGENT = "snook-legend-release"
# Partner Center's limit for "What's new in this version".
MAX_NOTES = 1500
# Older versions named after this one's highlights, while they fit.
EARLIER_VERSIONS = 3
# Upload in blocks of this size (Azure Blob Storage, the submission's SAS URL).
BLOCK = 8 * 1024 * 1024


# ---------------------------------------------------------------- the notes


def languages():
    """The codes of crates/client/whatsnew/*.md (en, pt-BR, zh-CN…)."""
    return sorted(f[:-3] for f in os.listdir(WHATSNEW) if f.endswith(".md"))


def whatsnew_for(listing):
    """The whatsnew file for a Store listing's language (`en-us`, `pt-br`,
    `zh-hans`…), or None when the game has no notes in it."""
    parts = listing.lower().replace("_", "-").split("-")
    primary, rest = parts[0], parts[1:]
    if primary == "pt":
        return "pt-PT" if "pt" in rest else "pt-BR"
    if primary == "zh":
        # The game's Chinese is simplified: not for a traditional listing.
        return None if set(rest) & {"hant", "tw", "hk", "mo"} else "zh-CN"
    if primary == "tl":
        primary = "fil"
    return primary if primary in languages() else None


# How a title ends before its text, when it has no punctuation of its own.
FULL_STOP = {"zh-CN": "。", "ja": "。", "hi": "।", "bn": "।", "th": ""}


def _joined(code, title, text):
    """A highlight on one line: its title, as a sentence, then its text."""
    stop = FULL_STOP.get(code, ".")
    if title[-1:] in ".!?…:;。！？：؟।":
        stop = ""
    space = "" if code in ("zh-CN", "ja") and (stop or title[-1:] in "。！？：") else " "
    return f"{title}{stop}{space}{text}"


def release_notes(code, version):
    """ "What's new in this version" in the whatsnew language `code`: the
    version's highlights, then the titles of the ones before it, within
    Partner Center's 1,500 characters (long highlights keep their title
    only, from the last one up)."""
    by_version = whatsnew_notes(code)
    index = next((i for i, (v, _) in enumerate(by_version) if v == version), None)
    if index is None:
        sys.exit(f"whatsnew/{code}.md has no section for {version} (see docs/RELEASING.md)")
    items = by_version[index][1]
    full = len(items)
    while True:
        lines = [version]
        for n, (title, text) in enumerate(items):
            lines.append("• " + (_joined(code, title, text) if n < full else title))
        text = "\n".join(lines)
        if len(text) <= MAX_NOTES or full == 0:
            break
        full -= 1
    text = text[:MAX_NOTES]
    earlier = []
    for v, older in by_version[index + 1 : index + 1 + EARLIER_VERSIONS]:
        line = f"{v}: " + " · ".join(t.rstrip(".") for t, _ in older)
        candidate = text + "\n\n" + "\n".join(earlier + [line])
        if len(candidate) > MAX_NOTES:
            break
        earlier.append(line)
    return text + ("\n\n" + "\n".join(earlier) if earlier else "")


# ---------------------------------------------------------------- the API


class ApiError(Exception):
    pass


def request(method, url, body=None, headers=None, token=None):
    """One HTTP request; JSON in and out unless `body` is bytes."""
    headers = dict(headers or {})
    headers.setdefault("User-Agent", USER_AGENT)
    if token:
        headers["Authorization"] = f"Bearer {token}"
    data = body
    if body is not None and not isinstance(body, bytes):
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=300) as resp:
                raw = resp.read()
                kind = resp.headers.get("Content-Type", "")
                return json.loads(raw) if raw and "json" in kind else raw
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace")[:2000]
            # Busy or briefly down: try again; anything else is an answer.
            if e.code in (429, 500, 502, 503, 504) and attempt < 3:
                time.sleep(10 * (attempt + 1))
                continue
            where = url.split("?")[0]
            raise ApiError(f"{method} {where}: HTTP {e.code}: {detail}") from None
        except (urllib.error.URLError, http.client.HTTPException, OSError) as e:
            # The connection failed or dropped: try again.
            if attempt < 3:
                time.sleep(10 * (attempt + 1))
                continue
            reason = getattr(e, "reason", None) or e
            raise ApiError(f"{method} {url.split('?')[0]}: {reason}") from None


def access_token(tenant, client_id, secret, token_url=TOKEN_URL, api=API):
    form = urllib.parse.urlencode(
        {
            "grant_type": "client_credentials",
            "client_id": client_id,
            "client_secret": secret,
            "resource": api,
        }
    ).encode()
    answer = request(
        "POST",
        token_url.format(tenant=tenant),
        form,
        {"Content-Type": "application/x-www-form-urlencoded"},
    )
    if isinstance(answer, bytes):
        answer = json.loads(answer)
    return answer["access_token"]


def zipped(package):
    """The package in a zip, at its root, as the submission API wants it."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.write(package, os.path.basename(package))
    return buf.getvalue()


def upload(sas_url, data):
    """Puts `data` at the submission's file upload URL, block by block."""
    common = {"x-ms-version": "2019-12-12"}
    ids = []
    for n, start in enumerate(range(0, len(data), BLOCK)):
        block_id = base64.b64encode(f"block-{n:06d}".encode()).decode()
        ids.append(block_id)
        url = f"{sas_url}&comp=block&blockid={urllib.parse.quote(block_id)}"
        request("PUT", url, data[start : start + BLOCK], common)
        print(f"  uploaded {min(start + BLOCK, len(data)) // 1024} of {len(data) // 1024} KiB")
    xml = '<?xml version="1.0" encoding="utf-8"?><BlockList>'
    xml += "".join(f"<Latest>{i}</Latest>" for i in ids) + "</BlockList>"
    request(
        "PUT",
        f"{sas_url}&comp=blocklist",
        xml.encode(),
        {**common, "Content-Type": "application/xml"},
    )


def submit(args, api=API, token_url=TOKEN_URL, poll_every=30):
    env = {k: os.environ.get(f"MSSTORE_{k}", "") for k in ("TENANT_ID", "CLIENT_ID", "CLIENT_SECRET")}
    missing = [f"MSSTORE_{k}" for k, v in env.items() if not v]
    if missing:
        sys.exit(f"Missing {', '.join(missing)} (the repository's secrets, see docs/MSSTORE.md)")
    if not os.path.isfile(args.package):
        sys.exit(f"{args.package}: no such package")

    # Every language's notes first: a missing section stops here, before
    # anything changes in Partner Center.
    notes = {code: release_notes(code, args.version) for code in languages()}

    token = access_token(env["TENANT_ID"], env["CLIENT_ID"], env["CLIENT_SECRET"], token_url, api)
    app_url = f"{api}/v1.0/my/applications/{args.app_id}"
    app = request("GET", app_url, token=token)
    pending = app.get("pendingApplicationSubmission")
    if pending:
        if not args.replace_pending:
            sys.exit(
                f"Partner Center already has a submission in progress ({pending['id']}). "
                "Finish or delete it there, or run the workflow by hand with "
                "'Replace a submission in progress' ticked."
            )
        print(f"Deleting the submission in progress {pending['id']}")
        request("DELETE", f"{app_url}/submissions/{pending['id']}", token=token)

    # A new submission starts as a copy of the last published one.
    sub = request("POST", f"{app_url}/submissions", b"", token=token)
    sub_url = f"{app_url}/submissions/{sub['id']}"
    print(f"Submission {sub['id']} created")
    try:
        for listing, content in sub.get("listings", {}).items():
            code = whatsnew_for(listing)
            source = f"whatsnew/{code}.md" if code else "whatsnew/en.md (the game has no notes in it)"
            code = code or "en"
            content.setdefault("baseListing", {})["releaseNotes"] = notes[code]
            print(f"  {listing}: notes from {source}, {len(notes[code])} characters")

        name = os.path.basename(args.package)
        for package in sub.get("applicationPackages", []):
            if package.get("fileStatus") != "PendingDelete":
                print(f"  package {package.get('fileName')} ({package.get('version')}): removed")
            package["fileStatus"] = "PendingDelete"
        sub.setdefault("applicationPackages", []).append(
            {
                "fileName": name,
                "fileStatus": "PendingUpload",
                "minimumDirectXVersion": "None",
                "minimumSystemRam": "None",
            }
        )
        print(f"  package {name}: added")

        sub = request("PUT", sub_url, sub, token=token)
        print("Uploading the package")
        upload(sub["fileUploadUrl"], zipped(args.package))
    except BaseException:
        # Nothing was sent to certification: don't leave a half-made draft.
        print(f"Something failed: deleting submission {sub['id']}", file=sys.stderr)
        try:
            request("DELETE", sub_url, token=token)
        except ApiError as e:
            print(f"  could not delete it ({e}); delete it in Partner Center", file=sys.stderr)
        raise

    request("POST", f"{sub_url}/commit", b"", token=token)
    print("Committed: Partner Center is checking the submission")
    started = token_at = time.monotonic()
    while True:
        state = request("GET", f"{sub_url}/status", token=token)
        status = state.get("status")
        if status != "CommitStarted":
            break
        if time.monotonic() - started > args.timeout * 60:
            sys.exit(
                f"Still '{status}' after {args.timeout} minutes: see the submission in Partner Center"
            )
        time.sleep(poll_every)
        # Tokens last about an hour: a fresh one for a long wait.
        if time.monotonic() - token_at > 45 * 60:
            token = access_token(env["TENANT_ID"], env["CLIENT_ID"], env["CLIENT_SECRET"], token_url, api)
            token_at = time.monotonic()
    details = state.get("statusDetails") or {}
    for warning in details.get("warnings") or []:
        print(f"  warning: {warning.get('code')}: {warning.get('details')}")
    if status == "CommitFailed" or details.get("errors"):
        for error in details.get("errors") or []:
            print(f"  error: {error.get('code')}: {error.get('details')}", file=sys.stderr)
        sys.exit(f"Partner Center refused the submission ({status}): it was not sent to certification")
    print(f"Sent to the Store: {status} (certification takes up to three working days)")


# ---------------------------------------------------------------- main


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)

    show = sub.add_parser("notes", help="print the release notes of every language")
    show.add_argument("version")
    show.add_argument("--lang", help="only this whatsnew language (pt-BR, en…)")

    send = sub.add_parser("submit", help="send the version to the Store")
    send.add_argument("--app-id", required=True, help="the Store ID (9NZ…)")
    send.add_argument("--version", required=True, help="the version, without the v (0.12.3)")
    send.add_argument("--package", required=True, help="the .msix")
    send.add_argument(
        "--replace-pending",
        action="store_true",
        help="delete a submission already in progress in Partner Center",
    )
    send.add_argument("--timeout", type=int, default=60, help="minutes to wait for Partner Center")

    args = parser.parse_args(argv)
    if args.command == "notes":
        for code in [args.lang] if args.lang else languages():
            text = release_notes(code, args.version)
            print(f"===== {code} ({len(text)} characters)\n{text}\n")
    else:
        try:
            submit(args)
        except ApiError as e:
            sys.exit(str(e))


if __name__ == "__main__":
    main()
