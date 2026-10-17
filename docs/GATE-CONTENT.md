# GATE CONTENT - commander gates of the Pilgrim Road (M8a companion)

Status: CONTENT READY 2026-10-17. Companion to docs/OVERWORLD.md
(scripted-gate design) and docs/HISTORICAL-FOES.md sections 4-6 (roster,
register canon - the register rules there BIND this file). Page records the
Kilij Arslan gate (screen 9, battle_ref 101) as shipped, then authors the
remaining commander gates so the landing is mechanical: one row of the map
table = one `ScreenDef` edit + one table row of text.

Campaign order (per OVERWORLD section 4, the road runs Europe -> Anatolia ->
Syria -> the Kingdom): Rum Seljuk pressure in Anatolia first (120, done),
the Zengid host on the Antiochene marches mid (121), the Egypt roads
(122), the Hattin wings (124 then 125) at the Horns, and Saladin (123) as
the finale. This is the escalation the trail's geography already encodes.

## 0. Placement map (landing diff reference)

| screen | region | gate / battle_ref | escorts (2) | boss | boss level offset | status |
|---|---|---|---|---|---|---|
| 9 | 3 Anatolia | Dorylaeum road / 101 | 108, 109 | 120 Kilij Arslan I | +2 | SHIPPED (reference row) |
| 13 | 4 The Coasts | Harim marches / 102 | 114, 116 | 121 Nur ad-Din | +2 | authored below |
| 16 | 5 Roads of Egypt | Egypt roads / 103 | 110, 116 | 122 Asad al-Din Shirkuh | +2 | authored below |
| 19 | 6 Horns of Hattin | the left horn / 104 | 116, 114 | 124 Gokbori | +2 | authored below |
| 20 | 6 Horns of Hattin | the right horn / 105 | 116, 117 | 125 Taqi al-Din Umar | +3 | authored below |
| 25 | 7 The Holy Land | the King's road / 108 | 116, 117 | 123 Saladin | +3 | authored below |

Orchestrator notes (data-only adjustments this map implies):

1. Screen 20 currently carries `commander_id: Some(123)`; move it to
   `Some(125)` so both Hattin wings stand in region 6 (the Horns) before
   the finale. Escorts/offsets land per the row above.
2. Screen 23 currently carries `Some(125)` and screen 24 carries
   `Some(122)`: both are M8a-spine stand-ins (client.rs comment marks
   them "pending M8 review"). Clear `scripted_gate` on 23 and 24 (or
   restaff as non-commander ambush refs; either is out of this file's
   scope). With the wings consolidated at screens 19-20, Saladin gates
   only the finale at screen 25.
3. Boss-offset plumbing: the shipped gate path hardcodes `+2` (client.rs
   `FollowUp::Gate`). The +3 rows above require the offset to become
   data (suggest a `boss_level_offset: u8` field on `BattleGate`, default
   2). Escorts keep the shipped `lv - 2` clamp.
4. The escort ids in the map match the existing `gate_escort_mix` arms
   (refs 102, 103, 104, 105, 108) exactly - no mix-table changes needed.

## 1. Gate: Nur ad-Din (121) - screen 13, The Coasts, ref 102

**Placement (one line).** The road descending from the Anatolian plateau
toward the Antiochene marches is the ground the Harim field army fought
on in 1164; the Zengid gate belongs here, between Anatolia and Egypt.

**Encounter.** escorts `[114 Kurdish Ghazw Rider, 116 Askari Alamdar]` +
boss `121` at +2. Rationale: Nur ad-Din's host was ghazw tribal cavalry
around the atabeg's drilled askar and its banners (HF section 2); the
banner-bearer escort is the rally mechanics of a disciplined host.

**Pre-battle text.**
> The ridge holds against you: spears in good order, the atabeg's banner
> above them. Nur ad-Din Mahmud ibn Zengi has ridden out to Harim's
> ground, and the road will not open while he stands it.

**Post-battle (victory).**
> The askar breaks in good order and withdraws toward Damascus; we count
> our dead and give thanks. The chroniclers of both hosts call his
> justice without reproach; the road is open, and a worthy man rides
> from it unharmed.

Codex: the Roll of Worthy Adversaries page for 121 is already written in
HF section 5 ("Nur ad-Din Mahmud b. Zengi"): use it verbatim; no new lore
line needed.

**Defeat (`where_fell` family, printed with screen/day by the client).**
> fell in battle upon the pilgrim road, at the Harim marches before the
> atabeg Nur ad-Din

## 2. Gate: Asad al-Din Shirkuh (122) - screen 16, Roads of Egypt, ref 103

**Placement.** Region 5 is named for his theater: Shirkuh marched on
Egypt three times (1164, 1167, 1169) against Frankish intervention; the
heaviest supply-drain stretch is where a boss of marches, not set
pieces, belongs.

**Encounter.** escorts `[110 Sudani Shieldbearer, 116 Askari Alamdar]` +
boss `122` at +2. Rationale: the Egypt campaigns were fought through and
around the Fatimid host - its great-shield foot held the cities Shirkuh
took and the columns he outmarched; the Alamdar is the column's core.

**Pre-battle text.**
> A column upon the march, dust to the horizon - it has outwalked every
> rumor of itself. Asad al-Din Shirkuh, thrice the master of Egypt's
> roads, turns his host across yours.

**Post-battle (victory).**
> His tired men scatter into the sands and he leads them without
> looking back; the road is ours, though the march has cost us dear.
> Within the year the chronicle records his death in Egypt - in the very
> seat he won - and the vizierate passing to his nephew Yusuf, called
> Salah al-Din.

Codex: HF section 5 already carries the 122 page ("Asad al-Din Shirkuh"):
use it verbatim; no new lore line needed.

**Defeat.**
> fell in battle upon the pilgrim road, upon the roads of Egypt before
> Asad al-Din Shirkuh

## 3. Gate: Gokbori (124) - screen 19, Horns of Hattin, ref 104

**Placement.** Cluster check: the Horns of Hattin (region 6) is the
gauntlet stretch; Saladin's two wing commanders at Hattin (both
attested, HF section 5 sub-boss texture) gate its last two screens back
to back, the pickets thickening under the dry hills.

**Encounter.** escorts `[116 Askari Alamdar, 114 Kurdish Ghazw Rider]` +
boss `124` at +2. Rationale: Gokbori - Muzaffar al-Din, lord of Harran -
was himself a Kurdish commander; the left wing of Hattin was tribal
Kurdish horse around regimental banners.

**Pre-battle text.**
> Under the waterless hills the King's road narrows, and horsemen dark
> as the ridge-line wait where it turns. Muzaffar al-Din Gokbori holds
> the left horn. There is no way around.

**Post-battle (victory).**
> The horn is turned; Gokbori draws his riders off the hill in good
> order, his banner still uncut, and the company gasps on toward the
> springs. History remembers him at Hattin a victor; here the road
> remembers an honorable man outfought, and let go.

**Defeat.**
> fell in battle upon the pilgrim road, beneath the Horns of Hattin
> before Muzaffar al-Din Gokbori

## 4. Gate: Taqi al-Din Umar (125) - screen 20, Horns of Hattin, ref 105

**Placement.** Second of the paired horn gates (see gate 124). Taqi
al-Din Umar, Saladin's nephew, led the right wing at Hattin; the paired
placement makes the Horns what the chronicle says they were.

**Encounter.** escorts `[116 Askari Alamdar, 117 Muqaddam of the Tulb]` +
boss `125` at +3. Rationale: the right horn is the harder gate - a
tulb officer beneath the wing commander, the regimental machinery of
Hattin's closing jaws. Offset +3 marks the escalation between the horns.

**Pre-battle text.**
> The road turns once more, and the horn turns with it: a wall of
> lances under a master's eye. Taqi al-Din Umar holds the right hand of
> Hattin's field. None pass the Horns who will not fight both.

**Post-battle (victory).**
> The second horn gives way at the ninth hour; the nephew withdraws to
> his uncle's command, fighting as he goes. The company passes between
> the Horns with empty water-skins and whole souls, and does not boast.

**Defeat.**
> fell in battle upon the pilgrim road, between the Horns of Hattin
> before Taqi al-Din Umar

## 5. Gate: Saladin (123) - screen 25, The Holy Land, ref 108

**Placement.** The finale screen of the region named for the destination:
the King's road's last gate before the City. The 1187/1191/1192 triptych
(Hattin, Arsuf, Jaffa) is compressed into one battle of record here, per
OVERWORLD's 40-screen budget; HF section 7's fuller three-battle finale
is the v2.x (c) lane.

**Encounter.** escorts `[116 Askari Alamdar, 117 Muqaddam of the Tulb]` +
boss `123` at +3 (capped at MAX_LEVEL by the shipped clamp). Rationale:
the Ayyubid guard - banner and tulb officer of the sultan's own askar.

**Pre-battle text.**
> At the last ridge the whole host stands, and above it a man the Latin
> chronicles themselves honor: Salah al-Din Yusuf ibn Ayyub, uniter of
> Egypt and Syria, between you and the City. He keeps his safe-conducts
> and he keeps his word; he will not give you the road. Kneel; pray;
> then rise and take the field.

**Post-battle (victory).**
> The host of Egypt and Syria yields the field, and its lord withdraws
> with his banners unbroken and his honor entire, as the chronicles of
> our own side confess he ever kept it. The road lies open to the City.
> Let no man boast over him in the telling: he was the worthiest foe
> this war produced, and the chronicler who is honest sets his virtue
> beside our own men's, that the rest of the story may be believed.

Codex: HF section 5 already carries the 123 page ("Saladin"): use it
verbatim (Scott [R] excluded per HF section 11); no new lore line needed.

**Defeat.**
> fell in battle upon the pilgrim road, upon the King's road before
> Salah al-Din Yusuf ibn Ayyub

---

## Register self-review (against HF section 6)

1. Redeemable men: every victory text routes the commander, none slants
   him; the routing text for Saladin follows HF section 5's own wording
   (honor attested on both sides).
2. Naming: narrator voice uses Rum/Zengid/Ayyubid office names; no
   unattributed exonym appears. No "Saracen" anywhere in this file.
3. Theological firmness without slander: no caricature, no treachery-
   by-birth, no horde framing; the only doctrinal sentence (pre-battle
   123) is a kneel-and-pray, not a sneer. The game's own side is plainly
   charged with its dead (122, 124 victory texts).
4. Sin-depth: untouched here - the gradient is visual, not prose.
5. Codex: all Roll pages deferred to HF section 5's existing commander
   entries by reference; commanders never convert, so no Testimonies
   hooks are implied.
6. Historical claims asserted: Harim 1164 (Zengid victory, attested);
   Shirkuh's three Egypt marches and death within two months of the
   vizierate (HF section 11 sources); Gokbori and Taqi al-Din Umar as
   the Hattin wings (HF section 11, Four Accounts); Saladin's
   safe-conducts honor (HF section 5). Dates/deaths for 121, 122, 123
   per HF section 5 entries.

[VERIFY] none - no claim in this file extends past HF sections 4, 5,
and 11.
