# Store listing (Microsoft Store)

The Store page of every language the game speaks (27) lives in the
repository and is sent to Partner Center by the Microsoft Store workflow
(`tools/msstore_submit.py`, see [MSSTORE.md](MSSTORE.md), "Lançar uma versão"):

| What | Where | Sent |
|---|---|---|
| Title, short description, description, product features, search terms, additional license terms | `store-listing/<language>.md` (`en`, `pt-BR`, `fr`… as in `crates/client/whatsnew/`) | on every release, and by hand with **Update the Store page** |
| "What's new in this version" | `crates/client/whatsnew/<language>.md` | on every release |
| Screenshots (1920 × 1080, in the order of their names, up to 10) | `store-art/screenshots/<language>/` (English for a language without its own) | when a language's listing is made, and by hand with **Update the Store page** |
| Logos (poster, box art, tile, hero) | `store-art/` (`tools/store_art.py`) | likewise |
| Privacy policy, website, support contact | Partner Center → **Properties** (the API ignores them) | by hand |

A listing Partner Center doesn't have yet is made by the next submission.
The package declares the same languages (`<Resources>` in
`crates/client/msix/AppxManifest.xml`), so the Store shows them as
supported.

## A listing file

`"## <field>"` starts a field, its name in English; the text under it is
shown as it is written (blank lines are paragraphs). The fields, all
required: `Title`, `Short description` (up to 1,000 characters),
`Description` (up to 10,000), `Features` (one `- ` line each, up to 20 of
200 characters), `Search terms` (separated by commas, up to 7 of 30
characters and 21 words in all), `License terms`. Check them all (the
workflow does it too):

```bash
python3 tools/msstore_submit.py listings
```

The 24 languages after pt-BR, en and es were translated from English with
the game's own words (`crates/client/lang/`); a native speaker's reading
is welcome, the search terms above all.

**Coin purchases in the Store** depend on the package: they are on only when
it was built with the repository variable `MSSTORE_PURCHASES` set to `on`
(and the server has the `SNOOK_LEGEND_MSSTORE_*` variables and the add-ons
are published; see [MSSTORE.md](MSSTORE.md), "Venda de moedas e VIP pela Store"). The
texts don't mention buying anything: while the Store sale is off, use
them as they are, and the age rating questionnaire shouldn't mention
purchases with money either. Once it is on, add the block below to each
description and tick "in-game purchases" in the questionnaire.

---

## Later: when coin purchases are on

Add this paragraph to each description in `store-listing/` (before the last line; translate it for the other languages), replace
**X** and **Y** with the prices of the cheapest and the dearest coin pack
and **Z** with the price of the month of VIP, as set in Partner Center
(Add-ons → Pricing; drop the sentence about VIP if `vip_month` is not on
sale), change "Grátis." / "Free." /
"Gratis." at the end of the short descriptions to "Grátis, com compras
opcionais de moedas." / "Free, with optional coin purchases." / "Gratis,
con compras opcionales de monedas.", and add a line to "What's new".

**Português:**
COMPRAS NO JOGO
O jogo é grátis. As moedas vêm de jogar (partidas valendo ranking e um bônus por dia jogado) e também podem ser compradas, de forma opcional, em pacotes de R$ X a R$ Y. Elas servem só para itens cosméticos: nada que faça ganhar partidas. O VIP (itens exclusivos e 400 moedas) é vendido um mês por vez, por R$ Z, e não se renova sozinho.

**English:**
IN-GAME PURCHASES
The game is free. Coins come from playing (rated games and a bonus for every day you play) and can also be bought, if you like, in packs from US$ X to US$ Y. They are only for cosmetic items: nothing that wins games. VIP (exclusive items and 400 coins) is sold a month at a time, for US$ Z, and doesn't renew by itself.

**Español:**
COMPRAS EN EL JUEGO
El juego es gratis. Las monedas se ganan jugando (partidas con clasificación y un bono por cada día que juegas) y también se pueden comprar, si quieres, en paquetes de US$ X a US$ Y. Sirven solo para objetos cosméticos: nada que haga ganar partidas. El VIP (artículos exclusivos y 400 monedas) se vende un mes a la vez, por US$ Z, y no se renueva solo.
