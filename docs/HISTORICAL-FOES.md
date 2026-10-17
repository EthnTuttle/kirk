# HISTORICAL FOES - design research for "The Road East"

Status: REINSTATED 2026-10-16. The owner restored the historical lane as
the main field-adversary theater and added the CONVERSION mechanic as the
collection grammar: defeated adversaries are issued a Call to Repentance
(contested by Obstinacy; spec in docs/CONVERSION.md); refusers' souls are
released unto God. The Seven Deadly Sins remain the allegorical boss tier
alongside the commanders in section 5. The game's voice names the foe's
god a false god — grounded in attested doctrinal conflict per
docs/DOCTRINAL-SINS.md, never caricature. Section 6 below is REVISED to
the new register (preamble plus rules 1-4 rewritten 2026-10-16); the open
questions in section 10 resolve per the owner's direction: conversion of
common archetypes is core (q5 yes, baptised join the roster; commanders
never convert); the 1187 Jerusalem surrender stays codex prose only (q6);
the historical-figures ban in PLAN.md section 1 is amended (q1 yes).
Sections 1-5 and 7-11 (factions, kit, archetypes, commanders, seams,
art, roadmap, sources) stand as written.


## 1. Era scope: 1095-1192 (recommended)

Band: Council of Clermont (1095) to the Truce of Jaffa (Sept 1192) - the
First Crusade's Anatolian passage and Antioch, the Zengid consolidation,
and the Hattin/Arsuf/Jaffa triptych of the Third. Our warrior-monk
champions already battle timeless allegory, so a strictly 12th-c. foe set
adds no anachronism v1.0 lacks; and Muslim military kit barely changed
across the window, so ONE art recipe serves the whole band. Mamluks of the
Bahri sultanate (1250+) are OUT: Mansura and Ain Jalut belong to a
post-kingdom parity world; Baybars waits for a hypothetical v3.

## 2. Faction survey

- **Seljuk Sultanate of Rum (Anatolia; ours 1096-1108).** The crusaders'
  first organized enemy: nearly all mounted - Turcoman tribal horse archers
  stiffened by the sultan's household. At Dorylaeum (1 July 1097) modern
  estimates give Kilij Arslan I only ~6-7,000 cavalry, allied with the
  Danishmend emir; the chroniclers' 150,000-360,000 are dismissed as
  impossible. Battlefield read: quick light archer swarms who never accept
  the charge.
- **Fatimid Egypt (969-1171).** The southern enemy at Ascalon. Distinctive
  look: massed black African (Sudani) spearmen and archers behind enormous
  hide shields, Armenian bowmen, Daylamite axemen, Turkish ghulam cavalry -
  al-Maqrizi's plausible late-1160s figures: 10,000 horse, 40,000 black
  infantry. Saladin disbanded it in 1169-71. Battlefield read: the anvil.
- **Zengids of Syria (Aleppo/Mosul, 1127-1183).** Nur ad-Din's atabeg state
  reunified Muslim Syria. Army: Turcoman horse archers, Kurdish spear-armed
  horsemen, professional ghulam cavalry, Bedouin auxiliaries, siege
  engineers; the ruler's askar numbered only 1,000-3,000, swollen by
  contingents. Battlefield read: the balanced, well-drilled host.
- **Ayyubids (Egypt + Syria, 1171-c.1250; ours to 1193).** Saladin's
  inheritance: Kurdish tribal cavalry and Turkic ghulam/mamluk heavy horse,
  mounted archers, Turcoman/Bedouin auxiliaries, Arab infantry - tulbs of
  70-200 grouped into ~1,000-man regiments; ~12,000 regular cavalry rode at
  Hattin in an army chroniclers put at 30-40,000. Battlefield read:
  Hattin's water-denial and arrow storms; Arsuf shows its failure against
  a host that will not break formation.
- **Mamluk sultanate:** out of scope (section 1).

## 3. Arms and armor canon (feeds the art recipe; anti-cliche)

Well-attested for 1095-1192: maille hauberks (zardiyya); iron lamellar
corselets (jawshan) over a padded coat, sometimes under a surcoat (burda);
the maille-lined kazaghand; conical khud helmets with maille aventails;
small rounded baydah ("egg") helmets; turbans wound over small caps on
light troops (a helmet CAST in bulbous turban form is Ilkhanid/Safavid,
14th-16th c. - banned); large round or kite-shaped hide shields; short
two-handed lances; straight double-edged saif as the standard sidearm;
composite bows for horse archers; Daylamite axes and zhupin javelins; maces
for armored men.

FLAGS - orientalist cliches the recipe must refuse: (a) the pop-culture
"scimitar" - a deep-curved saber, worse paired with a forward-curved
yataghan - is Ottoman work of the 15th-16th c. and later; our sword is the
straight saif (sabers existed among Turkic elites from the 9th c. but were
a minority arm in the 12th-c. Levant). (b) Barbary-pirate robes and sashes;
our foes are professionals in khaftan, maille, lamellar. (c) Camels as
combat mounts (baggage animals; Saladin's Hattin columns used them as
transport). (d) Scowling caricature faces - see section 6.

## 4. Archetype roster proposal (ids 108-117; commanders 120+, section 5)

These are honorable men, not sins. **Typing proposal: add `Typing::Valor`**
- a third axis arm meaning "armed honor", off the virtue/sin chart at flat
1.0x both ways. Virtue-typing them would wrongly claim our champions'
holiness for the foe; sin-typing them is a slander the game refuses; the
virtue wheel would grade their orthodoxy mechanically. Balance instead via
stats and kits, proved by `crusade-sim fuzz` (levels 5/20/40; target 40-60%
win rate inside the champion band - the C2 Sabina method). Fallback = open
question 2. Stat bands: per-stat 20..=130 (existing guard); archetype
totals 300-380; commanders 390-420 (top of the champion band 300..=420).
Kits cite existing rites 1-42 plus PROPOSED add-only rites 43+ ([NEW]).

| id | Name (ASCII) | Basis (unit, era, faction) | Arms / armor (art recipe) | Stats (total) | Kit sketch |
|---|---|---|---|---|---|
| 108 | Turcoman Bowrider | horse-archer screen; Rum Seljuks et al., 1096-1192 | unarmored; fur kalpak cap over wrapped turban, belted khaftan, riding boots, composite bow drawn, small round hide shield slung | 40/55/35/40/45/85 (300) | [NEW "Wheeling Shots" multi-hit], Measured Blow, [NEW "Dust of the Plain" evade] |
| 109 | Ghulam Lancer | household slave-soldier heavy cavalry; Zengid/Ayyubid askars | maille hauberk over khaftan, conical khud helm + aventail, kite shield, couched two-handed lance, saif at belt | 65/85/30/70/50/45 (345) | Sundering Blow, Bulwark's Stand, [NEW "Couched Lance" high-crit] |
| 110 | Sudani Shieldbearer | black African great-shield spearman; Fatimid foot to 1169 | towering full-body hide shield, short thrusting spear, plain white tunic, head-cloth; no metal | 80/55/25/85/45/30 (320) | Steady Ward, [NEW "Testudo of Hides" guard], Measured Blow |
| 111 | Armenian Bowman | foot archer; Fatimid + garrison service, 11th-12th c. | open coat, soft cap, composite bow taller than the horseman's, tall quiver, knife only | 55/60/35/50/55/65 (320) | [NEW "Storm of Arrows" multi-hit], [NEW "Volley in Turn"], Steady Ward |
| 112 | Daylamite Axeman | hill-foot shock infantry; Fatimid/Zengid service | two tabar axes (one slung), zhupin javelin, knee-length maille shirt, round shield on back | 70/80/25/65/40/40 (320) | [NEW "Cleave of Axes" high-crit], Measured Blow, [NEW "Zhupin Cast" priority] |
| 113 | Bedouin Outrider | tribal light-horse scout/raider; all factions' auxiliaries | flowing head-cloth and mantle, no armor, light lance, small hide buckler, lean pony | 45/65/30/40/40/85 (305) | [NEW "Harass and Fade" flinch], [NEW "Mirage Veil" protect], Measured Blow |
| 114 | Kurdish Ghazw Rider | spear-armed tribal cavalry of Nur ad-Din/Saladin, 1160s-87 | jawshan lamellar over khaftan, wound-turban cap, round shield, spear over-arm, saif at hip | 60/75/35/60/50/55 (335) | Sundering Blow, [NEW "Spear of the Hills"], Zealous Cry |
| 115 | Naffatun Sapper | naphtha-pot thrower of the siege parks; Zengid/Ayyubid | padded hood, wet rawhide apron, clay grenade cradled, torch held low; calm face | 50/45/70/55/60/40 (320) | [NEW "Comet of Naphtha" Brand], [NEW "Wet Hide Ward"], Litany of Stillness |
| 116 | Askari Alamdar | banner-bearer of a lord's askar; the tulb's rally point | maille + lamellar half-suit, regimental banner (faction accent) planted, saif low | 75/60/40/70/65/45 (355) | [NEW "Hold the Banner" buff], Bulwark's Stand, Measured Blow |
| 117 | Muqaddam of the Tulb | officer of a 70-200-man tulb; mini-boss tier | full lamellar cuirass over maille, khud helm + nasal, mace raised, saif sheathed, barded horse | 85/85/45/80/60/50 (405) | [NEW "Wheel the Line"], Couched Lance, Sundering Blow, Hold the Banner |

117's 405 rides the COMMANDER band by design - a boss fight minus the named
portrait. Names are transliterated office/unit terms, ASCII-clean: never a
slur, never mock-Arabic fantasy.

**Lore register (chronicle voice: reverent, plain, masculine - samples):**

- 108: "He loosed from the saddle and was gone before the shaft fell; the plain itself seemed to ride against us."
- 115: "He carried fire in clay and walked through arrows as a man walks through rain; burnt, we hold him no less in honor."
- 117: "He wheeled his tulb as a shepherd turns one flock; our left horn was spent before the ninth hour."

## 5. Commander / boss candidates (ids 120-123)

Historically honest portraits: the honor is real where chroniclers of BOTH sides record it, and so are the hard deeds. [R] = rejected later romance.

| id | Commander | Node + engagement | Fight register |
|---|---|---|---|
| 120 | Kilij Arslan I | Node 1 - Dorylaeum, 1 July 1097 | ambush waves; summons Bowriders |
| 121 | Nur ad-Din | Node 3 - the Harim winter, 1164 | disciplined combined arms |
| 122 | Asad al-Din Shirkuh | Node 4 - the roads of Egypt, 1164-69 | march/counter-march speed |
| 123 | Saladin | Nodes 5-7 - Hattin 1187 / Arsuf 1191 / Jaffa 1192 | three-battle finale |

- **Kilij Arslan I (Sultan of Rum, r. 1092-1107).** Beaten at Nicaea, then
  nearly destroyed Bohemond's vanguard at Dorylaeum before relief arrived;
  rebuilt and crushed the crusade of 1101. The hard, intelligent border
  prince: ideal first boss; the horse-archer lesson.
- **Nur ad-Din Mahmud b. Zengi (atabeg of Aleppo and Damascus, d. 1174).**
  Son of the taker of Edessa (1144); reunified Syria, crushed Antioch's
  field army at Harim (1164), sent Shirkuh into Egypt while raiding to mask
  him. Both sides' sources record his austerity, justice, and night prayer.
- **Asad al-Din Shirkuh (d. 1169).** Marched into Egypt three times against
  Frankish intervention, took Alexandria, died within two months of the
  vizierate - clearing the stage for his nephew Saladin. A boss of marches,
  not set pieces.
- **Saladin (Salah al-Din Yusuf b. Ayyub, d. 1193).** Kurdish officer's son
  risen to vizier of Egypt, unifier of Egypt and Syria; victor of Hattin
  (4 July 1187) - the kingdom's field army destroyed - recoverer of
  Jerusalem that October. Hostile Latin chronicles still record his keeping
  of safe-conducts; his execution of the sworn truce-breaker Reynald of
  Chatillon is recorded plainly; the chivalrous exchanges with Richard I at
  Jaffa were famous within a generation, and Dante set him among the
  virtuous in Limbo. Scott's accretions [R] we ignore.

Sub-boss texture: Gokbori and Taki al-Din Umar (Saladin's wings at Hattin),
both well attested, flavor the Hattin node.

## 6. THE REGISTER SECTION (revised 2026-10-16; binds every follow-on card)

The sin-gauntlet is allegory; the historical lane is PEOPLE - and the
game's theological voice is now explicit: these men fight for a false god,
and the merciful work of the crusade is to call them to repentance. These
rules bind every downstream card; the register is "chronicle-firm, not
cruel", grounded in docs/DOCTRINAL-SINS.md.

1. **Redeemable men, always.** Every adversary is a man Christ died for.
   The fallen foe is grave, whole; the chronicle's verbs for him are
   "yield", "stand", "hold" - never "gibber". Conversion of any common
   archetype is always mechanically possible (weighted by Obstinacy), and
   a converted foe's baptism is written as joy, not conquest candy.
   Contrast is carried by KIT and by sin-depth portrayal (rule 4), not by
   ugliness.
2. **Naming rule.** Our narrator voice uses the names the men used
   themselves: "the Sultanate of Rum", "the atabeg Nur ad-Din", "Ayyubid
   tulbs". Period exonyms (e.g. "Saracen") live only inside quotation
   marks, attributed to a named character or source.
3. **Theological firmness without slander.** The narrator may say plainly,
   in chronicle voice, that the foe's god is a false god and that his
   doctrine errs (per docs/DOCTRINAL-SINS.md, always citing the doctrinal
   point, e.g. denial of the Son, works-righteousness). Banned without
   exception: groveling, treachery-by-birth, "heathen horde" framing,
   ethnic mockery, mock-Arabic naming, visual racial caricature in prompts.
   The game's own side is not above rebuke - the chroniclers confessed
   crusader greed at 1099; our codex does too.
4. **Sin-depth visual register (new, owner direction).** Portrayal darkens
   as a foe's entrenchment deepens: common soldiers and converts are drawn
   as dignified men; elite zealots and sin-boss-adjacent commanders carry
   visible corruption (shadowed eyes, ashen palette drift within the
   LOCKED 16-color palette's existing slots if possible). This visual
   gradient mirrors the Obstinacy stat - see docs/CONVERSION.md - and
   must read as spiritual condition, never ethnicity or kit slander.
5. **Codex of record.** Clearing an archetype or defeating a commander
   unlocks a real-history page ("The Roll of Worthy Adversaries",
   Hagiography third book, attested matters marked against [R] romance,
   unlocked via a new additive settings key `campaign.foes_defeated`
   u32 bitmask - the 14 foe ids exceed a u8). Converting an archetype
   additionally unlocks a **Testimonies page**: the man's account of
   repentance - the emotional heart of the collection loop.
6. **Canon amended.** PLAN.md section 1 now permits honorable historical
   portrayals of deceased combatants of 1095-1192 under this register.

## 7. Mode / integration proposal

| Option | What | For | Against |
|---|---|---|---|
| (a) "The Road East" gauntlet | 7-node campaign parallel to the sin gauntlet: Dorylaeum, Antioch-road, Harim, Egypt-roads, Hattin, Arsuf, Jaffa | best chronicle arc; reuses the persistent-damage ladder | new campaign shell wiring |
| (b) Rotating practice foes | fork client.rs::PRACTICE_FOE_SPECIES (today [3,8,10]) to draw from 108-117 | cheapest taste: one const + bounds test | no story, no unlocks |
| (c) Scenario mode per battle | set-pieces with scripted openers | most educational | new mode machinery; v2.x |

RECOMMENDATION: **(a) for the v2.0 content drop; (b) as a v1.2 taste (two lines); (c) deferred past v2.0.**

Seams (add-only, zero churn): keep `crusade-core/src/data/species.rs`
LOCKED (its tests assert exactly 19 entries at ids 1..=12 + 101..=107) and
add `crusade-core/src/data/adversaries.rs` = `pub static ADVERSARIES:
&[SpeciesBase]`, ids 108..=117 + 120..=123; `data::species_by_id` widens to
scan both tables (ids < 108 unchanged). `Typing::Valor` is a new enum arm -
serde-forwards-only; old builds never meet it; new rites 43..=57 likewise
additive. **Persistence is untouched**: instances serialize species_id u16
only; no store migration; SINS_DEFEATED_KEY, ladder, wagers blind to this
lane. Rewards: baptism pays Firstfruits Shards, soul-release pays Mercy
Shards (CONVERSION.md sections 4-5); no third shard type (owner, 2026-10-16).

## 8. Art-production implications

Pipeline unchanged from docs/ART-STATUS.md: nano-banana-2-1 via PayPerQ at
$0.00, quant.py onto the LOCKED 16-color palette, stage_sprites.py + an
ADVERSARIES table, right-facing at 128x128. Faction accents use EXISTING
palette slots - ink #0B0E1B Zengid standarts, palegold #F4D35E the Ayyubid
yellow attested for Saladin's banners, red #A62639 for Rum; no palette
mutation. Section 4's "arms / armor" column IS the per-archetype wardrobe
list, ready for the SUBJECTS prompt table; descriptor seed for 108: "a lean
horseman at the gallop on a small pony, fur-trimmed kalpak cap over a
wrapped turban, belted khaftan, composite bow drawn, dust rising; dark-navy
background; face calm and dignified; pixel sprite facing right". Commanders
stay static boss sprites per the sin-boss rule.

Backlog: 10 archetype sprites + 4 commander portraits + derived 32x32 icons
+ one "road east" scene header = ~15 gen cells, $0 lane total; gpt-image-
1.5 alternate = 15 x $0.023 ~ $0.35 contingency. Human time: one
prompt/eyeball/retry pass; budget one weekend worst case. Boss-face tiles
and codex head-crops reuse the same 128x128 masters via shipped D3/4.7
seams.

## 9. Roadmap slotting (docs/V1-SHIP-PLAN.md + PLAN.md)

v1.0 shipped; v1.1 (PvP) feature-complete per ART-STATUS section 5; v1.2 is
wagers/cashu; v1.3 is mobile signing/SDK. THIS work seeds **v2.0 "content
live-ops"**: HF-1 data tables (adversaries.rs + rites 43+) + fuzz pass;
HF-2 art lane (~15 cells, $0); HF-3 Road East wiring (client.rs, view.rs,
screens.rs, battle.rs - serialize after any in-flight shell card); HF-4 the
Roll of Worthy Adversaries (mirrors 4.7). Optional v1.2 taste after HF-1:
rotate one adversary id into PRACTICE_FOE_SPECIES - two lines, no new UI.

## 10. Open questions for the owner — RESOLVED 2026-10-16

1. Historical figures ban amended: YES (PLAN.md section 1 rewritten).
2. Typing: `Typing::Valor` arm (1.0x both ways) - ADOPTED, landed in M7.
3. Register: "Saracen" may ship ONLY in clearly attributed quotation
   (named character/source speech). Resolved: quotation OK.
4. Rewards: Firstfruits Shards (baptism) + Mercy Shards (soul-release),
   per CONVERSION.md 4-5. The "Commendation" name is retired.
5. Conversion: common archetypes 108-117 convertible (the core loop);
   commanders NEVER. Resolved per CONVERSION.md.
6. Jerusalem line: codex prose only - confirmed.
7. Boss nodes REUSE campaign persistent damage (gauntlet discipline).
   Resolved: persistent damage.

## 11. Sources (attested basis noted per item; [R] = rejected later romance)

- Battle of Dorylaeum (1097): [Wikipedia](https://en.wikipedia.org/wiki/Battle_of_Dorylaeum_(1097)) (modern 6-7k estimate vs the chroniclers' 150-360k); [EBSCO Research Starters](https://www.ebsco.com/research-starters/military-history-and-science/battle-dorylaeum); [French History Podcast ch. 10](https://www.thefrenchhistorypodcast.com/77-chapter-10-the-battle-of-dorylaeum) (Danishmend alliance).
- Battle of Hattin: [Wikipedia](https://en.wikipedia.org/wiki/Battle_of_Hattin) (~40k incl. 12k regular cavalry; wings under Gokbori, Taki al-Din Umar); [US Army CGSC thesis, DTIC](https://apps.dtic.mil/sti/pdfs/ADA331805.pdf) (tulb 70-200; askar; toassin/qaraghulam); [Four Accounts, De Re Militari](https://deremilitari.org/2014/01/the-battle-of-hattin-1187-four-accounts) (primary texts incl. Baha al-Din).
- Battle of Arsuf: [Wikipedia](https://en.wikipedia.org/wiki/Battle_of_Arsuf); [Medievalists.net](https://www.medievalists.net/2022/01/battle-richard-saladin-arsuf) (provocation tactics vs Richard's discipline; the Hospitallers' premature charge).
- Battle of Jaffa 1192: [Wikipedia](https://en.wikipedia.org/wiki/Battle_of_Jaffa_(1192)); [HistoryNet](https://historynet.com/richard-lionheart-battle-jaffa-1192) (55-80 knights + ~2,000 crossbowmen; kneeling spear-hedge vs ~7,000 cavalry).
- Zengids: [Zengid dynasty, Wikipedia](https://en.wikipedia.org/wiki/Zengid_dynasty) (army composition; 1-3k askar; cites Irwin, Oxford History of the Crusades); [Race for Egypt 1163-1169, World History Encyclopedia](https://www.worldhistory.org/article/1487/zengids--the-crusaders-race-for-egypt-1163-1169-ce) (Shirkuh thrice; death within months).
- Fatimids: [Fatimid army, Wikipedia](https://en.wikipedia.org/wiki/Fatimid_army) (ethnic corps; al-Maqrizi via Yaacov Lev; dissolved 1169); [A. Zouache, Blacks in the Fatimid Army (HAL)](https://hal.science/hal-02945046/file/Zouache_Blacks_in_the_Fatimid_Army_OK.pdf).
- Arms/armor: [Museum With No Frontiers](https://islamicart.museumwnf.org/database_item.php?id=object%3BISL%3Bjo%3BMus01%3B25%3Ben) (lamellar over linen, burda surcoat); [Introduction to Islamic Arms/Armours, ResearchGate](https://www.researchgate.net/publication/307683215_Introduction_to_the_Study_of_Islamic_Arms_and_Armours) (straight vs curved; deep curve from 15th c. AD); [Swordis, the Saif](https://swordis.com/blog/arabian-sword) (vendor blog corroborating: blades curve via Turkic influence from al-Mu'tasim's day; kilij is Ottoman).
- Helmets/tech: [David Nicolle, Military Technology of Classical Islam, extract mirrors](https://warfare.6te.net/Nicolle-Islam-crossbows.htm); [Nicolle, Edinburgh PhD thesis vol. II](https://era.ed.ac.uk/bitstream/handle/1842/7432/331246_vol2.pdf?sequence=2) (khud and baydah helmets; the standard authority); [Turban Helmets, Ilkhanid-Safavid (Academia.edu)](https://www.academia.edu/107237564/Turban_Helmets_from_the_Il_khanid_Period_to_the_Safavid_Era) (bulbous turban-helm = 14th-16th c.; banned here).
- Saladin in the Latin West: [Medievalists.net](https://www.medievalists.net/2014/09/saladin-good-pr-medieval-west) (William of Tyre, Jacques de Vitry, William of Newburgh); [Evolution of the Saladin Legend in the West (Academia.edu)](https://www.academia.edu/1496191/The_Evolution_of_the_Saladin_Legend_in_the_West) (Dante, Boccaccio); [J. Phillips, T&F 2024](https://www.tandfonline.com/doi/full/10.1080/14765276.2024.2360549).
- Period voice: [William of Tyre, Historia (Fordham Medieval Sourcebook)](https://sourcebooks.web.fordham.edu/source/tyre-cde.asp) - our quotation mine.
- [R] rejected canon: Walter Scott's The Talisman (1825) and the 19th-c.
  "noble Saracen" costume tradition - kilijs, turban-helmets, antics.

Prepared 2026-10-15 by the research worker; this file only; no git.
