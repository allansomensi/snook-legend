# Translating Snook Legend

Snook Legend is a 3D eight-ball pool game (Rust, open source) with an online lounge, a shop of cosmetic items (cloths, cues, chalk, balls), coins, VIP, tournaments, daily (correspondence) games, ranked ladders, achievements, and computer opponents. It plays two rule sets: the "Brazilian rules" (as played in Brazil's bars, with called pockets) and the "official rules" (WPA). It is a polished game; players expect a natural, friendly, concise interface in their own language.

The working folder is `/tmp/claude-0/-home-user-snook-legend/62580145-a1f0-5c7b-86aa-b95263dd03b3/scratchpad/i18n` (called `I18N` below). The game's code is in `/home/user/snook-legend` — **read it, never change it**. Write only inside `I18N/out/<code>/`.

## 1. The interface texts

`I18N/source.tsv` has 2050 lines: `N<TAB>English<TAB>Spanish`. The English is the source; the Spanish column is the existing (good, human-reviewed) translation, there to show the meaning and context of short or ambiguous texts. The Brazilian Portuguese translation is in `/home/user/snook-legend/crates/client/lang/pt-BR.txt` if a second reference helps.

Write the translation as lines `N<TAB>translation` (the same N), in order, into `I18N/out/<code>/ui-01.tsv` (entries 1–300), `ui-02.tsv` (301–600), … `ui-07.tsv` (1801–2050): one Write call per file, so no single output is huge. Read `source.tsv` in parts too (e.g. 300 lines at a time with offset/limit).

Hard rules (the checker enforces them):
- Every `{placeholder}` stays exactly as written (same names, braces included); you may move it within the sentence. Never translate what is inside braces.
- `\n` is a literal backslash-n (a new line in the game): keep the same number of them.
- No tab characters inside a translation; no line breaks.
- Leading/trailing spaces exactly as in the English (a few entries start with a space, like ` · left english`).
- A translation must not start with `#` unless the English does.
- Entries whose English ends in ` (inside a sentence)` are the rules' names as written in the middle of a sentence (e.g. Portuguese "regra brasileira", lower case): translate the name only, never the parenthesis; use the casing your language needs mid-sentence.
- Keep proper names and brands as they are: Snook Legend, VIP, Stripe, Pix, Microsoft Store, Twemoji, flag-icons, iso-codes, DejaVu, Noto, WPA, MIT, CC-BY, LGPL, OFL, PNG, LAN, Wi-Fi, Esc, Enter, Tab, Shift, Ctrl, etc. The computer opponents' names (Tito, Inês, Diego, Amara, Yuri, …) stay as they are.

Quality:
- Short texts are often buttons, tabs or labels with little room: keep them about as short as the English (or the Spanish). Prefer the usual word in games/apps of your language (e.g. "Settings", "Sign in", "Shop", "Friends").
- When a short text is ambiguous ("free", "yours", "Use", "open", "Round"), search the code for it (`grep -rn '"the text"' /home/user/snook-legend/crates/client/src`) to see where it appears, and check the Spanish column.
- Billiards terms: first decide a glossary for your language — the standard terms used by players/commentators of pool in your language (cue ball, the 8-ball / "{n}-ball", break, foul, pocket, called pocket, cushion/rail, cue ball in hand, head string, head spot, foot spot, scratch, solids, stripes, rack, safety, shot clock, english/spin, cue, chalk, cloth, frame) — and use it consistently in every text.
- One register, consistently: the one polished games in your language use to address the player (see the language notes below).
- Numbers come in through placeholders as Western digits; write any numbers in the text with Western digits too.

## 2. The rules and "What's new"

Translate these Markdown files (each paragraph is ONE line, as in the English — keep it so):
- `/home/user/snook-legend/crates/client/rules/brazilian/en.md` → `I18N/out/<code>/rules-brazilian.md`
- `/home/user/snook-legend/crates/client/rules/official/en.md` → `I18N/out/<code>/rules-official.md`
- `/home/user/snook-legend/crates/client/whatsnew/en.md` → `I18N/out/<code>/whatsnew.md` (write it in two or three Write calls if long: e.g. write the first half, then use Bash `cat >>` with a quoted heredoc, or Write a second file and concatenate)

Structure: lines starting with a single `#` at the top are comments (translate them or keep them in English, your choice). `## …` starts a part (rules: translate the part's name) or a version (whatsnew: keep `## 0.8.3` etc. EXACTLY). `### …` is an item title (translate). The text lines under it are translated. The same parts, the same items, in the same order. Use the same glossary as the interface: the rules must name things exactly as the game's screens do. The Spanish versions (`es.md` next to each `en.md`) are good references.

## 3. Check and report

Run `python3 I18N/check.py <code>` and fix every problem it reports, until it prints OK. Then spot-check a sample of your own work for quality (glossary consistency, register, terse labels) and fix what you find.

Your final message (keep it short): "OK" or the checker's last output, the glossary you chose (English → yours, ~15 terms), the register used, and any entries you were unsure about (number + why).
