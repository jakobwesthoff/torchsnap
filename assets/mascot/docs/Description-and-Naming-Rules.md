## How Mascot Descriptions Work

### The Description Philosophy

Snappy mascot descriptions follow a strict "wink wink, do you get it?" style.
They are **not** visual descriptions of what the mascot looks like. Instead,
they are clever, humorous references that let fans immediately recognize the
pop culture character being referenced — without ever naming the copyrighted IP
directly.

The reference **is** the description. If the reference is good enough, you can
picture the costume without anyone spelling out "red suit with gold trim" or
"black leather jacket with sunglasses."

### Style Rules

1. **Never name the IP directly** — no character names, no franchise names, no
   studio names in the description value itself. The key name and any internal
   comments can reference the IP, but the user-facing description text must not.

2. **Keep it short** — one sentence, often structured as a brief setup followed
   by an em dash and a punchline. Aim for punchy, not verbose.

3. **Use iconic quotes, traits, or situations** — lean on what makes the
   character instantly recognizable: catchphrases, signature items, famous
   scenes, or well-known lore.

4. **Avoid dry visual descriptions** — don't list clothing colours, accessories,
   or poses. That's what the image is for. The text should evoke the character's
   personality, not their wardrobe.

5. **Humorous tone** — the descriptions should have a playful, tongue-in-cheek
   quality. Think easter egg, not encyclopedia entry.

6. **Start with "Snappy"** — every description begins with the mascot name.

7. **Tie the punchline to the launcher** — whenever possible, bend the
   reference toward launching, commands, hotkeys, or apps. That's what makes
   the joke land *for this product* instead of being a generic costume caption.

### Examples (from the current mascots.json)

```
"black-coat-shades": "Snappy after taking the red pill -- there is no launcher."
→ This is Neo from The Matrix. Two iconic references combined: the red pill
  choice and "there is no spoon," flipped to fit the launcher.

"iron-circuit": "Snappy in glowing power armor -- built in a cave, with a box of scraps."
→ This is Iron Man. The "cave with a box of scraps" line is iconic from the
  first film. No need to name the suit; the quote does all the work.

"jade-giant-rage": "Snappy gone jade and furious -- you wouldn't like this owl when it's angry."
→ This is The Hulk. The catchphrase is unmistakable, and swapping "me" for
  "this owl" threads the mascot into the line.

"battle-scarred-cyborg": "Snappy as a battle-damaged cyborg -- it'll be back. With a launch command."
→ This is The Terminator. "I'll be back" is the most iconic line; appending
  "With a launch command" ties it to the app without diluting the reference.

"chrome-centurion": "Snappy as a chrome centurion with a sweeping red eye -- by your command, the app will launch."
→ This is a Cylon from Battlestar Galactica. "By your command" is the
  catchphrase; "the app will launch" closes the loop with the product.

"stone-gargoyle": "Snappy turned to stone -- one thousand years of stony sleep can make an owl grumpy."
→ This is from the Gargoyles animated series. The stone-by-day premise and
  the thousand-year sleep are the show's setup.

"hooded-force-wanderer": "Snappy as a hooded desert hermit -- these aren't the apps you're looking for."
→ This is Obi-Wan / a Jedi from Star Wars. "These aren't the droids you're
  looking for" maps cleanly onto "apps" without changing the rhythm of the
  line.

"oversized-dark-helmet": "Snappy in an absurdly oversized dark helmet -- surrounded by apps, and this launch has gone to plaid."
→ This is Dark Helmet from Spaceballs. "Gone to plaid" (ludicrous speed) is
  the signature gag; "surrounded by apps" nods to "surrounded by assholes."

"whip-and-torch-explorer": "Snappy on a relic hunt -- apps. Why did it have to be apps?"
→ This is Indiana Jones. "Snakes. Why did it have to be snakes?" is the iconic
  line — the swap from "snakes" to "apps" is the entire joke.

"astronaut": "Snappy suited up for orbit -- one small launch for an owl, one giant leap for owl-kind."
→ This is the Apollo 11 / Neil Armstrong reference. "One small step… one giant
  leap for mankind" becomes launch-flavoured and owl-flavoured in one move.
```

### The Evolution Process

The style was developed iteratively:

1. **First pass**: Replaced explicit IP names (e.g. "Iron Man-style power
   armor") with generic descriptions. This was too bland.

2. **Second pass**: Added recognizable references as appended punchlines. Better,
   but the descriptions were still too long and visually descriptive.

3. **Third pass (final style)**: Stripped out visual descriptions entirely and
   let the reference carry the whole description. The reference alone should tell
   you what Snappy looks like — "built in a cave, with a box of scraps"
   immediately conjures red-and-gold armour and a glowing chest reactor without
   anyone describing them.

### Special Case: Doctor Who Incarnations

The Doctor Who series required hiding the incarnation number in idiomatic
expressions rather than saying "the Nth incarnation":

- 1st → "the one and only original" (contains "one")
- 2nd → "playing second fiddle" / "second to none" (idioms)
- 3rd → "third time's the charm" / "third time in exile" (idiom)
- 4th → "four jelly babies short of a full bag" (character trait + number)
- 5th → "always taking five" / "the celery has five known uses" (idiom + trait)
- 6th → "a sixth sense for clashing colours" (idiom + trait)
- 7th → "always seven moves ahead" (trait + number)
- 8th → "behind the eight ball" (idiom)
- 9th → "nine hundred years old" (lore reference, contains "nine")
- 10th → "a perfect ten" (idiom)
- 11th → "turning it up to eleven" (Spinal Tap reference layered in)
- 12th → "striking twelve" / "twelve centuries of disapproval" (clock / lore)
- 13th → "a baker's dozen" / "thirteen items in the kit bag" (idiom / lore)
- War Doctor → "the unnumbered face" (lore reference)

All share the framing "an ever-changing time traveller" to connect them as a
series without naming the show.

## NSFW vs. Non-NSFW Variants

Every mascot in `mascots.json` carries an `"nsfw": true | false` flag, and
any NSFW mascot also has the suffix `-nsfw` baked into both its key and its
image filename (e.g. `snappy-battle-scarred-cyborg-nsfw-1024.png`). The flag
lets the app hide edgier variants behind a user setting.

### What "NSFW" means here

Snappy is a chibi owl — nothing is sexually explicit. In this project
"NSFW" is really "edgy / not safe for the office" and almost always means
one of:

- **A weapon is visible** — a firearm, blade, blaster, bladed glove,
  machete, proton pack with streams crossed, etc.
- **The character is canonically violent or horror-coded** — a slasher-
  film killer, an alien hunter, a zombie-adjacent revenant, a vampire
  with fangs on display.
- **Visible gore or menace** — blood on a blade, battle damage that
  reads as injury rather than cool.

A costume alone is never enough to be NSFW. The Hulk-style rage pose
(`jade-giant-rage`) or the Terminator costume without the gun
(`battle-scarred-cyborg`) both stay safe. The moment Snappy picks up
the gun, the same character flips to `-nsfw`.

### The pairing pattern

Many characters exist in **both** forms — the non-NSFW entry is the
"costume only" take, and the NSFW entry is the same Snappy with the
character's signature weapon or menace added:

- `battle-scarred-cyborg` (cyborg owl in a leather jacket, no weapon)
  vs. `battle-scarred-cyborg-nsfw` (same owl, now gripping a flaming
  pistol) — the difference is a single prop.
- `chrome-centurion` (armored chrome-centurion pose) vs.
  `chrome-centurion-nsfw` (same armor, now dual-wielding blasters).
- `hockey-mask-slasher` (hockey mask, stalking pose) vs.
  `hockey-mask-slasher-nsfw` (same mask, now holding a bloodied blade).
- `scarlet-merc` / `scarlet-merc-nsfw`, `engage-crimson` /
  `engage-crimson-nsfw`, `dashing-captain` / `dashing-captain-nsfw`,
  `multipass-cabbie` / `multipass-cabbie-nsfw`, `remember-remember` /
  `remember-remember-nsfw`, and others follow the same pattern.

When both variants exist, the descriptions typically use the **same
reference family** but lean harder into the violent / iconic line for
the NSFW version (e.g. the non-NSFW Terminator says "it'll be back.
With a launch command.", while the NSFW version escalates to "I need
your launcher, your commands, and your torch.").

### Standalone NSFW-only entries

Some characters cannot meaningfully exist without the element that
makes them NSFW — the weapon, gauntlet, or menace **is** the
character. These ship NSFW-only, with no sanitized counterpart:

- `infinity-gauntlet-titan-nsfw` — Thanos without the Infinity
  Gauntlet isn't Thanos.
- `hooded-blade-bearer-nsfw` — the Assassin's Creed reference is the
  hidden blade; remove it and nothing's left.
- `pirate-nsfw`, `ninja-nsfw` — a pirate without a cutlass or a ninja
  without a blade would be a costume, not the archetype.
- `dreadlock-hunter-nsfw` / `-2-nsfw` — the Predator is defined by the
  shoulder cannon and trophy-taking.
- `flame-paladin-nsfw`, `forest-tunic-hero-nsfw`,
  `desert-force-wielder-nsfw` / `-2-nsfw`, `daywalker-blade-nsfw`,
  `adamantium-claw-berserker-nsfw`, `striped-sweater-claw-nsfw`,
  `triangle-ops-nsfw` / `-2-nsfw` — same principle: the iconic weapon
  or deadly signature is load-bearing for the reference.

A handful of references also ship **multiple NSFW variants** off a
single non-NSFW base (e.g. `proton-pack-buster` plus
`proton-pack-buster-nsfw` through `-4-nsfw`), each riffing on a
different escalation of the same character.

### Rules when adding a new mascot

1. **Decide once, suffix everywhere** — if an entry is NSFW, its key,
   its image filename, and its `nsfw` flag must agree. No mixed state.
2. **Weapon in frame ⇒ NSFW.** If the render has a visible weapon,
   blood, or overt menace, mark it NSFW. Borderline props (torches,
   lanterns, tools, staves, sonic screwdrivers, wands, a pirate's
   multipass, cartoon banana "guns") are **not** weapons and stay
   non-NSFW.
3. **Prefer pairs where the character allows it.** If the underlying
   IP has both a "costume" read and a "weaponized" read, produce both
   — it gives users the choice and maximises the mascot set.
4. **Keep the description style identical across both variants.** Same
   "wink wink" rules apply; the NSFW description is just free to lean
   into the character's edgier catchphrase.
