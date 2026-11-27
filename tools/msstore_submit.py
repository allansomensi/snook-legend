#!/usr/bin/env python3
"""Sends a version to the Microsoft Store: a new submission in Partner Center
with the new MSIX (in place of the previous one) and "What's new in this
version" in every language of the Store listing, written from the game's
own "What's new" (crates/client/whatsnew/<language>.md). Prices, the age
rating and the rest of Partner Center are kept as they were in the last
published submission. See docs/MSSTORE.md and docs/STORE-LISTING.md.

The release notes, as they will be sent (nothing is sent):

    python3 tools/msstore_submit.py notes 0.12.3
    python3 tools/msstore_submit.py notes 0.12.3 --lang pt-BR

The Store listings of store-listing/*.md, checked against Partner Center's
limits (nothing is sent):

    python3 tools/msstore_submit.py listings

The connection with Partner Center, changing nothing (the workflow, run by
hand with "Only check the connection" ticked):

    MSSTORE_TENANT_ID=… MSSTORE_CLIENT_ID=… MSSTORE_CLIENT_SECRET=… \\
    python3 tools/msstore_submit.py check --app-id 9NZ15BNMZPTG

The submission (what the Microsoft Store workflow runs on a release tag):

    MSSTORE_TENANT_ID=… MSSTORE_CLIENT_ID=… MSSTORE_CLIENT_SECRET=… \\
    python3 tools/msstore_submit.py submit --app-id 9NZ15BNMZPTG \\
        --version 0.12.3 --package target/msix/SnookLegend_0.12.3.0_x64.msix

It also writes every listing's texts from store-listing/<language>.md and
makes the listings Partner Center doesn't have yet, with their screenshots
(store-art/screenshots/<language>/, English until a language has its own)
and logos (store-art/). The Store page alone, texts, screenshots and logos
of every language, without a new package (the workflow, run by hand with
"Update the Store page" ticked):

    MSSTORE_TENANT_ID=… MSSTORE_CLIENT_ID=… MSSTORE_CLIENT_SECRET=… \\
    python3 tools/msstore_submit.py listing --app-id 9NZ15BNMZPTG

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
import re
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


# ---------------------------------------------------------------- the listing

LISTING = os.path.join(os.path.dirname(WHATSNEW), "..", "..", "store-listing")
LISTING = os.path.normpath(LISTING)
FIELDS = ("Title", "Short description", "Description", "Features", "Search terms", "License terms")
# Partner Center's limits (search terms: also 21 words in all).
MAX_TEXT = {"Short description": 1000, "Description": 10000, "License terms": 10000}
MAX_FEATURES, MAX_FEATURE = 20, 200
MAX_TERMS, MAX_TERM, MAX_TERM_WORDS = 7, 30, 21
# The Store listing made for each of the game's languages when Partner
# Center has none in it yet (the first three came from the package).
LISTING_LANGUAGE = {
    "en": "en-us", "pt-BR": "pt-br", "es": "es-es", "pt-PT": "pt-pt",
    "fr": "fr-fr", "de": "de-de", "it": "it-it", "nl": "nl-nl", "sv": "sv-se",
    "pl": "pl-pl", "cs": "cs-cz", "hu": "hu-hu", "ro": "ro-ro", "el": "el-gr",
    "ru": "ru-ru", "uk": "uk-ua", "tr": "tr-tr", "id": "id-id", "fil": "fil-ph",
    "vi": "vi-vn", "th": "th-th", "hi": "hi-in", "bn": "bn-bd", "ar": "ar-sa",
    "zh-CN": "zh-cn", "ja": "ja-jp", "ko": "ko-kr",
}  # fmt: skip


def listing_codes():
    """The languages with a listing in store-listing/ (en, pt-BR…)."""
    return sorted(f[:-3] for f in os.listdir(LISTING) if f.endswith(".md"))


def read_listing(code):
    """store-listing/<code>.md: {field: text}, with Features and Search
    terms as lists. "## <field>" starts a field; lines before the first
    one are comments."""
    path = os.path.join(LISTING, f"{code}.md")
    fields, name, lines = {}, None, []
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.rstrip("\n")
            if line.startswith("## "):
                if name:
                    fields[name] = "\n".join(lines).strip()
                name, lines = line[3:].strip(), []
            elif name is not None:
                lines.append(line)
    if name:
        fields[name] = "\n".join(lines).strip()
    rel = os.path.relpath(path, os.path.dirname(LISTING))
    unknown = sorted(set(fields) - set(FIELDS))
    missing = [f for f in FIELDS if not fields.get(f)]
    if unknown or missing:
        sys.exit(f"{rel}: " + "; ".join(
            ([f"unknown fields {unknown}"] if unknown else [])
            + ([f"missing or empty {missing}"] if missing else [])
        ))  # fmt: skip
    features = [l.strip() for l in fields["Features"].splitlines() if l.strip()]
    if any(not l.startswith("- ") for l in features):
        sys.exit(f"{rel}: every line of Features starts with \"- \"")
    fields["Features"] = [l[2:].strip() for l in features]
    fields["Search terms"] = [t.strip() for t in fields["Search terms"].split(",") if t.strip()]
    return fields


def listing_problems(fields):
    """What Partner Center would refuse in a listing."""
    problems = []
    for name, limit in MAX_TEXT.items():
        if len(fields[name]) > limit:
            problems.append(f"{name}: {len(fields[name])} characters (at most {limit})")
    if len(fields["Features"]) > MAX_FEATURES:
        problems.append(f"Features: {len(fields['Features'])} (at most {MAX_FEATURES})")
    for feature in fields["Features"]:
        if len(feature) > MAX_FEATURE:
            problems.append(f"Feature over {MAX_FEATURE} characters: {feature}")
    terms = fields["Search terms"]
    if len(terms) > MAX_TERMS:
        problems.append(f"Search terms: {len(terms)} (at most {MAX_TERMS})")
    for term in terms:
        if len(term) > MAX_TERM:
            problems.append(f"Search term over {MAX_TERM} characters: {term}")
    words = sum(len(t.split()) for t in terms)
    if words > MAX_TERM_WORDS:
        problems.append(f"Search terms: {words} words in all (at most {MAX_TERM_WORDS})")
    return problems


MANIFEST = os.path.normpath(os.path.join(WHATSNEW, "..", "msix", "AppxManifest.xml"))


def check_languages():
    """The game's languages, their listings and the package's languages
    agree: every language has a listing file, and the package declares the
    regions of LISTING_LANGUAGE (the Store shows them as supported)."""
    problems = []
    game, files, known = set(languages()), set(listing_codes()), set(LISTING_LANGUAGE)
    if game != known:
        problems.append(f"LISTING_LANGUAGE and whatsnew/ differ: {sorted(game ^ known)}")
    if files != game:
        problems.append(f"store-listing/ and whatsnew/ differ: {sorted(files ^ game)}")
    with open(MANIFEST, encoding="utf-8") as f:
        declared = {l.lower() for l in re.findall(r'<Resource Language="([^"]+)"', f.read())}
    wanted = set(LISTING_LANGUAGE.values())
    if declared != wanted:
        problems.append(
            f"AppxManifest.xml's <Resources> and LISTING_LANGUAGE differ: {sorted(declared ^ wanted)}"
        )
    return problems


def check_listings(codes=None):
    """Reads and checks store-listing/*.md; stops on the first bad one."""
    listings = {}
    for code in codes or listing_codes():
        if code not in LISTING_LANGUAGE:
            sys.exit(f"store-listing/{code}.md: not one of the game's languages {sorted(LISTING_LANGUAGE)}")
        fields = read_listing(code)
        problems = listing_problems(fields)
        if problems:
            sys.exit(f"store-listing/{code}.md:\n  " + "\n  ".join(problems))
        listings[code] = fields
    return listings


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


def credentials():
    env = {k: os.environ.get(f"MSSTORE_{k}", "") for k in ("TENANT_ID", "CLIENT_ID", "CLIENT_SECRET")}
    missing = [f"MSSTORE_{k}" for k, v in env.items() if not v]
    if missing:
        sys.exit(f"Missing {', '.join(missing)} (the repository's secrets, see docs/MSSTORE.md)")
    return env


def check(args, api=API, token_url=TOKEN_URL):
    """Signs in and reads the game in Partner Center, changing nothing:
    the secrets, the application's role and the Store ID, checked before
    release day."""
    env = credentials()
    mine = check_listings()
    try:
        token = access_token(env["TENANT_ID"], env["CLIENT_ID"], env["CLIENT_SECRET"], token_url, api)
    except ApiError as e:
        sys.exit(
            f"Signing in failed: {e}\nCheck MSSTORE_TENANT_ID, MSSTORE_CLIENT_ID and "
            "MSSTORE_CLIENT_SECRET (the secret's Value, not its ID, and not expired)."
        )
    print("Signed in to Microsoft Entra")
    app_url = f"{api}/v1.0/my/applications/{args.app_id}"
    try:
        app = request("GET", app_url, token=token)
    except ApiError as e:
        sys.exit(
            f"Reading the game in Partner Center failed: {e}\nIs the application added in "
            "Partner Center (Account settings → User management → Microsoft Entra "
            "applications) with the Manager role, and is MSSTORE_APP_ID the Store ID?"
        )
    print(f"Partner Center: {app.get('primaryName', args.app_id)} ({args.app_id})")
    published = app.get("lastPublishedApplicationSubmission")
    if published:
        sub = request("GET", f"{app_url}/submissions/{published['id']}", token=token)
        listings = sorted(sub.get("listings", {}))
        print(f"  last published submission: {published['id']}")
        for package in sub.get("applicationPackages", []):
            print(f"  package: {package.get('fileName')} ({package.get('version')})")
        print(f"  listing languages ({len(listings)}):")
        covered = set()
        for listing in listings:
            code = whatsnew_for(listing)
            covered.add(code)
            texts = f"texts from store-listing/{code}.md" if code in mine else "texts kept as they are"
            print(f"    {listing}: {texts}, notes from whatsnew/{code or 'en'}.md")
        missing = [c for c in mine if c not in covered]
        if missing:
            print(f"  languages the next submission adds ({len(missing)}):")
            for code in missing:
                shots, lang = screenshots(code)
                note = "" if lang == code else " (English screenshots for now)"
                print(f"    {LISTING_LANGUAGE[code]}: from store-listing/{code}.md{note}")
    pending = app.get("pendingApplicationSubmission")
    if pending:
        print(
            f"  a submission is in progress ({pending['id']}): the next release stops "
            "until it is finished or deleted (or replaced, by hand)"
        )
    print("Ready: a release tag will be sent to the Store")


ART = os.path.normpath(os.path.join(LISTING, "..", "store-art"))
# The Store logos each listing gets, by Partner Center's image type: the
# types the English listing has (made by tools/store_art.py), or the poster
# and the box art when it has none of them.
LOGOS = {
    "StoreLogo9x16": "poster-1440x2160.png",
    "StoreLogoSquare": "box-2160x2160.png",
    "Icon": "tile-300x300.png",
    "PromotionalArt16x9": "hero-3840x2160.png",
    "XboxBrandedKeyArt": "xbox-key-art-584x800.png",
    "XboxTitledHeroArt": "xbox-hero-titled-1920x1080.png",
    "XboxFeaturedPromotionalArt": "xbox-promo-square-1080x1080.png",
}
MAX_SCREENSHOTS = 10


def screenshots(code):
    """store-art/screenshots/<code>/*.png in order (English until the
    language has its own), and the language they are in."""
    for lang in (code, "en"):
        folder = os.path.join(ART, "screenshots", lang)
        if os.path.isdir(folder):
            shots = sorted(f for f in os.listdir(folder) if f.endswith(".png"))
            if shots:
                return [os.path.join(folder, f) for f in shots[:MAX_SCREENSHOTS]], lang
    sys.exit("store-art/screenshots/en has no screenshots")


def logo_types(reference):
    used = [i.get("imageType") for i in reference.get("images", []) if i.get("fileStatus") != "PendingDelete"]
    types = [t for t in LOGOS if t in used]
    return types or ["StoreLogo9x16", "StoreLogoSquare"]


def set_images(key, base, code, types, files):
    """The listing's screenshots and logos from the repository: the ones of
    those types it had are removed, the new ones go in the upload."""
    managed = {"Screenshot", *types}
    for image in base.get("images", []):
        if image.get("imageType") in managed:
            image["fileStatus"] = "PendingDelete"
    shots, lang = screenshots(code)
    wanted = [(path, "Screenshot") for path in shots]
    wanted += [(os.path.join(ART, LOGOS[t]), t) for t in types]
    for path, kind in wanted:
        name = f"Images/{key}/{kind}-{os.path.basename(path)}"
        files[name] = path
        base.setdefault("images", []).append(
            {"fileName": name, "fileStatus": "PendingUpload", "imageType": kind, "description": ""}
        )
    note = "" if lang == code else " (in English: none in this language yet)"
    return f"{len(shots)} screenshots{note}, {len(types)} logos"


def set_texts(base, fields):
    base["title"] = fields["Title"]
    base["shortDescription"] = fields["Short description"]
    base["description"] = fields["Description"]
    base["features"] = fields["Features"]
    base["keywords"] = fields["Search terms"]
    base["licenseTerms"] = fields["License terms"]


def apply_listings(sub, listings, notes, all_images):
    """Every language of store-listing/ in the submission: the texts (and,
    with `notes`, "What's new") of the listings it has, and a new listing
    with its images for each language it lacks. With `all_images`, every
    listing's screenshots and logos are uploaded again too. Returns the
    files to upload: {path in the zip: file}."""
    files = {}
    existing = sub.setdefault("listings", {})
    reference = (existing.get("en-us") or next(iter(existing.values()), {})).get("baseListing", {})
    types = logo_types(reference)
    covered = set()
    for key, content in existing.items():
        code = whatsnew_for(key)
        base = content.setdefault("baseListing", {})
        line = []
        if code in listings:
            covered.add(code)
            set_texts(base, listings[code])
            line.append(f"texts from store-listing/{code}.md")
        if notes is not None:
            note = notes.get(code or "en")
            if note:
                base["releaseNotes"] = note
                line.append(f"notes from whatsnew/{code or 'en'}.md")
        if all_images and code in listings:
            line.append(set_images(key, base, code, types, files))
        print(f"  {key}: " + (", ".join(line) or "unchanged (no file for it in store-listing/)"))
    for code, fields in listings.items():
        if code in covered:
            continue
        key = LISTING_LANGUAGE[code]
        base = {
            "copyrightAndTrademarkInfo": reference.get("copyrightAndTrademarkInfo", ""),
            "recommendedHardware": reference.get("recommendedHardware", []),
            "minimumHardware": reference.get("minimumHardware", []),
            "images": [],
        }
        set_texts(base, fields)
        if notes and notes.get(code):
            base["releaseNotes"] = notes[code]
        existing[key] = {"baseListing": base, "platformOverrides": {}}
        print(f"  {key}: NEW listing, texts from store-listing/{code}.md, " + set_images(key, base, code, types, files))
    return files


def zipped(files):
    """The files in a zip, at the paths the submission names them by."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for name, path in files.items():
            kind = zipfile.ZIP_STORED if path.endswith(".png") else zipfile.ZIP_DEFLATED
            z.write(path, name, compress_type=kind)
    return buf.getvalue()


def published_version(sub):
    """The version of the package in the submission (0.12.3.0 → 0.12.3)."""
    for package in sub.get("applicationPackages", []):
        version = package.get("version") or ""
        if package.get("fileStatus") != "PendingDelete" and version:
            parts = version.split(".")
            return ".".join(parts[:3]) if len(parts) == 4 and parts[3] == "0" else version
    return None


def submit(args, api=API, token_url=TOKEN_URL, poll_every=30):
    """A new submission, a copy of the last published one, with the
    repository's listings and either a new package and its version's notes
    (`submit`, on a release) or the screenshots and logos of every listing
    uploaded again (`listing`, by hand)."""
    env = credentials()
    package = getattr(args, "package", None)
    if package and not os.path.isfile(package):
        sys.exit(f"{package}: no such package")

    # Everything read and checked first: a mistake stops here, before
    # anything changes in Partner Center.
    listings = check_listings()
    notes = None
    if package:
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
        if not package:
            # New listings get the notes of the version in the Store.
            version = published_version(sub)
            known = {v for v, _ in whatsnew_notes("en")}
            if version in known:
                notes = {code: release_notes(code, version) for code in languages()}
                print(f"  the Store has {version}: new listings get its notes")
        files = apply_listings(sub, listings, notes if package else None, all_images=not package)
        if not package and notes:
            # Only for the listings made now; the others keep theirs.
            for key, content in sub["listings"].items():
                base = content["baseListing"]
                if not base.get("releaseNotes"):
                    base["releaseNotes"] = notes.get(whatsnew_for(key) or "en", "")

        if package:
            name = os.path.basename(package)
            for old in sub.get("applicationPackages", []):
                if old.get("fileStatus") != "PendingDelete":
                    print(f"  package {old.get('fileName')} ({old.get('version')}): removed")
                old["fileStatus"] = "PendingDelete"
            sub.setdefault("applicationPackages", []).append(
                {
                    "fileName": name,
                    "fileStatus": "PendingUpload",
                    "minimumDirectXVersion": "None",
                    "minimumSystemRam": "None",
                }
            )
            files[name] = package
            print(f"  package {name}: added")

        sub = request("PUT", sub_url, sub, token=token)
        if files:
            print(f"Uploading {len(files)} files")
            upload(sub["fileUploadUrl"], zipped(files))
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

    texts = sub.add_parser("listings", help="check store-listing/*.md and print their sizes")
    texts.add_argument("--lang", help="only this language (pt-BR, en…)")

    test = sub.add_parser("check", help="sign in and read the game in Partner Center (changes nothing)")
    test.add_argument("--app-id", required=True, help="the Store ID (9NZ…)")

    page = sub.add_parser(
        "listing",
        help="update the Store page from store-listing/ and store-art/ (texts, screenshots, logos)",
    )
    page.add_argument("--app-id", required=True, help="the Store ID (9NZ…)")
    page.add_argument(
        "--replace-pending",
        action="store_true",
        help="delete a submission already in progress in Partner Center",
    )
    page.add_argument("--timeout", type=int, default=60, help="minutes to wait for Partner Center")

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
    if args.command == "listings":
        problems = [] if args.lang else check_languages()
        for code, fields in check_listings([args.lang] if args.lang else None).items():
            terms = fields["Search terms"]
            print(
                f"{code:6} {LISTING_LANGUAGE[code]:6}  short {len(fields['Short description']):4}"
                f"  description {len(fields['Description']):5}  features {len(fields['Features']):2}"
                f" (longest {max(map(len, fields['Features'])):3})  search terms {len(terms)}"
                f" ({sum(len(t.split()) for t in terms)} words)"
            )
        if problems:
            sys.exit("\n".join(problems))
        return
    if args.command == "notes":
        for code in [args.lang] if args.lang else languages():
            text = release_notes(code, args.version)
            print(f"===== {code} ({len(text)} characters)\n{text}\n")
    else:
        try:
            (check if args.command == "check" else submit)(args)
        except ApiError as e:
            sys.exit(str(e))


if __name__ == "__main__":
    main()
