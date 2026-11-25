# Staff API

The staff portal's calls to the server (`/admin/api`). Every call needs the
staff session's bearer token (`Authorization: Bearer …`); each area below
says which role it needs. Errors answer `{"error": "…"}` with a 4xx status.

## Collection and content

### `GET /admin/api/users/{name}/collection`

Moderators and administrators. Everything the player has, with how and when
they got it: owned items, the items of their achievements, and the
reactions those achievements bring (code `reaction_<code>`, also for
reactions that are not shop items). Free items and those VIP members use
while VIP are not listed. Read only (nothing is written to the audit log).

```json
{
  "username": "ana",
  "items": [
    {"code": "cue_christmas", "name": "Candy cane", "category": "cue",
     "rarity": "epic", "bound": true,
     "origin": {"how": "staff", "reason": "Christmas 2026 winner"},
     "acquired_at": 1798000000},
    {"code": "cloth_blue", "name": "Electric blue", "category": "cloth",
     "rarity": "common", "bound": false,
     "origin": {"how": "bought", "price": 1000}, "acquired_at": 1797000000},
    {"code": "reaction_fire", "name": null, "category": "reaction",
     "rarity": null, "bound": false,
     "origin": {"how": "achievement", "code": "streak_10"}, "acquired_at": 1796000000}
  ]
}
```

`origin.how` is one of `bought` (`price`), `gift` (`from`), `staff`
(`reason`), `tournament` (`name`, `place`), `achievement` (`code`),
`auction` (`price`), `vip_month` (`month`, `YYYY-MM`), `event` (`name`),
`season` (`name`), `welcome_pack`, `unknown`. `acquired_at`: Unix seconds,
0 when not known. Items owned before 0.12 have their origin worked out from
how they were added (bought, a present, the welcome pack, the staff).

### Changes to existing calls

- `POST /admin/api/users/{name}/item` (administrators) also gives items
  only won (`"how": "earned"` in the catalogue). The `reason` is kept as the
  item's origin and shown to the player under it in the collection.
- `GET /admin/api/economy`: each entry of `catalog` has `"rarity"`
  (`common`, `rare`, `epic`, `legendary`) and `"bound"` (true: can't be
  given nor auctioned).

